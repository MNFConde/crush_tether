//! Lua 脚本引擎（M6.1，mlua 0.12 / Lua 5.4 vendored）：[`super::RhaiEngine`]
//! 同一 [`super::RuleEngine`] trait 的第二实现（design.md「DSL 引擎（定稿）」）。
//!
//! - **沙箱**：`Lua::new_with` 安全模式 + 显式库白名单（coroutine / table /
//!   math / string / utf8——无 io / os / package / debug / ffi）；base 库中
//!   可触文件系统或污染 stdout 的 `dofile` / `loadfile` / `load` / `print`
//!   加载后置 nil（mlua 不代劳，自证清单见 [`sanitize_base`]）。
//! - **限流**（与 rhai `max_operations` 同语义映射）：**全局**指令数 hook
//!   （[`INSTRUCTION_BUDGET`]——`set_global_hook` 形态，主线程与脚本自建
//!   协程都被计数；超限 → 运行时错误 → 调用方 fail-safe confirm）+ 内存
//!   上限 [`MEMORY_LIMIT`]（OOM 防线）。死循环/深递归/OOM 尝试一律有界
//!   拦截。协程语义边界：`coroutine.resume` 类 pcall 吞协程内错误——
//!   超预算协程被终止（DoS 已阻）但脚本不报错、继续走到返回值（design.md
//!   更正登记 18）。
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

use super::{AllowActivation, ConfirmAs, ScriptCtx, ScriptDecision, ScriptError, ScriptOutcome};

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

/// 声明式规则条目（Lua 侧，`rule(名字, 优先级, 函数)` 注册产物）。
struct LuaRule {
    name: String,
    priority: i64,
    f: Function,
}

/// 声明式规则数上限（与 rhai 侧 [`super::MAX_RULES`] 同值）。
const MAX_RULES: usize = super::MAX_RULES;

/// Lua 引擎实例：编译缓存 + 原语闭包捕获的上下文。
pub struct LuaEngine {
    /// Lua 状态锚：`check` Function 绑定其状态，字段本身无需读取，但必须
    /// 随实例保活（mlua 对象不延长 state 生命周期，实测销毁即失效）。
    #[allow(dead_code)]
    lua: Lua,
    check: Function,
    /// 声明式规则条目（按优先级升序；空 = 旧 check 形态）。
    rules: Vec<LuaRule>,
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
        sanitize_base(&lua)?;

