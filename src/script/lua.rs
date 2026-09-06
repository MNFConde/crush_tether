//! Lua 脚本引擎（M6.1，mlua 0.12 / Lua 5.4 vendored）：[`super::RhaiEngine`]
//! 同一 [`super::RuleEngine`] trait 的第二实现（design.md「DSL 引擎（定稿）」）。
//!
//! - **沙箱**：`Lua::new_with` 安全模式 + 显式库白名单（coroutine / table /
//!   math / string / utf8——无 io / os / package / debug / ffi）；base 库中
//!   可触文件系统或污染 stdout 的 `dofile` / `loadfile` / `load` / `print`
//!   加载后置 nil（mlua 不代劳，自证清单见 [`sanitize_base`]）。
//! - **限流**（与 rhai `max_operations` 同语义映射）：指令数 hook
//!   （[`INSTRUCTION_BUDGET`]，超限 → 运行时错误 → 调用方 fail-safe
//!   confirm）+ 内存上限 [`MEMORY_LIMIT`]（对齐 rhai 字符串/数组上限的
//!   OOM 防线）。死循环/深递归/OOM 尝试一律有界拦截。
//! - **词汇约定**：与 rhai 同一封装类型——ctx 传 [`super::ScriptCtx`]
//!   userdata（只读字段 `ctx.bin` 等），决策值 [`super::ScriptDecision`]
//!   userdata（全局 `decision` 表四常量；`__eq` 按变体比较）。返回值
//!   `nil` = PASS（design.md 词汇约定「Lua 侧映射 nil」验收点）；
//!   userdata 决策值按变体映射；裸字符串经 [`super::ScriptDecision::parse`]
//!   双保险解析；其他类型一律契约违约。
//! - **script_allow**：运行时 `allow(name)` 原语对账（机制 3，与 rhai 同
//!   语义）；加载期字面量提取（机制 1）与声明集对账（机制 2）见
//!   [`extract_allow_literals`]。定稿点（逃逸检查/deny 终审）引擎无关，
//!   在 [`super::finalize`] 复用。
//! - 顶层语句在编译期执行一次（函数定义落全局），`check(ctx)` 缺失 →
//!   编译期拒载；chunk 编译缓存于实例（serve 复用同一实例）。

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use mlua::{
    AnyUserData, Function, HookTriggers, Lua, LuaOptions, MetaMethod, StdLib, UserData,
    UserDataFields, UserDataMethods, Value, VmState,
};

use crate::cmd_parse::SimpleCommand;
use crate::config::merge::ScriptAllowDecls;
use crate::knowledge::KnowledgeBase;
use crate::model::Decision;

use super::{AllowActivation, ScriptCtx, ScriptDecision, ScriptError, ScriptOutcome};

/// 指令预算（对齐 rhai `set_max_operations(100_000)` 同量级；hook 每
/// [`INSTRUCTION_CHECK_INTERVAL`] 条指令核对一次）。
const INSTRUCTION_BUDGET: u64 = 200_000;
/// hook 触发间隔（条指令）——间隔越小限流越精确、开销越大，取千条折中。
const INSTRUCTION_CHECK_INTERVAL: u32 = 1_000;
/// 内存上限（整个 VM；对齐 rhai 字符串/数组上限组合出的 OOM 防线量级）。
const MEMORY_LIMIT: usize = 16 * 1024 * 1024;

/// base 库中需要移除的危险全局（mlua safe 模式不代劳）：
/// `dofile`/`loadfile` 触文件系统、`load` 动态编代码（混淆面）、
/// `print` 污染 stdout（check/hook 协议通道）。
const DANGEROUS_BASE_GLOBALS: &[&str] = &["dofile", "loadfile", "load", "print"];

/// Lua 引擎实例：编译缓存 + 原语闭包捕获的上下文。
pub struct LuaEngine {
    /// Lua 状态锚：`check` Function 绑定其状态，字段本身无需读取，但必须
    /// 随实例保活（mlua 对象不延长 state 生命周期，实测销毁即失效）。
    #[allow(dead_code)]
    lua: Lua,
    check: Function,
    /// 指令预算计数器（每次 evaluate 归零；hook 闭包持有同一 Arc）。
    budget: Arc<AtomicU64>,
    /// 声明集副本（定稿点作用域化逃逸检查的判据）。
    decls: ScriptAllowDecls,
    /// 机制 1 提取的 `allow("…")` 字面量集。
    allow_literals: Vec<String>,
}

