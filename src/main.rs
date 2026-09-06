//! bin 入口：三角色运行时（design.md「软件与项目内脚本分工」）。
//!
//! - `check`：单发全量管线（兜底/冒烟/测试基准）。
//! - `hook`：connect-or-spawn 主路径（连不上 serve → detached spawn +
//!   ~200ms 有界重试 → 仍失败降级本进程 check，绝不无裁决放行）。
//! - `serve`：常驻服务（独占 bind 单实例 + 串行 accept + idle 退出）。
//! - `benchmark`：双跑对比（in-process vs serve 路径），验收 diff 为空。
//!
//! 裁决管线：配置加载（显式覆盖或三层发现 + 引导生成）→ 字段级继承合并 →
//! rules.toml 查表（多命中合成）→ rules.rhai 脚本 → 定稿点 → 组合裁决，
//! 装配在 `service::RuleSet`（serve 与 check 共用同一实现）。

use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Duration;

use crush_tether::channel::{self, Agent};
use crush_tether::model::Verdict;
use crush_tether::service::{self, RuleSet};

fn main() -> ExitCode {
    let mut agent = Agent::Crush;
    let mut mode = String::from("check");
    let mut config_arg: Option<String> = None;
    let mut engine_arg: Option<String> = None;
    let mut project_arg: Option<PathBuf> = None;
    let mut idle_secs: Option<u64> = None;

    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "check" | "hook" | "serve" | "benchmark" => mode = arg,
            "--agent" => {
                if let Some(a) = args.next().as_deref().and_then(Agent::parse) {
                    agent = a;
                }
            }
            "--config" => match args.next() {
                Some(p) => config_arg = Some(p),
                None => {
                    eprintln!("crush-tether: --config requires a path argument");
                    return fail_safe_confirm(agent);
                }
            },
            "--engine" => match args.next() {
                Some(e) => engine_arg = Some(e),
                None => {
                    eprintln!("crush-tether: --engine requires an engine name");
                    return fail_safe_confirm(agent);
                }
            },
            "--project" => match args.next() {
                Some(p) => project_arg = Some(PathBuf::from(p)),
                None => {
                    eprintln!("crush-tether: --project requires a path argument");
                    return fail_safe_confirm(agent);
                }
            },
            "--idle-exit" => match args.next().and_then(|v| v.parse::<u64>().ok()) {
                Some(s) => idle_secs = Some(s),
                None => {
                    eprintln!("crush-tether: --idle-exit requires seconds");
                    return fail_safe_confirm(agent);
                }
            },
            _ => {}
        }
    }

    // 脚本引擎选型（design.md「DSL 引擎（定稿）」）：v1 仅 rhai；未知引擎
    // 属配置错误 → 告警 + fail-safe confirm，不静默退回默认引擎。
    if let Some(e) = &engine_arg
        && !crush_tether::script::engine_supported(e)
    {
        eprintln!("crush-tether: unsupported engine `{e}` (supported: rhai); fail-safe confirm");
        return fail_safe_confirm(agent);
    }
    let engine = engine_label(engine_arg.as_deref());

    match mode.as_str() {
        "serve" => {
            let project = project_arg.unwrap_or_else(crush_tether::cmd_parse::project_root);
            let idle = Duration::from_secs(idle_secs.unwrap_or(30));
            service::serve_main(project, engine, config_arg, idle)
        }
        "hook" => run_hook(agent, config_arg.as_deref(), &engine),
        "benchmark" => run_benchmark(agent, config_arg.as_deref(), &engine),
        _ => run_check(agent, config_arg.as_deref(), &engine),
    }
}

/// 引擎标签（`--engine` 缺省 rhai）；进入端点名 hash 与脚本装配。
fn engine_label(engine_arg: Option<&str>) -> String {
    engine_arg.unwrap_or("rhai").to_string()
}

fn read_project(agent: Agent) -> Option<(String, PathBuf)> {
    let input = channel::read_hook_input(agent)?;
    let project = input
        .project_dir
        .as_deref()
        .map(PathBuf::from)
        .unwrap_or_else(crush_tether::cmd_parse::project_root);
    Some((input.command, project))
}