        // 全局指令数限流 hook：主线程与脚本自建协程都被计数（线程级
        // set_hook 只挂主线程，C 层 coroutine.create 不继承——协程会
        // 逃逸预算）；每次 evaluate 前 budget 归零，超限 → 运行时错误。
        let budget = Arc::new(AtomicU64::new(0));
        let hook_budget = budget.clone();
        lua.set_global_hook(
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

        register_primitives(&lua, project, kb)?;
        register_decision_table(&lua)?;
        register_allow(&lua, decls.clone())?;

        // 声明式规则注册器（M8.6）：`rule(名字, 优先级, 函数)` 引擎注入；
        // chunk 顶层执行本就发生（函数定义落全局），注册调用同批收集。
        // 重复名/空名/负优先级/超上限在注册边界报错 → chunk 执行失败 → 拒载。
        let rules: std::rc::Rc<std::cell::RefCell<Vec<LuaRule>>> = Default::default();
        {
            let reg = rules.clone();
            let f = lua
                .create_function(move |_, (name, priority, f): (String, i64, Function)| {
                    let name = name.trim().to_string();
                    if name.is_empty() {
                        return Err(mlua::Error::runtime("rule() name must not be empty"));
                    }
                    if priority < 0 {
                        return Err(mlua::Error::runtime(format!(
                            "rule(\"{name}\") priority must be >= 0"
                        )));
                    }
                    let mut list = reg.borrow_mut();
                    if list.iter().any(|r| r.name == name) {
                        return Err(mlua::Error::runtime(format!(
                            "duplicate rule name `{name}`"
                        )));
                    }
                    if list.len() >= MAX_RULES {
                        return Err(mlua::Error::runtime(format!(
                            "too many rules (limit {MAX_RULES})"
                        )));
                    }
                    list.push(LuaRule { name, priority, f });
                    Ok(())
                })
                .map_err(compile_err)?;
            lua.globals().set("rule", f).map_err(compile_err)?;
        }
        {
            let f = lua
                .create_function(|_, sub: String| {
                    let sub = sub.trim().to_string();
                    if sub.is_empty() {
                        return Err(mlua::Error::runtime(
                            "confirm_as() sub-name must not be empty",
                        ));
                    }
                    Ok(ConfirmAs(sub))
                })
                .map_err(compile_err)?;
            lua.globals().set("confirm_as", f).map_err(compile_err)?;
        }

        // 顶层语句执行一次（函数定义落全局；`rule()` 注册同批收集）；
        // 语法错误在此暴露。
        let chunk: Function = lua
            .load(source)
            .set_name("rules.lua")
            .into_function()
            .map_err(|e| ScriptError::Compile(e.to_string()))?;
        // 顶层执行错误（含 rule() 注册边界与限流）→ 加载期语义拒载；
        // 语法错误已在 into_function 阶段以 Compile 暴露。
        chunk
            .call::<()>(())
            .map_err(|e| ScriptError::Rejected(format!("script top-level failed: {e}")))?;
        let check: Function = lua.globals().get("check").unwrap_or_else(|_| {
            // 占位：无 check 的声明式脚本由下方校验放行，占位函数不会被调用。
            lua.create_function(|_, ()| Ok(()))
                .expect("placeholder function")
        });

        let mut collected: Vec<LuaRule> = std::mem::take(&mut *rules.borrow_mut());
        if collected.is_empty() {
            // 兼容形态：无注册规则 → 必须有 `check(ctx)` 入口。
            let has_check = lua
                .globals()
                .get::<Option<Function>>("check")
                .ok()
                .flatten()
                .is_some();
            if !has_check {
                return Err(ScriptError::Rejected(
                    "rules.lua must define `check(ctx)` or register rules via \
                     `rule(name, priority, fn)`"
                        .into(),
                ));
            }
        } else {
            // 稳定排序：同优先级保注册顺序（= 定义顺序）。
            collected.sort_by_key(|r| r.priority);
        }

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
            rules: collected,
            budget,
            decls,
            allow_literals: extracted,
        })
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
        if self.rules.is_empty() {
            // 兼容形态：单 `check(ctx)` 入口（机制不变，M8.6 双形态并存）。
            let result: Value = self
                .check
                .call(ctx)
                .map_err(|e| ScriptError::Runtime(e.to_string()))?;
            return parse_return(result, "check", false);
        }
        // 声明式形态：按优先级序逐规则调用——PASS（不表态）交下一个，
        // 表态（confirm/deny/激活）短路；指令预算跨规则共享（更严侧）。
        for r in &self.rules {
            let result: Value =
                r.f.call(ctx.clone())
                    .map_err(|e| ScriptError::Runtime(format!("rule `{}`: {e}", r.name)))?;
            let outcome = parse_return(result, &r.name, true)?;
            if !matches!(outcome, ScriptOutcome::Pass) {
                return Ok(outcome);
            }
        }
        Ok(ScriptOutcome::Pass)
    }
}