impl LuaEngine {
    /// 编译脚本并装配沙箱（库白名单 + 限流 + 原语注册）；编译错误在此
    /// 暴露。`decls` 为 `rules.toml` `script_allow` 声明集（机制 2 对账 +
    /// 机制 3 运行时校验 + 定稿点作用域判据）。
    pub fn compile(
        source: &str,
        project: PathBuf,
        kb: Option<Arc<KnowledgeBase>>,
        decls: ScriptAllowDecls,
    ) -> Result<Self, ScriptError> {
        let lua = Lua::new_with(
            StdLib::COROUTINE | StdLib::TABLE | StdLib::MATH | StdLib::STRING | StdLib::UTF8,
            LuaOptions::new(),
        )
        .map_err(|e| ScriptError::Compile(e.to_string()))?;
        lua.set_memory_limit(MEMORY_LIMIT)
            .map_err(|e| ScriptError::Compile(e.to_string()))?;
        sanitize_base(&lua);

        // 指令数限流 hook：每次 evaluate 前 budget 归零，超限 → 运行时错误。
        let budget = Arc::new(AtomicU64::new(0));
        let hook_budget = budget.clone();
        lua.set_hook(
            HookTriggers::new().every_nth_instruction(INSTRUCTION_CHECK_INTERVAL),
            move |_, _| {
                let used =
                    hook_budget.fetch_add(u64::from(INSTRUCTION_CHECK_INTERVAL), Ordering::Relaxed);
                if used >= INSTRUCTION_BUDGET {
                    Err(mlua::Error::runtime(
                        "script exceeded instruction budget (limit throttling)",
                    ))
                } else {
                    Ok(VmState::Continue)
                }
            },
        )
        .map_err(|e| ScriptError::Compile(e.to_string()))?;

        register_primitives(&lua, project, kb);
        register_decision_table(&lua)?;
        register_allow(&lua, decls.clone());

        // 顶层语句执行一次（函数定义落全局）；语法错误在此暴露。
        let chunk: Function = lua
            .load(source)
            .set_name("rules.lua")
            .into_function()
            .map_err(|e| ScriptError::Compile(e.to_string()))?;
        chunk
            .call::<()>(())
            .map_err(|e| ScriptError::Compile(e.to_string()))?;
        let check: Function = lua
            .globals()
            .get("check")
            .map_err(|_| ScriptError::Compile("rules.lua must define `check(ctx)`".into()))?;

        // 机制 1：加载期字面量提取（非字面量实参 → 拒载）；机制 2：声明集
        // 对账——提取集 − 声明集 ≠ ∅ → 拒载。
        let extracted = extract_allow_literals(source)?;
        for name in &extracted {
            if decls.scope_of(name).is_none() {
                return Err(ScriptError::Rejected(format!(
                    "script calls allow(\"{name}\") but `{name}` is not declared in \
                     rules.toml `script_allow`; declarations are the only source of \
                     allow activations"
                )));
            }
        }

        Ok(Self {
            lua,
            check,
            budget,
            decls,
            allow_literals: extracted,
        })
    }

    /// 声明集（定稿点作用域化逃逸检查用）。
    pub fn decls(&self) -> &ScriptAllowDecls {
        &self.decls
    }

    /// 提取集（机制 1 产物；lint 死声明检查与 load 事件行的脚本侧数据源）。
    pub fn allow_literals(&self) -> &[String] {
        &self.allow_literals
    }
}

impl super::RuleEngine for LuaEngine {
    fn allow_literals(&self) -> &[String] {
        &self.allow_literals
    }

    fn decls(&self) -> &ScriptAllowDecls {
        &self.decls
    }

