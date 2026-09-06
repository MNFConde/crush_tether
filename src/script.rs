//! 脚本层引擎（Rhai，v1）：沙箱执行 `rules.rhai`，承载声明层表达不了的
//! 条件判断（design.md「DSL 引擎（定稿）」）。
//!
//! - 同一 [`RuleEngine`] trait 是引擎开闭落点（`--engine lua` 随 P6 落地）。
//! - Rust 提供**不可绕过的安全原语**（`path_escapes` / `inside_repo` /
//!   `kb_*` 知识库数据源），DSL 只能组合判定、不能绕过；沙箱默认无
//!   文件/进程/网络 API。
//! - 限流（定稿要求）：`max_operations` / `max_call_levels` /
//!   `max_expr_depths` + 字符串/数组上限——死循环或 OOM 尝试被有界拦截。
//! - 脚本契约：入口 `fn check(ctx) -> string`，返回 `decision::PASS`（即
//!   空串，无意见，保留查表裁决）/ `"allow"` / `"confirm"` / `"deny"`（等价
//!   词汇 `decision::` 常量见 [`decision`]）；其他返回值、编译错误、
//!   运行时错误、限流触发一律由调用方映射为 fail-safe confirm。
//! - **script_allow（M4.0，受控放行）**：脚本只能激活用户在 `rules.toml`
//!   `script_allow` 声明过的 bin——`allow("bin")` 原语 + 加载期字面量提取 /
//!   声明集对账拒载 / 运行时双保险 / 定稿点作用域化逃逸检查（[`finalize`]）
//!   / deny 终审（design.md「脚本条件放行（script_allow，定稿）」五件套）。
//!   裸 `"allow"` 字符串仍是契约违约。
//! - AST 编译一次缓存于实例（check 每次全量、serve 复用同一实例）。

use std::path::{Path, PathBuf};
use std::sync::Arc;

use rhai::{ASTNode, Dynamic, Engine, EvalAltResult, Expr, Position, Stmt};

use crate::cmd_parse::SimpleCommand;
use crate::config::merge::{DeclScope, ScriptAllowDecls};
use crate::knowledge::KnowledgeBase;
use crate::model::Decision;

/// 本构建支持的脚本层引擎名（`--engine`；Lua 随 P6）。
pub const SUPPORTED_ENGINES: &[&str] = &["rhai"];

/// 脚本层失败原因（任何变体 → 调用方 fail-safe confirm）。
#[derive(Debug)]
pub enum ScriptError {
    /// 脚本文件读取失败。
    Io(std::io::Error),
    /// rhai 编译失败。
    Compile(String),
    /// 加载期语义拒载（机制 1/2）：`allow()` 实参非字面量、声明集对账失败。
    /// 与 Compile 同为「脚本整体不可用」——拒载整个脚本 + fail-safe confirm。
    Rejected(String),
    /// 运行期错误（含限流触发）。
    Runtime(String),
    /// 脚本返回了契约之外的值。
    Contract(String),
}

impl std::fmt::Display for ScriptError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ScriptError::Io(e) => write!(f, "io error: {e}"),
            ScriptError::Compile(e) => write!(f, "compile error: {e}"),
            ScriptError::Rejected(e) => write!(f, "script rejected: {e}"),
            ScriptError::Runtime(e) => write!(f, "runtime error: {e}"),
            ScriptError::Contract(e) => write!(f, "contract violation: {e}"),
        }
    }
}

impl std::error::Error for ScriptError {}

/// 脚本词汇约定（design.md「脚本词汇约定」，M4.0 前置小改落地）：
/// 决策值 = 只读模块常量 `decision::` 四值；`PASS` = 无意见（不表态、
/// 交还查表基线），Rhai 侧映射空串（Lua 引擎映射 nil，P6）。
/// 脚本可用限定名 `decision::ALLOW` 等，也可写等价裸字符串——最终校验
/// 在引擎（双保险），变量遮蔽污染不了引擎侧词汇表。
pub mod decision {
    /// 放行（仅经 `allow("bin")` 带名通道，裸字符串违约）。
    pub const ALLOW: &str = "allow";
    /// 升级为人工确认。
    pub const CONFIRM: &str = "confirm";
    /// 阻断。
    pub const DENY: &str = "deny";
    /// 无意见（交还查表基线；Rhai 侧空串）。
    pub const PASS: &str = "";
}

/// 把 `decision::` 四常量以只读静态模块注入脚本（限定名访问）。
fn register_decision_module(engine: &mut Engine) {
    let mut m = rhai::Module::new();
    m.set_var("ALLOW", decision::ALLOW);
    m.set_var("CONFIRM", decision::CONFIRM);
    m.set_var("DENY", decision::DENY);
    m.set_var("PASS", decision::PASS);
    engine.register_static_module("decision", m.into());
}

/// `allow("bin")` 的激活标记（自定义类型）：只能由 `allow` 原语构造，作为
/// `check()` 返回值流入定稿点；脚本无法伪造（裸字符串 "allow" 仍契约违约）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AllowActivation(pub String);

