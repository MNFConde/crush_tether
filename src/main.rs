//! bin 入口：运行模式分派（design.md「软件与项目内脚本分工」）。
//!
//! - `check`：单发全量管线（兜底/冒烟/测试基准）；`--batch` 批量裁决表、
//!   `--cases <file>` 断言用例对账（M7.1）。
//! - `hook`：connect-or-spawn 主路径（连不上 serve → detached spawn +
//!   ~200ms 有界重试 → 仍失败降级本进程 check，绝不无裁决放行）。
//! - `serve`：常驻服务（独占 bind 单实例 + 串行 accept + idle 退出）。
//! - `benchmark`：双跑对比（in-process vs serve 路径），验收 diff 为空。
//! - `explain`：单发全溯源报告（M7.1 人读调试）。
//! - `repl`：用户面规则调试器（M7.1，每条输入重载配置，改规则即测）。
//! - `init`：显式生成默认配置包（P8/M8.1，D-09——配置创建唯一路径，自动
//!   生成已移除；缺省项目层，`--user`/`--global` 切目标层）。
//! - `suggest`：权限建议（M8.6，D-10 定位 = 配置打磨手段）——裁决日志 ×
//!   执行记录交叉推断反复批准的命令，stdout 输出建议块（零写入）。
//!
//! 裁决管线：配置加载（显式覆盖或三层发现）→ 字段级继承合并 →
//! rules.toml 查表（多命中合成）→ rules.rhai 脚本 → 定稿点 → 组合裁决，
//! 装配在 `service::RuleSet`（serve 与 check 共用同一实现）。

use std::io::BufRead;
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
    let mut batch = false;
    let mut cases: Option<String> = None;
    let mut init_user = false;
    let mut init_global = false;
    let mut threshold: Option<usize> = None;
    let mut window_days: Option<u64> = None;
    let mut format = String::from("toml");
    let mut positional: Vec<String> = Vec::new();

    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "check" | "hook" | "serve" | "benchmark" | "explain" | "repl" | "init" | "suggest" => {
                mode = arg
            }
            "--agent" => match args.next().as_deref().and_then(Agent::parse) {
                Some(a) => agent = a,
                None => {
                    eprintln!(
                        "crush-tether: unknown --agent value; falling back to crush \
                         (supported: crush, claudecode, zcode)"
                    );
                }
            },
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
            "--batch" => batch = true,
            // suggest 选项（M8.6）：候选门槛 / 统计窗口 / 输出形态。
            "--threshold" => match args.next().and_then(|v| v.parse::<usize>().ok()) {
                Some(n) => threshold = Some(n),
                None => {
                    eprintln!("crush-tether: --threshold requires a number");
                    return fail_safe_confirm(agent);
                }
            },
            "--window" => match args.next().and_then(|v| v.parse::<u64>().ok()) {
                Some(d) => window_days = Some(d),
                None => {
                    eprintln!("crush-tether: --window requires days");
                    return fail_safe_confirm(agent);
                }
            },
            "--format" => match args.next() {
                Some(f) => format = f,
                None => {
                    eprintln!("crush-tether: --format requires toml|table");
                    return fail_safe_confirm(agent);
                }
            },
            // init 目标层（P8/M8.1）：缺省项目层，两旗标互斥。
            "--user" => init_user = true,
            "--global" => init_global = true,
            "--cases" => match args.next() {
                Some(p) => cases = Some(p),
                None => {
                    eprintln!("crush-tether: --cases requires a file path argument");
                    return fail_safe_confirm(agent);
                }
            },
            _ => {
                if arg.starts_with('-') {
                    // 未知 flag 告警不硬错（保兼容）：配置文件拼写错误是硬错误，
                    // CLI 拼错也不该静默改用缺省（fail-safe 哲学一致性）。
                    eprintln!("crush-tether: unknown argument `{arg}` ignored");
                } else {
                    // 位置参数：explain 模式的命令文本（支持未引号多词形态）。
                    positional.push(arg);
                }
            }
        }
    }

    // 脚本引擎选型（design.md「DSL 引擎（定稿）」）：未知引擎属配置错误 →
    // 告警 + fail-safe confirm，不静默退回默认引擎。
    if let Some(e) = &engine_arg
        && !crush_tether::script::engine_supported(e)
    {
        eprintln!(
            "crush-tether: unsupported engine `{e}` (supported: {}); fail-safe confirm",
            crush_tether::script::SUPPORTED_ENGINES.join(", ")
        );
        return fail_safe_confirm(agent);
    }
    let engine = engine_label(engine_arg.as_deref());

    match mode.as_str() {
        "serve" => {
            let project = project_arg.unwrap_or_else(crush_tether::config::find_project_root);
            let idle = Duration::from_secs(idle_secs.unwrap_or(30));
            service::serve_main(project, engine, config_arg, idle)
        }
        "hook" => run_hook(agent, config_arg.as_deref(), &engine),
        "benchmark" => run_benchmark(agent, config_arg.as_deref(), &engine),
        "explain" => run_explain(
            config_arg.as_deref(),
            &engine,
            project_arg.as_ref(),
            &positional,
        ),
        "repl" => {
            let project = crush_tether::repl::resolve_project(project_arg.as_ref());
            crush_tether::repl::run(&project, config_arg.as_deref(), &engine)
        }
        "init" => run_init(project_arg.as_ref(), &engine, init_user, init_global),
        "suggest" => {
            let project = project_arg.unwrap_or_else(crush_tether::config::find_project_root);

            ExitCode::from(crush_tether::suggest::run(
                &project,
                &crush_tether::suggest::SuggestOptions {
                    threshold: threshold.unwrap_or(3),
                    window_days: window_days.unwrap_or(30),
                    format,
                },
            ) as u8)
        }
        _ => {
            if batch {
                return run_batch(config_arg.as_deref(), &engine, project_arg.as_ref());
            }
            if let Some(file) = cases.as_deref() {
                return run_cases(file, config_arg.as_deref(), &engine, project_arg.as_ref());
            }
            run_check(agent, config_arg.as_deref(), &engine)
        }
    }
}