    fn evaluate(
        &self,
        cmd: &SimpleCommand,
        verdict: Decision,
        project: &std::path::Path,
        pipe_to_shell: bool,
    ) -> Result<ScriptOutcome, ScriptError> {
        self.budget.store(0, Ordering::Relaxed);
        let ctx = ScriptCtx::new(cmd, verdict, project, pipe_to_shell);
        let result: Value = self
            .check
            .call(ctx)
            .map_err(|e| ScriptError::Runtime(e.to_string()))?;
        match result {
            // nil = PASS（词汇约定：Lua 侧映射 nil 等价）。
            Value::Nil => Ok(ScriptOutcome::Pass),
            Value::UserData(u) => {
                if let Ok(d) = u.borrow::<ScriptDecision>() {
                    return super::script_outcome_of(*d);
                }
                if let Ok(a) = u.borrow::<AllowActivation>() {
                    return Ok(ScriptOutcome::Activate(a.0.clone()));
                }
                Err(ScriptError::Contract(
                    "check() must return a decision value".into(),
                ))
            }
            // 双保险：等价裸字符串在返回边界统一解析。
            Value::String(s) => {
                let s = s
                    .to_str()
                    .map_err(|_| ScriptError::Contract("check() returned invalid UTF-8".into()))?;
                super::script_outcome_of(ScriptDecision::parse(&s).ok_or_else(|| {
                    ScriptError::Contract(format!(
                        "check() returned `{s}`; expected one of nil, confirm, deny, \
                         allow(\"bin\")"
                    ))
                })?)
            }
            _ => Err(ScriptError::Contract(
                "check() must return a decision value".into(),
            )),
        }
    }
}

/// base 库消毒：危险全局置 nil（`load`/`loadfile`/`dofile`/`print`）。
/// `require`/`io`/`os`/`package` 本就不在库白名单内，无需处理。
fn sanitize_base(lua: &Lua) {
    for name in DANGEROUS_BASE_GLOBALS {
        let _ = lua.globals().set(*name, Value::Nil);
    }
}

/// 注册 Rust 侧安全原语：纯函数、无 IO；知识库数据源经 `Arc` 共享只读
/// 事实（与 rhai 侧 [`super::RhaiEngine`] 同一函数集、同一语义）。
fn register_primitives(lua: &Lua, project: PathBuf, kb: Option<Arc<KnowledgeBase>>) {
    let p = project.clone();
    lua.globals()
        .set(
            "path_escapes",
            lua.create_function(move |_, word: String| {
                Ok(crate::cmd_parse::path_escapes(&word, &p))
            })
            .expect("register path_escapes"),
        )
        .expect("set path_escapes");
    lua.globals()
        .set(
            "inside_repo",
            lua.create_function(move |_, word: String| {
                Ok(crate::cmd_parse::inside_repo(&word, &project))
            })
            .expect("register inside_repo"),
        )
        .expect("set inside_repo");

    let k = kb.clone();
    lua.globals()
        .set(
            "kb_write_tokens",
            lua.create_function(move |_, (bin, sub): (String, String)| {
                Ok(k.as_ref()
                    .and_then(|k| k.bins.get(&bin))
                    .and_then(|e| e.subs.get(&sub))
                    .and_then(|e| e.write_tokens.as_ref())
                    .cloned()
                    .unwrap_or_default())
            })
            .expect("register kb_write_tokens"),
        )
        .expect("set kb_write_tokens");
    let k = kb.clone();
    lua.globals()
        .set(
            "kb_write_arg_count",
            lua.create_function(move |_, (bin, sub): (String, String)| {
                Ok(k.as_ref()
                    .and_then(|k| k.bins.get(&bin))
                    .and_then(|e| e.subs.get(&sub))
                    .and_then(|e| e.write_arg_count)
                    .unwrap_or(0))
            })
            .expect("register kb_write_arg_count"),
        )
        .expect("set kb_write_arg_count");
    let k = kb.clone();
    lua.globals()
        .set(
            "kb_may_write",
            lua.create_function(move |_, bin: String| {
                Ok(k.as_ref()
                    .and_then(|k| k.bins.get(&bin))
                    .and_then(|e| e.may_write)
                    .unwrap_or(false))
            })
            .expect("register kb_may_write"),
        )
        .expect("set kb_may_write");
    let k = kb.clone();
    lua.globals()
        .set(
            "kb_known",
            lua.create_function(move |_, bin: String| {
                Ok(k.as_ref().is_some_and(|k| k.bins.contains_key(&bin)))
            })
            .expect("register kb_known"),
        )
        .expect("set kb_known");
    let k = kb.clone();
    lua.globals()
        .set(
            "kb_irreversible",
            lua.create_function(move |_, (bin, flag): (String, String)| {
                Ok(k.as_ref()
                    .and_then(|k| k.bins.get(&bin))
                    .and_then(|e| e.flags.get(&flag))
                    .and_then(|f| f.irreversible)
                    .unwrap_or(false))
            })
            .expect("register kb_irreversible"),
        )
        .expect("set kb_irreversible");
    lua.globals()
        .set(
            "kb_present",
            lua.create_function(move |_, ()| Ok(kb.is_some()))
                .expect("register kb_present"),
        )
        .expect("set kb_present");
}