/// 脚本引擎抽象（引擎开闭落点；Rhai 为 v1 默认实现）。
pub trait RuleEngine {
    /// 单命令评估。`pipe_to_shell` 为管线原语计算的管道拓扑特征（整条
    /// 命令行级）。`allow(name)` 激活只产出 [`ScriptOutcome::Activate`]，
    /// 放行与否由定稿点（[`finalize`]）裁决——脚本自身无放行权。
    fn evaluate(
        &self,
        cmd: &SimpleCommand,
        verdict: Decision,
        project: &Path,
        pipe_to_shell: bool,
    ) -> Result<ScriptOutcome, ScriptError>;
}

/// 单命令脚本评估结果（管线第 2 步产出，第 3 步定稿点消费）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScriptOutcome {
    /// 无意见（`decision::PASS`）：保留查表裁决。
    Pass,
    /// 脚本升级裁决（confirm / deny）；定稿点对查表 deny 终审不可翻。
    Adjust(Decision),
    /// `allow(name)` 激活：放行面 = 用户声明集 ∩ 脚本条件命中，由定稿点
    /// 按声明作用域元数据复查后放行。
    Activate(String),
}

/// Rhai 引擎实例：`Engine` + 编译缓存的 AST + 原语闭包捕获的上下文。
pub struct RhaiEngine {
    engine: Engine,
    ast: rhai::AST,
    /// 声明集副本（定稿点作用域化逃逸检查的判据）。
    decls: ScriptAllowDecls,
    /// 机制 1 提取的 `allow("…")` 字面量集。
    allow_literals: Vec<String>,
}