/// 引擎标签（`--engine` 缺省 rhai）；进入端点名 hash，并决定脚本文件名
/// （`rules.rhai`/`rules.lua`）与日志 script.file 溯源。
fn engine_label(engine_arg: Option<&str>) -> String {
    engine_arg.unwrap_or("rhai").to_string()
}

/// init 模式（P8/M8.1，D-09）：显式生成默认配置包——配置创建唯一路径，
/// 已存在文件一律不动（缺哪个补哪个）。缺省项目层（`--project` 可指根），
/// `--user`/`--global` 切目标层（互斥）。目标目录解析失败 → stderr + exit 2。
fn run_init(project_arg: Option<&PathBuf>, engine: &str, user: bool, global: bool) -> ExitCode {
    if user && global {
        eprintln!("crush-tether: --user and --global are mutually exclusive");
        return ExitCode::from(2);
    }
    let dir = if global {
        match crush_tether::config::global_dir() {
            Some(d) => d,
            None => {
                eprintln!(
                    "crush-tether: cannot resolve global config directory \
                     (set CRUSH_TETHER_GLOBAL_DIR or PROGRAMDATA)"
                );
                return ExitCode::from(2);
            }
        }
    } else if user {
        match crush_tether::config::home_dir() {
            Some(h) => h.join(".config").join("crush-tether"),
            None => {
                eprintln!("crush-tether: cannot resolve home directory (set USERPROFILE or HOME)");
                return ExitCode::from(2);
            }
        }
    } else {
        project_arg
            .cloned()
            .unwrap_or_else(crush_tether::config::find_project_root)
            .join(".crush-tether")
    };
    match crush_tether::config::seed::write_default_pack(&dir, engine) {
        Ok(0) => {
            println!(
                "crush-tether: default pack already present in {} (nothing written)",
                dir.display()
            );
            ExitCode::from(0)
        }
        Ok(n) => {
            println!(
                "crush-tether: wrote {n} default pack file(s) to {}",
                dir.display()
            );
            ExitCode::from(0)
        }
        Err(e) => {
            eprintln!("crush-tether: init failed: {e}");
            ExitCode::from(2)
        }
    }
}

fn read_project(agent: Agent) -> Option<(String, PathBuf)> {
    let input = channel::read_hook_input(agent)?;
    let project = input
        .project_dir
        .as_deref()
        .map(PathBuf::from)
        .unwrap_or_else(crush_tether::config::find_project_root);
    Some((input.command, project))
}