/// 脚本返回值 → 评估产出（与 rhai 侧 [`super::parse_return`] 同语义）；
/// `unit` = 执行单元名、`named` = 该单元是否携带溯源名。
fn parse_return(result: Value, unit: &str, named: bool) -> Result<ScriptOutcome, ScriptError> {
    match result {
        // nil = PASS（词汇约定：Lua 侧映射 nil 等价）。
        Value::Nil => Ok(ScriptOutcome::Pass),
        Value::UserData(u) => {
            if let Ok(a) = u.borrow::<AllowActivation>() {
                return Ok(ScriptOutcome::Activate(a.0.clone()));
            }
            if let Ok(c) = u.borrow::<ConfirmAs>() {
                return Ok(ScriptOutcome::Adjust(
                    Decision::Confirm,
                    Some(format!("{unit}:{}", c.0)),
                ));
            }
            if let Ok(d) = u.borrow::<ScriptDecision>() {
                return match *d {
                    ScriptDecision::Pass => Ok(ScriptOutcome::Pass),
                    ScriptDecision::Allow => Err(ScriptError::Contract(
                        "scripts cannot return a bare `allow` (use the declared \
                         allow(\"bin\") channel; bare values cannot be reconciled)"
                            .into(),
                    )),
                    ScriptDecision::Confirm => Ok(ScriptOutcome::Adjust(
                        Decision::Confirm,
                        named.then(|| unit.to_string()),
                    )),
                    ScriptDecision::Deny => Ok(ScriptOutcome::Adjust(
                        Decision::Deny,
                        named.then(|| unit.to_string()),
                    )),
                };
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
            let d = ScriptDecision::parse(&s).ok_or_else(|| {
                ScriptError::Contract(format!(
                    "check() returned `{s}`; expected one of nil, confirm, deny, \
                     allow(\"bin\")"
                ))
            })?;
            match d {
                ScriptDecision::Pass => Ok(ScriptOutcome::Pass),
                ScriptDecision::Allow => Err(ScriptError::Contract(
                    "scripts cannot return a bare `allow` (use the declared \
                     allow(\"bin\") channel; bare values cannot be reconciled)"
                        .into(),
                )),
                ScriptDecision::Confirm => Ok(ScriptOutcome::Adjust(
                    Decision::Confirm,
                    named.then(|| unit.to_string()),
                )),
                ScriptDecision::Deny => Ok(ScriptOutcome::Adjust(
                    Decision::Deny,
                    named.then(|| unit.to_string()),
                )),
            }
        }
        _ => Err(ScriptError::Contract(
            "check() must return a decision value".into(),
        )),
    }
}

/// mlua 注册类错误 → 脚本编译失败（注册发生在全新 VM，失败只可能是资源
/// 类异常；按 Compile 类别向上传播，不 panic）。
fn compile_err(e: mlua::Error) -> ScriptError {
    ScriptError::Compile(e.to_string())
}

/// base 库消毒：危险全局置 nil（`load`/`loadfile`/`dofile`/`print`）。
/// `require`/`io`/`os`/`package` 本就不在库白名单内，无需处理。
fn sanitize_base(lua: &Lua) -> Result<(), ScriptError> {
    for name in DANGEROUS_BASE_GLOBALS {
        lua.globals().set(*name, Value::Nil).map_err(compile_err)?;
    }
    Ok(())
}

/// 注册 Rust 侧安全原语：纯函数、无 IO；知识库数据源经 `Arc` 共享只读
/// 事实（与 rhai 侧 [`super::RhaiEngine`] 同一函数集、同一语义）。
fn register_primitives(
    lua: &Lua,
    project: PathBuf,
    kb: Option<Arc<KnowledgeBase>>,
) -> Result<(), ScriptError> {
    let p = project.clone();
    lua.globals()
        .set(
            "path_escapes",
            lua.create_function(move |_, word: String| {
                Ok(crate::cmd_parse::path_escapes(&word, &p))
            })
            .map_err(compile_err)?,
        )
        .map_err(compile_err)?;
    lua.globals()
        .set(
            "inside_repo",
            lua.create_function(move |_, word: String| {
                Ok(crate::cmd_parse::inside_repo(&word, &project))
            })
            .map_err(compile_err)?,
        )
        .map_err(compile_err)?;

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
            .map_err(compile_err)?,
        )
        .map_err(compile_err)?;
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
            .map_err(compile_err)?,
        )
        .map_err(compile_err)?;
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
            .map_err(compile_err)?,
        )
        .map_err(compile_err)?;
    let k = kb.clone();
    lua.globals()
        .set(
            "kb_known",
            lua.create_function(move |_, bin: String| {
                Ok(k.as_ref().is_some_and(|k| k.bins.contains_key(&bin)))
            })
            .map_err(compile_err)?,
        )
        .map_err(compile_err)?;
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
            .map_err(compile_err)?,
        )
        .map_err(compile_err)?;
    lua.globals()
        .set(
            "kb_present",
            lua.create_function(move |_, ()| Ok(kb.is_some()))
                .map_err(compile_err)?,
        )
        .map_err(compile_err)?;
    Ok(())
}

/// 注册全局 `decision` 表：四常量为 [`ScriptDecision`] userdata（构造封闭，
/// 脚本无法拼出第四种决策值；`__eq` 按变体比较见 UserData impl）。
fn register_decision_table(lua: &Lua) -> Result<(), ScriptError> {
    let t = lua.create_table().map_err(compile_err)?;
    t.set("ALLOW", ScriptDecision::Allow).map_err(compile_err)?;
    t.set("CONFIRM", ScriptDecision::Confirm)
        .map_err(compile_err)?;
    t.set("DENY", ScriptDecision::Deny).map_err(compile_err)?;
    t.set("PASS", ScriptDecision::Pass).map_err(compile_err)?;
    lua.globals().set("decision", t).map_err(compile_err)
}

/// 机制 3：运行时双保险——`allow(name)` 执行时再校验 name ∈ 声明集；
/// 未声明 → 运行时错误 → 调用方 fail-safe confirm。
fn register_allow(lua: &Lua, decls: ScriptAllowDecls) -> Result<(), ScriptError> {
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
        .map_err(compile_err)?;
    lua.globals().set("allow", f).map_err(compile_err)
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

impl UserData for ConfirmAs {}

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