impl RhaiEngine {
    /// 编译脚本并装配沙箱（限流 + 原语注册）；编译错误在此暴露。
    /// `decls` 为 `rules.toml` `script_allow` 声明集（机制 2 对账 + 机制 3
    /// 运行时校验 + 定稿点作用域判据）。
    pub fn compile(
        source: &str,
        project: PathBuf,
        kb: Option<Arc<KnowledgeBase>>,
        decls: ScriptAllowDecls,
    ) -> Result<Self, ScriptError> {
        let mut engine = Engine::new();
        // 限流（防死循环 / 深递归 / OOM）。
        engine.set_max_operations(100_000);
        engine.set_max_call_levels(64);
        engine.set_max_expr_depths(32, 32);
        engine.set_max_array_size(1_000);
        engine.set_max_string_size(10_000);

        register_primitives(&mut engine, project, kb);
        register_decision_module(&mut engine);
        register_allow(&mut engine, decls.clone());

        let ast = engine
            .compile(source)
            .map_err(|e| ScriptError::Compile(e.to_string()))?;

        // 机制 1：加载期 AST 字面量提取（非字面量实参 → 拒载）。
        let extracted = extract_allow_literals(&ast)?;
        // 机制 2：声明集对账——提取集 − 声明集 ≠ ∅ → 拒载（脚本作者无法
        // 替用户决定放行谁）。
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
            engine,
            ast,
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

impl RuleEngine for RhaiEngine {
    fn evaluate(
        &self,
        cmd: &SimpleCommand,
        verdict: Decision,
        project: &Path,
        pipe_to_shell: bool,
    ) -> Result<ScriptOutcome, ScriptError> {
        let mut scope = rhai::Scope::new();
        let ctx = build_context(cmd, verdict, project, pipe_to_shell);
        let result: Dynamic = self
            .engine
            .call_fn(&mut scope, &self.ast, "check", (ctx,))
            .map_err(|e| ScriptError::Runtime(e.to_string()))?;
        // 激活标记（allow 原语构造的自定义类型）→ 交给定稿点。
        if let Some(a) = result.clone().try_cast::<AllowActivation>() {
            return Ok(ScriptOutcome::Activate(a.0));
        }
        let s = result
            .into_string()
            .map_err(|_| ScriptError::Contract("check() must return a decision value".into()))?;
        match s.as_str() {
            "" => Ok(ScriptOutcome::Pass),
            // allow 契约：放行必须走带名通道（allow("bin")，引擎才有的对账）；
            // 裸 "allow" 字符串仍是契约违约，fail-safe confirm。
            "allow" => Err(ScriptError::Contract(
                "scripts cannot return a bare `allow` (use the declared allow(\"bin\") \
                 channel; bare strings cannot be reconciled)"
                    .into(),
            )),
            "confirm" => Ok(ScriptOutcome::Adjust(Decision::Confirm)),
            "deny" => Ok(ScriptOutcome::Adjust(Decision::Deny)),
            other => Err(ScriptError::Contract(format!(
                "check() returned `{other}`; expected one of \"\", confirm, deny, \
                 allow(\"bin\")"
            ))),
        }
    }
}

/// 机制 3：运行时双保险——`allow(name)` 执行时再校验 name ∈ 声明集；
/// 未声明 → 运行时错误 → 调用方 fail-safe confirm。
fn register_allow(engine: &mut Engine, decls: ScriptAllowDecls) {
    engine.register_type_with_name::<AllowActivation>("AllowActivation");
    engine.register_fn(
        "allow",
        move |name: &str| -> Result<AllowActivation, Box<EvalAltResult>> {
            if decls.scope_of(name).is_some() {
                Ok(AllowActivation(name.to_string()))
            } else {
                Err(Box::new(EvalAltResult::ErrorRuntime(
                    Dynamic::from(format!(
                        "allow(\"{name}\") rejected: `{name}` is not declared in rules.toml \
                     `script_allow`"
                    )),
                    Position::NONE,
                )))
            }
        },
    );
}

/// 定稿点（design.md 筛查管线第 3 步）：全引擎唯一的放行出口。
///
/// - **deny 终审**：查表落 deny 的命令，脚本任何产出都翻不动（不可逆操作
///   不给任何机制留放行通道）；
/// - **allow(name) 激活**：按声明作用域元数据复查——local 声明对原始命令
///   参数执行 `path_escapes`（与查表层同一实现），逃逸 → confirm；global
///   声明豁免（两表皆现 global 胜已在声明集 `scope_of` 体现）。检查在引擎、
///   单点、脚本不可绕过也不可代劳（脚本自查通过照样再查）；
/// - 查表 allow 上的激活为幂等 no-op。
///
/// 返回（最终裁决，原因说明——裁决日志 reason 字段数据源）。
pub fn finalize(
    initial: Decision,
    outcome: ScriptOutcome,
    decls: &ScriptAllowDecls,
    cmd: &SimpleCommand,
    project: &Path,
) -> (Decision, Option<String>) {
    match outcome {
        ScriptOutcome::Pass => (initial, None),
        ScriptOutcome::Adjust(d) => {
            if initial == Decision::Deny {
                (
                    Decision::Deny,
                    Some("script outcome discarded: deny is final".into()),
                )
            } else {
                (d, Some("adjusted by rules.rhai".into()))
            }
        }
        ScriptOutcome::Activate(name) => {
            if initial == Decision::Deny {
                return (
                    Decision::Deny,
                    Some(format!(
                        "allow(\"{name}\") activation discarded: deny is final"
                    )),
                );
            }
            match decls.scope_of(&name) {
                Some(DeclScope::Global) => (
                    Decision::Allow,
                    Some(format!("allow(\"{name}\") activated (global declaration)")),
                ),
                Some(DeclScope::Local) => {
                    if cmd
                        .args()
                        .iter()
                        .any(|w| crate::cmd_parse::path_escapes(w, project))
                    {
                        (
                            Decision::Confirm,
                            Some(format!(
                                "allow(\"{name}\") activation downgraded: path escapes \
                                 repository"
                            )),
                        )
                    } else {
                        (
                            Decision::Allow,
                            Some(format!(
                                "allow(\"{name}\") activated (local declaration; args \
                                 inside repo)"
                            )),
                        )
                    }
                }
                // 双保险：机制 3 已在运行时拒绝未声明名；此分支到不了则保守。
                None => (
                    Decision::Confirm,
                    Some("allow activation without declaration; fail-safe".into()),
                ),
            }
        }
    }
}

/// 机制 1（加载期字面量提取）：AST 静态提取脚本中全部 `allow("…")` 调用的
/// 实参字面量。校验对象是语法树不是运行值——实参不是字符串字面量（变量 /
/// 拼接 / 循环变量）→ 拒载。遍历用 rhai 官方 `AST::walk`（含函数体；rhai
/// `internals` feature，随锁版钉死），个别 AST 内部变体不下潜时由机制 3
/// 运行时双保险兜底。
fn extract_allow_literals(ast: &rhai::AST) -> Result<Vec<String>, ScriptError> {
    let mut out: Vec<String> = Vec::new();
    let mut rejected: Option<ScriptError> = None;
    ast.walk(&mut |path: &[ASTNode]| {
        let Some(node) = path.last() else {
            return true;
        };
        // `allow("…")` 的两种出现形态：表达式（`return allow(...)` / 参数位）
        // 或语句级 FnCall（单函数调用成句的专用变体——含 `return allow(...)`
        // 被优化器折叠后的形态——其自身不再作为 Expr 节点被访问）。
        let fc = match node {
            ASTNode::Expr(Expr::FnCall(fc, _) | Expr::MethodCall(fc, _)) => fc,
            ASTNode::Stmt(Stmt::FnCall(fc, _)) => fc,
            _ => return true,
        };
        if fc.name != "allow" {
            return true;
        }
        match fc.args.first() {
            // 字面量名 → 收集进提取集。
            Some(Expr::StringConstant(s, _)) => {
                out.push(s.to_string());
                true
            }
            // 非字面量实参 → 拒载（终止遍历）。
            _ => {
                rejected = Some(ScriptError::Rejected(
                    "`allow()` must be called with a string literal bin name (dynamic \
                     names are not statically reconcilable)"
                        .into(),
                ));
                false
            }
        }
    });
    match rejected {
        Some(e) => Err(e),
        None => Ok(out),
    }
}

/// 构造脚本可见的命令上下文（只读特征；原始词元，归一不在此层）。
/// rhai 的 Dynamic 对字符串取 owned 拷贝，不与命令借用纠缠。
fn build_context(
    cmd: &SimpleCommand,
    verdict: Decision,
    project: &Path,
    pipe_to_shell: bool,
) -> rhai::Map {
    let mut m = rhai::Map::new();
    let words: Vec<Dynamic> = cmd.words.iter().map(|w| Dynamic::from(w.clone())).collect();
    let args: Vec<Dynamic> = cmd
        .args()
        .iter()
        .map(|w| Dynamic::from(w.clone()))
        .collect();
    m.insert(
        "bin".into(),
        Dynamic::from(cmd.bin().unwrap_or("").to_string()),
    );
    m.insert(
        "sub".into(),
        // ctx 可选字段缺省 = 空字符串（词汇约定）：不向脚本暴露解释器
        // 内部的 unit/nil 语义；谓词写 `ctx.sub != ""`。
        Dynamic::from(cmd.args().first().cloned().unwrap_or_default()),
    );
    m.insert("words".into(), words.into());
    m.insert("args".into(), args.into());
    m.insert("verdict".into(), Dynamic::from(verdict.to_string()));
    m.insert("writes_redirect".into(), Dynamic::from(cmd.writes_redirect));
    m.insert("pipe_to_shell".into(), Dynamic::from(pipe_to_shell));
    m.insert(
        "project".into(),
        Dynamic::from(project.to_string_lossy().to_string()),
    );
    m
}

/// 注册 Rust 侧安全原语：纯函数、无 IO；知识库数据源经 `Arc` 共享只读事实。
fn register_primitives(engine: &mut Engine, project: PathBuf, kb: Option<Arc<KnowledgeBase>>) {
    // 路径原语：与判定层同一实现（词法归一 + 仓库边界）。
    let project_a = project.clone();
    engine.register_fn("path_escapes", move |word: &str| -> bool {
        crate::cmd_parse::path_escapes(word, &project_a)
    });
    engine.register_fn("inside_repo", move |word: &str| -> bool {
        crate::cmd_parse::inside_repo(word, &project)
    });

    // 知识库数据源（删光 → 空数组/0，脚本查不到数据走各自兜底分支）。
    let kb_tokens = kb.clone();
    engine.register_fn(
        "kb_write_tokens",
        move |bin: &str, sub: &str| -> rhai::Array {
            kb_tokens
                .as_ref()
                .and_then(|k| k.bins.get(bin))
                .and_then(|e| e.subs.get(sub))
                .and_then(|e| e.write_tokens.as_ref())
                .map(|tokens| tokens.iter().map(|t| Dynamic::from(t.clone())).collect())
                .unwrap_or_default()
        },
    );
    let kb_arg = kb.clone();
    engine.register_fn("kb_write_arg_count", move |bin: &str, sub: &str| -> i64 {
        kb_arg
            .as_ref()
            .and_then(|k| k.bins.get(bin))
            .and_then(|e| e.subs.get(sub))
            .and_then(|e| e.write_arg_count)
            .map(|c| i64::try_from(c).unwrap_or(i64::MAX))
            .unwrap_or(0)
    });
    let kb_may = kb.clone();
    engine.register_fn("kb_may_write", move |bin: &str| -> bool {
        kb_may
            .as_ref()
            .and_then(|k| k.bins.get(bin))
            .and_then(|e| e.may_write)
            .unwrap_or(false)
    });
    // bin 级知识存在性：供自定义脚本做覆盖判断。
    let kb_known = kb.clone();
    engine.register_fn("kb_known", move |bin: &str| -> bool {
        kb_known.as_ref().is_some_and(|k| k.bins.contains_key(bin))
    });
    // 知识库整体在位性：默认脚本的两态谓词兜底条件（删光 → confirm）。
    let kb_flag = kb.clone();
    engine.register_fn("kb_irreversible", move |bin: &str, flag: &str| -> bool {
        kb_flag
            .as_ref()
            .and_then(|k| k.bins.get(bin))
            .and_then(|e| e.flags.get(flag))
            .and_then(|f| f.irreversible)
            .unwrap_or(false)
    });
    // 知识库整体在位性：默认脚本的两态谓词兜底条件（删光 → confirm）。
    let kb_present = kb;
    engine.register_fn("kb_present", move || -> bool { kb_present.is_some() });
}

/// 脚本层链（design.md「配置拆分」：脚本层同文件按优先级，项目脚本最后
/// 执行可作最终裁决）：用户层先、项目层后，前一层输出为后一层输入。
/// 层标签用于日志溯源（`script.file` 区分两层文件）。
pub struct ScriptChain {
    /// 依执行序排列（user → project）。
    engines: Vec<(&'static str, RhaiEngine)>,
}

impl ScriptChain {
    /// 依层序评估整条链；任一层出错整体 `Err`（调用方 fail-safe confirm）。
    /// 返回（最终裁决，生效裁决所在层标签——激活或改判的层，
    /// 累积的最后一个原因说明）。
    pub fn evaluate(
        &self,
        cmd: &SimpleCommand,
        initial: Decision,
        project: &Path,
        pipe_to_shell: bool,
    ) -> Result<(Decision, Option<&'static str>, Option<String>), ScriptError> {
        let mut current = initial;
        let mut layer: Option<&'static str> = None;
        let mut reason = None;
        for (tag, engine) in &self.engines {
            let outcome = engine.evaluate(cmd, current, project, pipe_to_shell)?;
            let activate = matches!(outcome, ScriptOutcome::Activate(_));
            let (d, r) = finalize(current, outcome, engine.decls(), cmd, project);
            if activate || d != current {
                layer = Some(tag);
            }
            if r.is_some() {
                reason = r;
            }
            current = d;
        }
        Ok((current, layer, reason))
    }

    /// 全链 `allow("…")` 字面量并集（lint 死声明检查的数据源）。
    pub fn allow_literals(&self) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        for (_, e) in &self.engines {
            for lit in e.allow_literals() {
                if !out.contains(lit) {
                    out.push(lit.clone());
                }
            }
        }
        out
    }
}

/// 加载脚本层链：用户层 `~/.config/crush-tether/rules.rhai` 先、项目层
/// `.crush-tether/rules.rhai` 后（缺失 = 跳过该层；两层皆缺 = None，TOML
/// 自足）。任一层损坏（含 script_allow 对账拒载）→ `Err` → fail-safe
/// confirm。
pub fn load_script_chain(
    project: &Path,
    home: Option<&Path>,
    kb: Option<Arc<KnowledgeBase>>,
    decls: ScriptAllowDecls,
) -> Result<Option<ScriptChain>, ScriptError> {
    let mut engines = Vec::new();
    if let Some(h) = home {
        let path = h.join(".config").join("crush-tether").join("rules.rhai");
        if let Some(e) =
            load_optional_engine(&path, project.to_path_buf(), kb.clone(), decls.clone())?
        {
            engines.push(("user", e));
        }
    }
    let path = project.join(".crush-tether").join("rules.rhai");
    if let Some(e) = load_optional_engine(&path, project.to_path_buf(), kb, decls)? {
        engines.push(("project", e));
    }
    Ok((!engines.is_empty()).then(|| ScriptChain { engines }))
}

/// 单层脚本加载：不存在（NotFound）→ None；读取失败或编译/对账拒载 → Err。
fn load_optional_engine(
    path: &Path,
    project: PathBuf,
    kb: Option<Arc<KnowledgeBase>>,
    decls: ScriptAllowDecls,
) -> Result<Option<RhaiEngine>, ScriptError> {
    match std::fs::read_to_string(path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(ScriptError::Io(e)),
        Ok(source) => RhaiEngine::compile(&source, project, kb, decls).map(Some),
    }
}

/// `--engine` 参数校验（v1 仅 rhai；lua 随 P6）。
pub fn engine_supported(name: &str) -> bool {
    SUPPORTED_ENGINES.contains(&name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cmd_parse::flatten_commands;
    use std::path::PathBuf;

    fn cmd(s: &str) -> SimpleCommand {
        flatten_commands(s)
            .expect("parses")
            .into_iter()
            .next()
            .unwrap()
    }

    fn compile(src: &str) -> RhaiEngine {
        RhaiEngine::compile(
            src,
            PathBuf::from("D:/code/tmp/proj"),
            None,
            ScriptAllowDecls::default(),
        )
        .expect("compiles")
    }

    const PROJ: &str = "D:/code/tmp/proj";

    #[test]
    fn script_returns_decision_or_no_opinion() {
        let e = compile(concat!(
            "fn check(ctx) {",
            "  if ctx.bin == \"sudo\" { \"deny\" }",
            "  else if ctx.bin == \"rm\" { \"confirm\" }",
            "  else { \"\" }",
            "}"
        ));
        assert_eq!(
            e.evaluate(&cmd("sudo x"), Decision::Allow, Path::new(PROJ), false)
                .unwrap(),
            ScriptOutcome::Adjust(Decision::Deny)
        );
        assert_eq!(
            e.evaluate(&cmd("rm x"), Decision::Allow, Path::new(PROJ), false)
                .unwrap(),
            ScriptOutcome::Adjust(Decision::Confirm)
        );
        assert_eq!(
            e.evaluate(&cmd("ls"), Decision::Allow, Path::new(PROJ), false)
                .unwrap(),
            ScriptOutcome::Pass
        );
    }

    #[test]
    fn script_can_read_context_fields() {
        let e = compile(concat!(
            "fn check(ctx) {",
            "  if ctx.verdict == \"allow\" && ctx.writes_redirect { \"confirm\" }",
            "  else if ctx.words[0] == \"git\" && ctx.args[0] == \"push\" { \"deny\" }",
            "  else { \"\" }",
            "}"
        ));
        assert_eq!(
            e.evaluate(&cmd("git push"), Decision::Allow, Path::new(PROJ), false)
                .unwrap(),
            ScriptOutcome::Adjust(Decision::Deny)
        );
    }

    #[test]
    fn infinite_loop_is_bounded_by_operation_limit() {
        let e = compile("fn check(ctx) { while true {} }");
        let r = e.evaluate(&cmd("ls"), Decision::Allow, Path::new(PROJ), false);
        assert!(r.is_err(), "限流必须把死循环变成 Err → 上层 confirm");
    }

    #[test]
    fn deep_recursion_is_bounded() {
        let e = compile("fn f(x) { f(x) } fn check(ctx) { f(1) }");
        assert!(
            e.evaluate(&cmd("ls"), Decision::Allow, Path::new(PROJ), false)
                .is_err()
        );
    }

    #[test]
    fn sandbox_has_no_host_api() {
        // 未注册的「越权 API」在运行时不可达 → Err → 上层 confirm。
        let e = compile("fn check(ctx) { open(\"/etc/passwd\"); \"allow\" }");
        assert!(matches!(
            e.evaluate(&cmd("ls"), Decision::Allow, Path::new(PROJ), false),
            Err(ScriptError::Runtime(_))
        ));
        // 语法错误在编译期暴露。
        let src = "fn check(ctx) { let = ; }";
        assert!(matches!(
            RhaiEngine::compile(src, PathBuf::from(PROJ), None, ScriptAllowDecls::default()),
            Err(ScriptError::Compile(_))
        ));
    }

    #[test]
    fn contract_violation_on_non_decision_string() {
        let e = compile("fn check(ctx) { \"maybe\" }");
        assert!(matches!(
            e.evaluate(&cmd("ls"), Decision::Allow, Path::new(PROJ), false),
            Err(ScriptError::Contract(_))
        ));
    }

    #[test]
    fn primitives_are_composable_and_kb_aware() {
        let kb_src = concat!(
            "version = 1\n",
            "[git]\n",
            "sub.branch = { write_tokens = [\"-d\", \"-D\"] }\n",
            "sub.config = { write_arg_count = 2 }\n",
            "flag.\"--hard\" = { irreversible = true }\n",
            "[npx]\n",
            "may_write = true\n",
        );
        let kb = Arc::new(KnowledgeBase::parse_toml(kb_src).expect("kb parses"));
        let e = RhaiEngine::compile(
            concat!(
                "fn check(ctx) {",
                "  if ctx.bin == \"git\" && ctx.args[0] == \"branch\" {",
                "    for t in kb_write_tokens(\"git\", \"branch\") { if ctx.args.contains(t) { return \"confirm\"; } }",
                "  }",
                "  let n = kb_write_arg_count(\"git\", \"config\");",
                "  if ctx.bin == \"git\" && ctx.args[0] == \"config\" && n > 0 && ctx.args.len() - 1 >= n { return \"confirm\"; }",
                "  if kb_irreversible(\"git\", \"--hard\") && ctx.args.contains(\"--hard\") { return \"deny\"; }",
                "  if kb_may_write(\"npx\") && ctx.bin == \"npx\" { return \"confirm\"; }",
                "  if path_escapes(\"../outside\") && ctx.args.contains(\"../outside\") { return \"confirm\"; }",
                "  \"\"",
                "}"
            ),
            PathBuf::from(PROJ),
            Some(kb),
            ScriptAllowDecls::default(),
        )
        .expect("compiles");
        assert_eq!(
            e.evaluate(
                &cmd("git branch -d x"),
                Decision::Allow,
                Path::new(PROJ),
                false
            )
            .unwrap(),
            ScriptOutcome::Adjust(Decision::Confirm)
        );
        assert_eq!(
            e.evaluate(&cmd("git branch"), Decision::Allow, Path::new(PROJ), false)
                .unwrap(),
            ScriptOutcome::Pass
        );
        assert_eq!(
            e.evaluate(
                &cmd("git config a b"),
                Decision::Allow,
                Path::new(PROJ),
                false
            )
            .unwrap(),
            ScriptOutcome::Adjust(Decision::Confirm)
        );
        assert_eq!(
            e.evaluate(
                &cmd("git config --list"),
                Decision::Allow,
                Path::new(PROJ),
                false
            )
            .unwrap(),
            ScriptOutcome::Pass
        );
        assert_eq!(
            e.evaluate(
                &cmd("git reset --hard"),
                Decision::Allow,
                Path::new(PROJ),
                false
            )
            .unwrap(),
            ScriptOutcome::Adjust(Decision::Deny)
        );
        assert_eq!(
            e.evaluate(&cmd("npx foo"), Decision::Allow, Path::new(PROJ), false)
                .unwrap(),
            ScriptOutcome::Adjust(Decision::Confirm)
        );
        assert_eq!(
            e.evaluate(
                &cmd("cat ../outside"),
                Decision::Allow,
                Path::new(PROJ),
                false
            )
            .unwrap(),
            ScriptOutcome::Adjust(Decision::Confirm)
        );
        // 知识库删光：查不到数据 → 各兜底分支不触发 → 无意见。
        let e_empty = RhaiEngine::compile(
            "fn check(ctx) { if kb_write_tokens(\"git\", \"branch\").contains(\"-d\") { \"confirm\" } else { \"\" } }",
            PathBuf::from(PROJ),
            None,
            ScriptAllowDecls::default(),
        )
        .expect("compiles");
        assert_eq!(
            e_empty
                .evaluate(
                    &cmd("git branch -d x"),
                    Decision::Allow,
                    Path::new(PROJ),
                    false
                )
                .unwrap(),
            ScriptOutcome::Pass
        );
    }

    #[test]
    fn missing_script_file_is_none_not_error() {
        let r = load_script_chain(
            Path::new("D:/code/tmp/definitely-absent"),
            Some(Path::new("D:/code/tmp/definitely-absent")),
            None,
            ScriptAllowDecls::default(),
        )
        .unwrap();
        assert!(r.is_none(), "TOML 自足：两层脚本皆缺失不是错误");
    }

    #[test]
    fn allow_contract_rejects_script_allow() {
        // 裸 "allow" 字符串 = 契约违约（放行必须走 allow("bin") 带名通道）。
        let e = compile("fn check(ctx) { \"allow\" }");
        assert!(matches!(
            e.evaluate(&cmd("ls"), Decision::Allow, Path::new(PROJ), false),
            Err(ScriptError::Contract(_))
        ));
        // 升级权仍在：confirm / deny 合法。
        let e = compile(concat!(
            "fn check(ctx) {",
            "  if ctx.verdict == \"deny\" { \"deny\" } else { \"confirm\" }",
            "}"
        ));
        assert_eq!(
            e.evaluate(&cmd("ls"), Decision::Allow, Path::new(PROJ), false)
                .unwrap(),
            ScriptOutcome::Adjust(Decision::Confirm)
        );
        assert_eq!(
            e.evaluate(&cmd("sudo x"), Decision::Deny, Path::new(PROJ), false)
                .unwrap(),
            ScriptOutcome::Adjust(Decision::Deny)
        );
    }

    #[test]
    fn pipe_to_shell_flag_reaches_script() {
        // 管道拓扑由引擎原语计算，脚本只承载策略（谓词 3）。
        let e = compile(concat!(
            "fn check(ctx) {",
            "  if ctx.pipe_to_shell { \"deny\" } else { \"\" }",
            "}"
        ));
        assert_eq!(
            e.evaluate(&cmd("sh"), Decision::Allow, Path::new(PROJ), true)
                .unwrap(),
            ScriptOutcome::Adjust(Decision::Deny)
        );
        assert_eq!(
            e.evaluate(&cmd("sh"), Decision::Allow, Path::new(PROJ), false)
                .unwrap(),
            ScriptOutcome::Pass
        );
    }

    #[test]
    fn decision_module_constants_are_read_only_vocabulary() {
        // decision:: 四常量与裸字符串等价（引擎双保险；词汇约定见 design.md）。
        let e = compile(concat!(
            "fn check(ctx) {",
            "  if ctx.bin == \"sudo\" { decision::DENY }",
            "  else if ctx.bin == \"rm\" { decision::CONFIRM }",
            "  else { decision::PASS }",
            "}"
        ));
        assert_eq!(
            e.evaluate(&cmd("sudo x"), Decision::Allow, Path::new(PROJ), false)
                .unwrap(),
            ScriptOutcome::Adjust(Decision::Deny)
        );
        assert_eq!(
            e.evaluate(&cmd("rm x"), Decision::Allow, Path::new(PROJ), false)
                .unwrap(),
            ScriptOutcome::Adjust(Decision::Confirm)
        );
        assert_eq!(
            e.evaluate(&cmd("ls"), Decision::Allow, Path::new(PROJ), false)
                .unwrap(),
            ScriptOutcome::Pass,
            "PASS 映射无意见"
        );
        // 限定名是常量：脚本内重新赋值不得改变引擎侧词汇表。
        let e = compile("fn check(ctx) { let DENY = decision::ALLOW; DENY }");
        assert!(
            e.evaluate(&cmd("ls"), Decision::Allow, Path::new(PROJ), false)
                .is_err(),
        );
    }

    #[test]
    fn ctx_sub_defaults_to_empty_string_not_unit() {
        // ctx 可选字段缺省 = 空字符串（词汇约定）：不暴露解释器 unit 语义。
        let e = compile(concat!(
            "fn check(ctx) {",
            "  if ctx.sub == \"\" { decision::CONFIRM } else { decision::PASS }",
            "}"
        ));
        assert_eq!(
            e.evaluate(&cmd("ls"), Decision::Allow, Path::new(PROJ), false)
                .unwrap(),
            ScriptOutcome::Adjust(Decision::Confirm)
        );
        assert_eq!(
            e.evaluate(&cmd("git status"), Decision::Allow, Path::new(PROJ), false)
                .unwrap(),
            ScriptOutcome::Pass
        );
    }

    // ── script_allow 五件套 ──────────────────────────────────────────────

    fn decls(local: &[&str], global: &[&str]) -> ScriptAllowDecls {
        let mut d = ScriptAllowDecls::default();
        for b in local {
            d.declare_local(b);
        }
        for b in global {
            d.declare_global(b);
        }
        d
    }

    #[test]
    fn allow_activation_flows_to_outcome_for_declared_bin() {
        let e = RhaiEngine::compile(
            concat!(
                "fn check(ctx) {",
                "  if ctx.bin == \"ls\" && ctx.writes_redirect { return allow(\"ls\"); }",
                "  \"\"",
                "}"
            ),
            PathBuf::from(PROJ),
            None,
            decls(&["ls"], &[]),
        )
        .expect("compiles");
        assert_eq!(
            e.evaluate(
                &cmd("ls > out.txt"),
                Decision::Confirm,
                Path::new(PROJ),
                false
            )
            .unwrap(),
            ScriptOutcome::Activate("ls".into())
        );
        assert_eq!(
            e.evaluate(&cmd("ls"), Decision::Allow, Path::new(PROJ), false)
                .unwrap(),
            ScriptOutcome::Pass
        );
    }

    #[test]
    fn undeclared_allow_is_rejected_at_load_time() {
        // 机制 2：提取集 − 声明集 ≠ ∅ → 拒载。
        let r = RhaiEngine::compile(
            "fn check(ctx) { if ctx.bin == \"curl\" { return allow(\"curl\"); } \"\" }",
            PathBuf::from(PROJ),
            None,
            decls(&["ls"], &[]),
        );
        assert!(matches!(r, Err(ScriptError::Rejected(msg)) if msg.contains("curl")));
    }

    #[test]
    fn dynamic_allow_name_is_rejected_at_load_time() {
        // 机制 1：变量实参 → 拒载（校验语法树不是运行值）。
        for src in [
            "fn check(ctx) { let b = \"ls\"; return allow(b); }",
            "fn check(ctx) { for x in [\"a\", \"b\"] { return allow(x); } }",
        ] {
            let r =
                RhaiEngine::compile(src, PathBuf::from(PROJ), None, decls(&["ls", "curl"], &[]));
            assert!(matches!(r, Err(ScriptError::Rejected(_))), "{src}");
        }
        // 字符串拼接被优化器折叠为字面量 → 提取到折叠后的实参并与声明集
        // 对账：未声明 → 拒载（静态提取与运行值恒一致，审计面不缩小）。
        let r = RhaiEngine::compile(
            "fn check(ctx) { return allow(\"c\" + \"url\"); }",
            PathBuf::from(PROJ),
            None,
            decls(&["ls"], &[]),
        );
        assert!(matches!(r, Err(ScriptError::Rejected(msg)) if msg.contains("curl")));
    }

    #[test]
    fn deny_is_final_over_script_outcomes() {
        let cmd_deny = cmd("sudo x");
        let proj = Path::new(PROJ);
        let d = ScriptAllowDecls::default();
        // Adjust 在 deny 之上无效（终审）。
        let (v, reason) = finalize(
            Decision::Deny,
            ScriptOutcome::Adjust(Decision::Confirm),
            &d,
            &cmd_deny,
            proj,
        );
        assert_eq!(v, Decision::Deny);
        assert!(reason.unwrap().contains("deny is final"));
        // 激活在 deny 之上无效（终审）。
        let (v, reason) = finalize(
            Decision::Deny,
            ScriptOutcome::Activate("ls".into()),
            &decls(&["ls"], &[]),
            &cmd_deny,
            proj,
        );
        assert_eq!(v, Decision::Deny);
        assert!(reason.unwrap().contains("deny is final"));
    }

    #[test]
    fn activation_scope_decides_escape_check() {
        let proj = Path::new(PROJ);
        // local 声明：仓库内参数 → allow；逃逸参数 → confirm。
        let (v, _) = finalize(
            Decision::Confirm,
            ScriptOutcome::Activate("ls".into()),
            &decls(&["ls"], &[]),
            &cmd("ls > out.txt"),
            proj,
        );
        assert_eq!(v, Decision::Allow);
        let (v, reason) = finalize(
            Decision::Confirm,
            ScriptOutcome::Activate("ls".into()),
            &decls(&["ls"], &[]),
            &cmd("ls ../../outside.txt"),
            proj,
        );
        assert_eq!(v, Decision::Confirm);
        assert!(reason.unwrap().contains("escapes"));
        // global 声明：豁免逃逸检查。
        let (v, reason) = finalize(
            Decision::Confirm,
            ScriptOutcome::Activate("docker".into()),
            &decls(&[], &["docker"]),
            &cmd("docker > /outside.txt"),
            proj,
        );
        assert_eq!(v, Decision::Allow);
        assert!(reason.unwrap().contains("global"));
        // 两表皆现 → global 胜（scope_of 体现）。
        let d = decls(&["ls"], &["ls"]);
        assert_eq!(d.scope_of("ls"), Some(DeclScope::Global));
        // 查表 allow 上的激活 = 幂等。
        let (v, _) = finalize(
            Decision::Allow,
            ScriptOutcome::Activate("ls".into()),
            &decls(&["ls"], &[]),
            &cmd("ls > out.txt"),
            proj,
        );
        assert_eq!(v, Decision::Allow);
    }
}
