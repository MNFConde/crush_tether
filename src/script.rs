//! 脚本层引擎（Rhai，v1）：沙箱执行 `rules.rhai`，承载声明层表达不了的
//! 条件判断（design.md「DSL 引擎（定稿）」）。
//!
//! - 同一 [`RuleEngine`] trait 是引擎开闭落点（`--engine lua` 随 P6 落地）。
//! - Rust 提供**不可绕过的安全原语**（`path_escapes` / `inside_repo` /
//!   `kb_*` 知识库数据源），DSL 只能组合判定、不能绕过；沙箱默认无
//!   文件/进程/网络 API。
//! - 限流（定稿要求）：`max_operations` / `max_call_levels` /
//!   `max_expr_depths` + 字符串/数组上限——死循环或 OOM 尝试被有界拦截。
//! - 脚本契约：入口 `fn check(ctx) -> string`，返回 `""`（无意见，保留
//!   查表裁决）/ `"allow"` / `"confirm"` / `"deny"`；其他返回值、编译错误、
//!   运行时错误、限流触发一律由调用方映射为 fail-safe confirm。
//! - AST 编译一次缓存于实例（check 每次全量、serve 复用同一实例）。

use std::path::{Path, PathBuf};
use std::sync::Arc;

use rhai::{Dynamic, Engine};

use crate::cmd_parse::SimpleCommand;
use crate::knowledge::KnowledgeBase;
use crate::model::Decision;

/// 本构建支持的脚本层引擎名（`--engine`；Lua 随 P6）。
pub const SUPPORTED_ENGINES: &[&str] = &["rhai"];

/// 脚本层失败原因（任何变体 → 调用方 fail-safe confirm）。
#[derive(Debug)]
pub enum ScriptError {
    Io(std::io::Error),
    Compile(String),
    Runtime(String),
    /// 脚本返回了契约之外的值。
    Contract(String),
}

impl std::fmt::Display for ScriptError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ScriptError::Io(e) => write!(f, "io error: {e}"),
            ScriptError::Compile(e) => write!(f, "compile error: {e}"),
            ScriptError::Runtime(e) => write!(f, "runtime error: {e}"),
            ScriptError::Contract(e) => write!(f, "contract violation: {e}"),
        }
    }
}

impl std::error::Error for ScriptError {}

/// 脚本引擎抽象（引擎开闭落点；Rhai 为 v1 默认实现）。
pub trait RuleEngine {
    /// 单命令评估：`Ok(None)` = 脚本无意见（保留查表裁决）。
    /// `pipe_to_shell` 为管线原语计算的管道拓扑特征（整条命令行级）。
    fn evaluate(
        &self,
        cmd: &SimpleCommand,
        verdict: Decision,
        project: &Path,
        pipe_to_shell: bool,
    ) -> Result<Option<Decision>, ScriptError>;
}

/// Rhai 引擎实例：`Engine` + 编译缓存的 AST + 原语闭包捕获的上下文。
pub struct RhaiEngine {
    engine: Engine,
    ast: rhai::AST,
}

impl RhaiEngine {
    /// 编译脚本并装配沙箱（限流 + 原语注册）；编译错误在此暴露。
    pub fn compile(
        source: &str,
        project: PathBuf,
        kb: Option<Arc<KnowledgeBase>>,
    ) -> Result<Self, ScriptError> {
        let mut engine = Engine::new();
        // 限流（防死循环 / 深递归 / OOM）。
        engine.set_max_operations(100_000);
        engine.set_max_call_levels(64);
        engine.set_max_expr_depths(32, 32);
        engine.set_max_array_size(1_000);
        engine.set_max_string_size(10_000);

        register_primitives(&mut engine, project, kb);

        let ast = engine
            .compile(source)
            .map_err(|e| ScriptError::Compile(e.to_string()))?;
        Ok(Self { engine, ast })
    }
}