/// check 模式：单发全量管线（in-process）。
fn run_check(agent: Agent, config_arg: Option<&str>, engine: &str) -> ExitCode {
    let Some((command, project)) = read_project(agent) else {
        // 读不到输入：保守 confirm（exit 0 无输出，走正常权限提示）。
        return ExitCode::from(0);
    };
    match check_verdict(
        &project,
        config_arg,
        engine,
        &command,
        agent,
        "check",
        (None, None),
    ) {
        Ok(verdict) => ExitCode::from(channel::emit(&verdict, agent)),
        Err(code) => code,
    }
}

/// hook 模式：事件分派（M8.6）——PostToolUse 走执行记录采集 + 会话放行
/// 对账（零裁决输出、恒 exit 0），PreToolUse（缺省）走 connect-or-spawn
/// 主路径 + 降级（降级态会话便签走文件形态）。
fn run_hook(agent: Agent, config_arg: Option<&str>, engine: &str) -> ExitCode {
    let Some(input) = channel::read_hook_input(agent) else {
        return ExitCode::from(0);
    };
    let project = input
        .project_dir
        .as_deref()
        .map(PathBuf::from)
        .unwrap_or_else(crush_tether::config::find_project_root);
    let command = input.command;
    let session = input.session_id.as_deref();
    let tool_use = input.tool_use_id.as_deref();
    // PostToolUse：执行记录采集（executions.jsonl 一行一执行；成败尽力
    // 提取）+ 会话放行对账（serve 直连优先，降级态转文件便签）。无阻断
    // 语义，采集失败不影响 agent（恒 exit 0、零输出）。
    if input.event.as_deref() == Some("PostToolUse") {
        if service::learn_enabled() {
            service::log_execution(
                &project,
                service::ExecutionRecord {
                    agent: agent.slug(),
                    session_id: session,
                    tool_use_id: tool_use,
                    command: &command,
                    success: input.tool_success,
                },
            );
        }
        if let (Some(s), Some(tu)) = (session, tool_use)
            && !service::hook_post(&project, engine, config_arg, s, tu)
        {
            service::file_confirm_pending(&project, s, tu);
        }
        return ExitCode::from(0);
    }
    if let Some(v) = service::hook_decide(
        &project,
        engine,
        config_arg,
        agent.slug(),
        &command,
        session,
        tool_use,
    ) {
        return ExitCode::from(channel::emit(&v, agent));
    }
    // 降级路径：本进程 check（仍然全量管线，绝不无裁决放行）+ 文件形态
    // 会话放行（serve 不可达时的便签查询/记录）。日志 mode 记 "hook"：
    // 审计可区分「serve 降级」与「独立 check」。
    match check_verdict(
        &project,
        config_arg,
        engine,
        &command,
        agent,
        "hook",
        (session, tool_use),
    ) {
        Ok(verdict) => ExitCode::from(channel::emit(&verdict, agent)),
        Err(code) => code,
    }
}