/// check 模式：单发全量管线（in-process）。
fn run_check(agent: Agent, config_arg: Option<&str>, engine: &str) -> ExitCode {
    let Some((command, project)) = read_project(agent) else {
        // 读不到输入：保守 confirm（exit 0 无输出，走正常权限提示）。
        return ExitCode::from(0);
    };
    match check_verdict(&project, config_arg, engine, &command, agent, "check") {
        Ok(verdict) => ExitCode::from(channel::emit(&verdict, agent) as u8),
        Err(code) => code,
    }
}

/// hook 模式：connect-or-spawn 主路径 + 降级。
fn run_hook(agent: Agent, config_arg: Option<&str>, engine: &str) -> ExitCode {
    let Some((command, project)) = read_project(agent) else {
        return ExitCode::from(0);
    };
    if let Some(v) = service::hook_decide(&project, engine, config_arg, agent.slug(), &command) {
        return ExitCode::from(channel::emit(&v, agent) as u8);
    }
    // 降级路径：本进程 check（仍然全量管线，绝不无裁决放行）。
    match check_verdict(&project, config_arg, engine, &command, agent, "check") {
        Ok(verdict) => ExitCode::from(channel::emit(&verdict, agent) as u8),
        Err(code) => code,
    }
}

/// benchmark 模式：双跑对比——in-process 全量管线 vs hook（serve）路径，
/// 裁决 diff 为空即 exit 0（验收 `--benchmark` 双跑 diff 为空）。
fn run_benchmark(agent: Agent, config_arg: Option<&str>, engine: &str) -> ExitCode {
    let Some((command, project)) = read_project(agent) else {
        return ExitCode::from(0);
    };
    let local = check_verdict(&project, config_arg, engine, &command, agent, "benchmark").ok();
    let via_serve = service::hook_decide(&project, engine, config_arg, agent.slug(), &command);
    let local_d = local.as_ref().map(|v| v.decision.to_string());
    let serve_d = via_serve.as_ref().map(|v| v.decision.to_string());
    let match_ = match (&local_d, &serve_d) {
        (Some(a), Some(b)) => a == b,
        _ => false,
    };
    println!(
        "{{\"benchmark\":{{\"command\":{},\"local\":{},\"serve\":{},\"match\":{}}}}}",
        json_str(&command),
        json_str(local_d.as_deref().unwrap_or("")),
        serve_d
            .as_deref()
            .map(json_str)
            .unwrap_or_else(|| "null".into()),
        match_
    );
    ExitCode::from(u8::from(!match_))
}

fn json_str(s: &str) -> String {
    serde_json::to_string(s).unwrap_or_else(|_| "\"\"".into())
}

/// in-process 全量管线（check 与 hook 降级共用）；加载失败 → 告警 +
/// fail-safe confirm（Err 携带该退出码）。
fn check_verdict(
    project: &std::path::Path,
    config_arg: Option<&str>,
    engine: &str,
    command: &str,
    agent: Agent,
    mode: &str,
) -> Result<Verdict, ExitCode> {
    match RuleSet::load(project, engine, config_arg) {
        Ok(rs) => {
            let (verdict, trace) = rs.decide_trace(command, project);
            service::log_verdict(
                project,
                command,
                &verdict,
                &trace,
                service::LogContext {
                    mode,
                    agent: agent.slug(),
                    kb_present: rs.kb_present,
                    explicit: rs.config_path.as_deref(),
                },
            );
            Ok(verdict)
        }
        Err(msg) => {
            eprintln!("{msg}");
            Err(fail_safe_confirm(agent))
        }
    }
}

/// fail-safe 兜底：confirm（两 agent 契约下均为静默 exit 0，走正常权限提示）。
fn fail_safe_confirm(agent: Agent) -> ExitCode {
    let verdict = Verdict::confirm("configuration error; fail-safe");
    ExitCode::from(channel::emit(&verdict, agent) as u8)
}