/// 注册全局 `decision` 表：四常量为 [`ScriptDecision`] userdata（构造封闭，
/// 脚本无法拼出第四种决策值；`__eq` 按变体比较见 UserData impl）。
fn register_decision_table(lua: &Lua) -> Result<(), ScriptError> {
    let t = lua
        .create_table()
        .map_err(|e| ScriptError::Compile(e.to_string()))?;
    t.set("ALLOW", ScriptDecision::Allow)
        .map_err(|e| ScriptError::Compile(e.to_string()))?;
    t.set("CONFIRM", ScriptDecision::Confirm)
        .map_err(|e| ScriptError::Compile(e.to_string()))?;
    t.set("DENY", ScriptDecision::Deny)
        .map_err(|e| ScriptError::Compile(e.to_string()))?;
    t.set("PASS", ScriptDecision::Pass)
        .map_err(|e| ScriptError::Compile(e.to_string()))?;
    lua.globals()
        .set("decision", t)
        .map_err(|e| ScriptError::Compile(e.to_string()))
}

/// 机制 3：运行时双保险——`allow(name)` 执行时再校验 name ∈ 声明集；
/// 未声明 → 运行时错误 → 调用方 fail-safe confirm。
fn register_allow(lua: &Lua, decls: ScriptAllowDecls) {
    let f = lua
        .create_function(move |_, name: String| {
            if decls.scope_of(&name).is_some() {
                Ok(AllowActivation(name))
            } else {
                Err(mlua::Error::runtime(format!(
                    "allow(\"{name}\") rejected: `{name}` is not declared in rules.toml \
                     `script_allow`"
                )))
            }
        })
        .expect("register allow");
    lua.globals().set("allow", f).expect("set allow");
}

/// 机制 1（Lua 侧，加载期字面量提取）：保守扫描脚本源中 `allow("…")` /
/// `allow('…')` 调用的实参字面量。与 rhai 的 AST 提取相比语义收窄（登记
/// design.md 更正登记）：Lua 无公开 AST，扫描按词法近似——
/// - 实参是带引号字面量 → 收集；
/// - `allow(` 后第一个非空白字符不是引号 → 拒载（动态名不可静态对账，
///   与 rhai 同语义）；
/// - 极端形态（多行括号、注释内调用）宁可漏收不误拒：漏收的调用由机制 3
///   运行时对账兜底（未声明照样拒），审计面不缩小。
fn extract_allow_literals(source: &str) -> Result<Vec<String>, ScriptError> {
    let stripped = strip_lua_comments(source);
    let source = stripped.as_str();
    let mut out = Vec::new();
    let bytes = source.as_bytes();
    let mut i = 0;
    while let Some(pos) = source[i..].find("allow") {
        let at = i + pos;
        i = at + "allow".len();
        // 词边界：`allow` 前后都不能是标识符字符（排除 `my_allow`/`allowx`）。
        if at > 0 && is_ident_byte(bytes[at - 1]) {
            continue;
        }
        // `allow` 之后必须紧跟可选空白 + `(`，排除 `allowx` 等标识符。
        let rest = &source[i..];
        let trimmed = rest.trim_start();
        let paren = i + (rest.len() - trimmed.len());
        if !trimmed.starts_with('(') {
            continue;
        }
        // 括号后第一个非空白字符必须是引号（字面量实参）。
        let after = &source[paren + 1..];
        let lead = after.len() - after.trim_start().len();
        let arg_at = paren + 1 + lead;
        let Some(&quote) = bytes.get(arg_at) else {
            continue;
        };
        if quote != b'"' && quote != b'\'' {
            return Err(ScriptError::Rejected(
                "`allow()` must be called with a string literal bin name (dynamic names \
                 are not statically reconcilable)"
                    .into(),
            ));
        }
        let Some(end) = source[arg_at + 1..].find(quote as char) else {
            return Err(ScriptError::Rejected(
                "`allow()` literal argument is not terminated".into(),
            ));
        };
        out.push(source[arg_at + 1..arg_at + 1 + end].to_string());
        i = arg_at + 1 + end + 1;
    }
    Ok(out)
}

