//! `repl` 模式（M7.1 用户面调试器）：读配置快照逐条调试自己的规则配置与
//! 脚本，即时显示命中与未命中原因。
//!
//! 设计定点（ROADMAP M7.1，2026-09-08 就地定并登记）：
//! - **纯 stdio**（零行编辑依赖）：输入一行算一行；空行 = 重复上一条
//!   （覆盖「改规则 → 重跑同一命令」的热重载复测主路径），`:hist` 列历史、
//!   `!N` 重跑第 N 条；不引 rustyline，行编辑体验留作后续升级（求值核
//!   与 explain 共用，升级只换输入壳）。
//! - **每条输入重新加载配置**：项目配置小（毫秒级），天然获得「改规则即
//!   测、免重启」——不接 serve 的 notify 热重载（单用户交互无常驻必要）。
//! - **不落裁决日志**：调试工具防噪音；审计面由 check/hook/serve 承载。

use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use crate::report::render_command;
use crate::service::RuleSet;

const HELP: &str = "commands: <bash command> | Enter (repeat last) | !N (rerun #N) | \
:hist | :lint | :q";

/// REPL 主循环。配置加载失败打印错误并继续（不退出——修完配置即测）。
pub fn run(project: &Path, config_arg: Option<&str>, engine: &str) -> ExitCode {
    eprintln!(
        "crush-tether repl — project: {}  engine: {}  ({HELP})",
        project.display(),
        engine
    );
    let stdin = std::io::stdin();
    let mut history: Vec<String> = Vec::new();
    loop {
        print!("tether> ");
        let _ = std::io::stdout().flush();
        let mut line = String::new();
        if stdin.lock().read_line(&mut line).unwrap_or(0) == 0 {
            break; // EOF
        }
        let line = line.trim().to_string();
        match line.as_str() {
            ":q" | ":quit" | ":exit" => break,
            ":h" | ":help" => {
                eprintln!("{HELP}");
                continue;
            }
            ":hist" => {
                for (i, h) in history.iter().enumerate() {
                    eprintln!("{:>3}  {}", i + 1, h);
                }
                continue;
            }
            ":lint" => {
                show_lint(project, config_arg, engine);
                continue;
            }
            _ => {}
        }
        let effective: String = if line.is_empty() {
            match history.last() {
                Some(l) => l.clone(),
                None => continue,
            }
        } else if let Some(rest) = line.strip_prefix('!') {
            match rest
                .parse::<usize>()
                .ok()
                .and_then(|n| history.get(n.checked_sub(1).unwrap_or(usize::MAX)).cloned())
            {
                Some(h) => h,
                None => {
                    eprintln!("no such history entry: {rest}");
                    continue;
                }
            }
        } else {
            line.clone()
        };
        if !effective.is_empty() && history.last() != Some(&effective) {
            history.push(effective.clone());
        }
        eval_line(project, config_arg, engine, &effective);
    }
    ExitCode::from(0)
}

/// 单条命令评估：全量加载 + explain 溯源（stdout 为结果面，stderr 为提示面）。
fn eval_line(project: &Path, config_arg: Option<&str>, engine: &str, command: &str) {
    match RuleSet::load(project, config_arg, engine) {
        Ok(rs) => {
            let report = rs.explain(command, project);
            for (i, c) in report.commands.iter().enumerate() {
                print!("{}", render_command(i + 1, c));
            }
            println!("combined: {}", report.combined.decision);
        }
        Err(e) => eprintln!("config error: {e}"),
    }
}

/// 显式查看当前配置的 lint 告警（平时不刷屏）。
fn show_lint(project: &Path, config_arg: Option<&str>, engine: &str) {
    match RuleSet::load(project, config_arg, engine) {
        Ok(rs) => {
            if rs.lint_warnings.is_empty() {
                eprintln!("lint: no warnings");
            }
            for w in &rs.lint_warnings {
                eprintln!("lint: {:?} {} ({})", w.severity, w.message, w.code);
            }
        }
        Err(e) => eprintln!("config error: {e}"),
    }
}

/// 项目根解析（REPL 与 explain 共用）：`--project` 优先，否则 cwd 上溯。
pub fn resolve_project(explicit: Option<&PathBuf>) -> PathBuf {
    explicit
        .cloned()
        .unwrap_or_else(crate::config::find_project_root)
}