impl RuleEngine for RhaiEngine {
    fn evaluate(
        &self,
        cmd: &SimpleCommand,
        verdict: Decision,
        project: &Path,
        pipe_to_shell: bool,
    ) -> Result<Option<Decision>, ScriptError> {
        let mut scope = rhai::Scope::new();
        let ctx = build_context(cmd, verdict, project, pipe_to_shell);
        let result: Dynamic = self
            .engine
            .call_fn(&mut scope, &self.ast, "check", (ctx,))
            .map_err(|e| ScriptError::Runtime(e.to_string()))?;
        let s = result
            .into_string()
            .map_err(|_| ScriptError::Contract("check() must return a string".into()))?;
        match s.as_str() {
            "" => Ok(None),
            // allow 契约（M3.2 定稿）：脚本 v1 无放行权——放行语义完全由
            // rules.toml 承载；返回 allow（含无条件兜底）被结构性拒绝，
            // 由调用方映射为 fail-safe confirm。
            "allow" => Err(ScriptError::Contract(
                "scripts cannot return `allow` (v1 allow contract: allow belongs to \
                 rules.toml only; scripts may only raise to confirm/deny)"
                    .into(),
            )),
            "confirm" => Ok(Some(Decision::Confirm)),
            "deny" => Ok(Some(Decision::Deny)),
            other => Err(ScriptError::Contract(format!(
                "check() returned `{other}`; expected one of \"\", confirm, deny"
            ))),
        }
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
        match cmd.args().first() {
            Some(s) => Dynamic::from(s.clone()),
            None => Dynamic::UNIT,
        },
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

/// 加载项目层脚本 `.crush-tether/rules.rhai`（缺失 = 无脚本层，TOML 自足）。
pub fn load_project_script(
    project: &Path,
    kb: Option<Arc<KnowledgeBase>>,
) -> Result<Option<RhaiEngine>, ScriptError> {
    let path = project.join(".crush-tether").join("rules.rhai");
    match std::fs::read_to_string(&path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(ScriptError::Io(e)),
        Ok(source) => RhaiEngine::compile(&source, project.to_path_buf(), kb).map(Some),
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
        RhaiEngine::compile(src, PathBuf::from("D:/code/tmp/proj"), None).expect("compiles")
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
            Some(Decision::Deny)
        );
        assert_eq!(
            e.evaluate(&cmd("rm x"), Decision::Allow, Path::new(PROJ), false)
                .unwrap(),
            Some(Decision::Confirm)
        );
        assert_eq!(
            e.evaluate(&cmd("ls"), Decision::Allow, Path::new(PROJ), false)
                .unwrap(),
            None
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
            Some(Decision::Deny)
        );
    }

    #[test]
    fn infinite_loop_is_bounded_by_operation_limit() {
        let e = compile("fn check(ctx) { while true {} }");
        let t0 = std::time::Instant::now();
        let r = e.evaluate(&cmd("ls"), Decision::Allow, Path::new(PROJ), false);
        assert!(r.is_err(), "限流必须把死循环变成 Err → 上层 confirm");
        assert!(
            t0.elapsed() < std::time::Duration::from_secs(2),
            "有界时间返回"
        );
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
            RhaiEngine::compile(src, PathBuf::from(PROJ), None),
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
            Some(Decision::Confirm)
        );
        assert_eq!(
            e.evaluate(&cmd("git branch"), Decision::Allow, Path::new(PROJ), false)
                .unwrap(),
            None
        );
        assert_eq!(
            e.evaluate(
                &cmd("git config a b"),
                Decision::Allow,
                Path::new(PROJ),
                false
            )
            .unwrap(),
            Some(Decision::Confirm)
        );
        assert_eq!(
            e.evaluate(
                &cmd("git config --list"),
                Decision::Allow,
                Path::new(PROJ),
                false
            )
            .unwrap(),
            None
        );
        assert_eq!(
            e.evaluate(
                &cmd("git reset --hard"),
                Decision::Allow,
                Path::new(PROJ),
                false
            )
            .unwrap(),
            Some(Decision::Deny)
        );
        assert_eq!(
            e.evaluate(&cmd("npx foo"), Decision::Allow, Path::new(PROJ), false)
                .unwrap(),
            Some(Decision::Confirm)
        );
        assert_eq!(
            e.evaluate(
                &cmd("cat ../outside"),
                Decision::Allow,
                Path::new(PROJ),
                false
            )
            .unwrap(),
            Some(Decision::Confirm)
        );
        // 知识库删光：查不到数据 → 各兜底分支不触发 → 无意见。
        let e_empty = RhaiEngine::compile(
            "fn check(ctx) { if kb_write_tokens(\"git\", \"branch\").contains(\"-d\") { \"confirm\" } else { \"\" } }",
            PathBuf::from(PROJ),
            None,
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
            None
        );
    }

    #[test]
    fn missing_script_file_is_none_not_error() {
        let r = load_project_script(Path::new("D:/code/tmp/definitely-absent"), None).unwrap();
        assert!(r.is_none(), "TOML 自足：脚本缺失不是错误");
    }

    #[test]
    fn allow_contract_rejects_script_allow() {
        // v1 脚本无放行权：返回 allow = 契约违约（无条件兜底被结构性拒绝）。
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
            Some(Decision::Confirm)
        );
        assert_eq!(
            e.evaluate(&cmd("sudo x"), Decision::Deny, Path::new(PROJ), false)
                .unwrap(),
            Some(Decision::Deny)
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
            Some(Decision::Deny)
        );
        assert_eq!(
            e.evaluate(&cmd("sh"), Decision::Allow, Path::new(PROJ), false)
                .unwrap(),
            None
        );
    }
}
