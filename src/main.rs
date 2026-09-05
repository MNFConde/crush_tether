//! bin 入口：check 模式（P1 最小闭环）。
//!
//! 用法：`crush-tether check --agent crush` —— stdin 收 hook JSON，
//! 按三档语义输出 allow JSON / 静默 / exit 2。serve 模式随 P4 落地。
//!
//! 裁决主路径（M2.3 起）：配置加载（显式覆盖或三层发现）→ 字段级继承合并 →
//! rules.toml 查表（多命中合成）；管线原语（解析拉平、管道 sink、组合裁决）
//! 在 `engine::decide_with`。内置判定表仅存在于库内供回归测试，M3.3 删除。

use std::path::PathBuf;
use std::process::ExitCode;

use crush_tether::channel::{self, Agent};
use crush_tether::config::{self, Layers};
use crush_tether::engine;
use crush_tether::lookup::RuleLookup;
use crush_tether::model::Verdict;

fn main() -> ExitCode {
    let mut agent = Agent::Crush;
    let mut mode = String::from("check");
    let mut config_arg: Option<String> = None;

    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "check" | "serve" => mode = arg,
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
            _ => {}
        }
    }

    if mode == "serve" {
        eprintln!("serve mode lands in P4; use check for now");
        return ExitCode::from(2);
    }

    let Some(input) = channel::read_hook_input(agent) else {
        // 读不到输入：保守 confirm（exit 0 无输出，走正常权限提示）。
        return ExitCode::from(0);
    };

    // 项目根：hook 注入优先（路径逃逸检查基准 + 配置发现锚点）。
    let project = input
        .project_dir
        .as_deref()
        .map(PathBuf::from)
        .unwrap_or_else(crush_tether::cmd_parse::project_root);

    // 配置：显式覆盖（--config > CRUSH_TETHER_CONFIG）优先，否则分层发现
    // （项目 > 用户 > 全局）；任一存在层损坏 → stderr 告警 + fail-safe
    // confirm，绝不静默回落其他层（design.md「损坏 ≠ 缺失」，D-03）。
    let merged = match config::explicit_path(config_arg.as_deref()) {
        Some(path) => config::load_file(&path).map(|f| {
            config::merge(Layers {
                global: None,
                user: None,
                project: Some(&f),
            })
        }),
        None => {
            let home = config::home_dir();
            config::discover_layers(Some(&project), home.as_deref())
                .map(|l| config::merge(Layers::from_found(&l)))
        }
    };
    let lookup = match merged {
        Ok(m) => RuleLookup::new(m),
        Err(e) => {
            eprintln!("crush-tether: config failed to load: {e}; fail-safe confirm");
            return fail_safe_confirm(agent);
        }
    };

    let verdict = engine::decide_with(&input.command, &project, &|cmd, project| {
        lookup.classify(cmd, project)
    });

    ExitCode::from(channel::emit(&verdict, agent) as u8)
}

/// fail-safe 兜底：confirm（两 agent 契约下均为静默 exit 0，走正常权限提示）。
fn fail_safe_confirm(agent: Agent) -> ExitCode {
    let verdict = Verdict::confirm("configuration error; fail-safe");
    ExitCode::from(channel::emit(&verdict, agent) as u8)
}