fn is_ident_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

/// 剥离 Lua 注释（行注释 `-- …` 与块注释 `--[[ … ]]`，替换为空格保序）：
/// 注释里被注掉的 `allow("x")` 不得参与机制 1 提取（否则误拒整个脚本）。
/// 已知边界：字符串实参内含 `--` 会被误剥为注释——后果是**漏收**该调用
/// （不误拒），机制 3 运行时对账仍拦截未声明名，审计面不缩小。
fn strip_lua_comments(source: &str) -> String {
    let b = source.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'-' && i + 1 < b.len() && b[i + 1] == b'-' {
            let rest = &source[i + 2..];
            let long = rest.trim_start_matches('-');
            if let Some(long) = long.strip_prefix("[[") {
                // 块注释：--[[ … ]]（一级等号；嵌套级未识别时按行注释保守处理）。
                if let Some(end) = long.find("]]") {
                    let consumed = 2 + (rest.len() - long.len() - 2) + 2 + end + 2;
                    out.extend(std::iter::repeat_n(b' ', consumed));
                    i += consumed;
                    continue;
                }
            }
            while i < b.len() && b[i] != b'\n' {
                out.push(b' ');
                i += 1;
            }
            continue;
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

// ── mlua 类型桥接（与 rhai 侧注册同一套封装类型） ─────────────────────

impl UserData for ScriptCtx {
    fn add_fields<F: UserDataFields<Self>>(fields: &mut F) {
        fields.add_field_method_get("bin", |_, c| Ok(c.bin.clone()));
        fields.add_field_method_get("sub", |_, c| Ok(c.sub.clone()));
        fields.add_field_method_get("words", |_, c| Ok(c.words.clone()));
        fields.add_field_method_get("args", |_, c| Ok(c.args.clone()));
        fields.add_field_method_get("verdict", |_, c| Ok(c.verdict));
        fields.add_field_method_get("writes_redirect", |_, c| Ok(c.writes_redirect));
        fields.add_field_method_get("pipe_to_shell", |_, c| Ok(c.pipe_to_shell));
        fields.add_field_method_get("project", |_, c| Ok(c.project.clone()));
    }
}

impl UserData for ScriptDecision {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        // ctx.verdict == decision.ALLOW：按变体比较（Lua 的 __eq 只在两侧
        // 同为 userdata 时触发；异型 userdata 借用失败视为不等，不报错）。
        methods.add_meta_method(MetaMethod::Eq, |_, a: &ScriptDecision, b: AnyUserData| {
            let matched = b
                .borrow::<ScriptDecision>()
                .map(|d| *a == *d)
                .unwrap_or(false);
            Ok(matched)
        });
    }
}

impl UserData for AllowActivation {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extraction_collects_literals_and_rejects_dynamic_names() {
        let ok = extract_allow_literals(concat!(
            "function check(ctx)\n",
            "  if ctx.bin == \"ls\" then return allow(\"ls\") end\n",
            "  return allow('docker')\n",
            "end\n",
        ))
        .unwrap();
        assert!(ok.contains(&"ls".to_string()));
        assert!(ok.contains(&"docker".to_string()));

        assert!(extract_allow_literals("function check(ctx) return allow(ctx.bin) end").is_err());
        // 词边界：my_allow / allowx 不算调用。
        let decoy = "local my_allow = 1\nfunction check(ctx) return nil end\n";
        assert!(extract_allow_literals(decoy).unwrap().is_empty());
    }

    #[test]
    fn commented_out_allow_is_not_extracted() {
        // 行注释与块注释里的调用不参与提取（误拒防护）。
        let src = concat!(
            "-- allow(\"curl\")\n",
            "--[[\n",
            "return allow(\"wget\")\n",
            "]]\n",
            "function check(ctx) return nil end\n",
        );
        assert!(extract_allow_literals(src).unwrap().is_empty());
    }
}