/// benchmark 模式：双跑对比——in-process 全量管线 vs hook（serve）路径，
/// 裁决 diff 为空即 exit 0（验收 `--benchmark` 双跑 diff 为空）。
fn run_benchmark(agent: Agent, config_arg: Option<&str>, engine: &str) -> ExitCode {
    let Some((command, project)) = read_project(agent) else {
        return ExitCode::from(0);
    };
    let local = check_verdict(
        &project,
        config_arg,
        engine,
        &command,
        agent,
        "benchmark",
        (None, None),
    )
    .ok();
    let via_serve = service::hook_decide(
        &project,
        engine,
        config_arg,
        agent.slug(),
        &command,
        None,
        None,
    );
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
    session_keys: (Option<&str>, Option<&str>),
) -> Result<Verdict, ExitCode> {
    let (session, tool_use) = session_keys;
    match RuleSet::load(project, config_arg, engine) {
        Ok(rs) => {
            let (verdict, components) = rs.decide_components(command, project);
            // 会话放行（M8.6，降级态文件形态）：只作用于 confirm；非 confirm
            // 原样返回。serve 主路径的改判在 serve 内（apply_session_allow）。
            let verdict = service::maybe_file_session_allow(
                verdict,
                &components,
                command,
                session,
                tool_use,
                project,
            );
            let trace = service::merge_traces(&components);
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
                    script_file: crush_tether::script::script_file_name(&rs.engine),
                    session_id: session,
                    tool_use_id: tool_use,
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
    ExitCode::from(channel::emit(&verdict, agent))
}

/// explain 模式（M7.1）：单发全溯源报告（人读）。命令 = 位置参数拼接
/// （`explain git push` 与 `explain 'git push'` 等价）；不落裁决日志。
fn run_explain(
    config_arg: Option<&str>,
    engine: &str,
    project_arg: Option<&PathBuf>,
    positional: &[String],
) -> ExitCode {
    if positional.is_empty() {
        eprintln!("usage: crush-tether explain '<command>'");
        return ExitCode::from(2);
    }
    let command = positional.join(" ");
    let project = crush_tether::repl::resolve_project(project_arg);
    match RuleSet::load(&project, config_arg, engine) {
        Ok(rs) => {
            print!(
                "{}",
                crush_tether::report::render_report(&rs.explain(&command, &project))
            );
            ExitCode::from(0)
        }
        Err(e) => {
            eprintln!("crush-tether: {e}");
            ExitCode::from(2)
        }
    }
}

/// check --batch（M7.1）：stdin 一行一命令 → 裁决表（人读；exit 0 恒定，
/// 表是报告不是阻断）。空行跳过；加载失败 stderr + exit 2。
fn run_batch(config_arg: Option<&str>, engine: &str, project_arg: Option<&PathBuf>) -> ExitCode {
    let project = crush_tether::repl::resolve_project(project_arg);
    let rs = match RuleSet::load(&project, config_arg, engine) {
        Ok(rs) => rs,
        Err(e) => {
            eprintln!("crush-tether: {e}");
            return ExitCode::from(2);
        }
    };
    let stdin = std::io::stdin();
    for line in stdin.lock().lines().map_while(Result::ok) {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let v = rs.decide(line, &project);
        println!("{}", crush_tether::report::render_batch_line(line, &v));
    }
    ExitCode::from(0)
}

/// `--cases` 用例文件 schema（TOML）。
#[derive(serde::Deserialize)]
struct CasesFile {
    version: u64,
    #[serde(rename = "case")]
    cases: Vec<CaseEntry>,
}

#[derive(serde::Deserialize)]
struct CaseEntry {
    cmd: String,
    expect: String,
}

/// check --cases（M7.1）：断言式规则用例批量对账——输入 + 期望档位，
/// 逐条对账输出 PASS/FAIL 表；全过 exit 0，任一失败 exit 1。
fn run_cases(
    file: &str,
    config_arg: Option<&str>,
    engine: &str,
    project_arg: Option<&PathBuf>,
) -> ExitCode {
    let text = match std::fs::read_to_string(file) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("crush-tether: cannot read cases file `{file}`: {e}");
            return ExitCode::from(2);
        }
    };
    let parsed: CasesFile = match toml::from_str(&text) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("crush-tether: cases file `{file}` parse error: {e}");
            return ExitCode::from(2);
        }
    };
    if parsed.version != 1 {
        eprintln!(
            "crush-tether: cases file `{file}` version {} unsupported (expected 1)",
            parsed.version
        );
        return ExitCode::from(2);
    }
    let project = crush_tether::repl::resolve_project(project_arg);
    let rs = match RuleSet::load(&project, config_arg, engine) {
        Ok(rs) => rs,
        Err(e) => {
            eprintln!("crush-tether: {e}");
            return ExitCode::from(2);
        }
    };
    let mut failed = 0usize;
    for c in &parsed.cases {
        let actual = crush_tether::model::Decision::parse(&c.expect).map(|expect| {
            let actual = rs.decide(&c.cmd, &project).decision;
            (expect, actual)
        });
        match actual {
            Some((expect, actual)) => {
                let pass = actual == expect;
                if !pass {
                    failed += 1;
                }
                println!(
                    "{}",
                    crush_tether::report::render_case_line(pass, &c.cmd, expect, actual)
                );
            }
            None => {
                println!("FAIL  (bad expect `{}`) {}", c.expect, c.cmd);
                failed += 1;
            }
        }
    }
    let total = parsed.cases.len();
    println!("{}/{total} passed", total - failed);
    if failed == 0 {
        ExitCode::from(0)
    } else {
        ExitCode::from(1)
    }
}
