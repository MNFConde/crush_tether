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
use std::sync::Arc;

use crush_tether::channel::{self, Agent};
use crush_tether::config::{self, Layers};
use crush_tether::engine;
use crush_tether::lookup::RuleLookup;
use crush_tether::model::Verdict;
use crush_tether::script::RuleEngine;

fn main() -> ExitCode {
    let mut agent = Agent::Crush;
    let mut mode = String::from("check");
    let mut config_arg: Option<String> = None;
    let mut engine_arg: Option<String> = None;

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
            "--engine" => match args.next() {
                Some(e) => engine_arg = Some(e),
                None => {
                    eprintln!("crush-tether: --engine requires an engine name");
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
    // 知识库 main（项目层单文件，与 --config 无关）单独降级：损坏按
    // 「缺失 + 告警」处理——知识库只记事实不产生裁决，删光/损坏后归一失效
    // （等价命令按字面查表落 default），判定不受影响。
    let home = config::home_dir();
    let found = config::discover_layers(Some(&project), home.as_deref());
    // 三层皆缺 → 引导生成默认包（生成动作是管线引导步骤，不经规则链，
    // design.md「零内置策略与默认配置生成」）。任一层损坏（found 为 Err）
    // 时不生成：损坏 ≠ 缺失（D-03），fail-safe confirm、原文件不动。
    let found = match found {
        Ok(l) if l.all_absent() && config::explicit_path(config_arg.as_deref()).is_none() => {
            match config::seed::seed_defaults_if_absent(&project) {
                Ok(_) => {
                    eprintln!(
                        "crush-tether: no config found; seeded defaults in {}",
                        project.join(".crush-tether").display()
                    );
                    config::discover_layers(Some(&project), home.as_deref())
                }
                Err(e) => {
                    eprintln!(
                        "crush-tether: seeding default config failed: {e}; continuing \
                         without config (fail-safe confirm)"
                    );
                    Ok(l)
                }
            }
        }
        other => other,
    };
    let kb = found.as_ref().ok().and_then(|l| l.knowledge.as_ref());
    let lookup = if let Some(path) = config::explicit_path(config_arg.as_deref()) {
        match config::load_file(&path) {
            Ok(f) => RuleLookup::new(
                config::merge(Layers {
                    global: None,
                    user: None,
                    project: Some(&f),
                }),
                kb,
            ),
            Err(e) => {
                eprintln!(
                    "crush-tether: explicit config {} failed to load: {e}; fail-safe confirm",
                    path.display()
                );
                return fail_safe_confirm(agent);
            }
        }
    } else {
        match &found {
            Ok(l) => RuleLookup::new(config::merge(Layers::from_found(l)), kb),
            Err(e) => {
                eprintln!("crush-tether: config failed to load: {e}; fail-safe confirm");
                return fail_safe_confirm(agent);
            }
        }
    };

    // 脚本层：项目 rules.rhai（缺失 = 无脚本层，TOML 自足）。脚本承载条件
    // 判断并可产生裁决，故编译/加载失败必须告警 + fail-safe confirm，不能
    // 静默跳过（与知识库的降级策略相反）。
    let script = match crush_tether::script::load_project_script(
        &project,
        kb.map(|k| Arc::new(k.clone())),
    ) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("crush-tether: rules.rhai failed to load: {e}; fail-safe confirm");
            return fail_safe_confirm(agent);
        }
    };

    let verdict = engine::decide_with(&input.command, &project, &|cmd, project, pipe_to_shell| {
        let mut v = lookup.classify(cmd, project);
        if let Some(script) = &script {
            match script.evaluate(cmd, v.decision, project, pipe_to_shell) {
                Ok(Some(d)) => {
                    if d != v.decision {
                        v = Verdict {
                            decision: d,
                            reason: Some("adjusted by rules.rhai".into()),
                        };
                    }
                }
                Ok(None) => {}
                Err(e) => {
                    eprintln!("crush-tether: script evaluation failed: {e}; fail-safe confirm");
                    v = Verdict::confirm("script evaluation failed; fail-safe");
                }
            }
        }
        v
    });

    ExitCode::from(channel::emit(&verdict, agent) as u8)
}

/// fail-safe 兜底：confirm（两 agent 契约下均为静默 exit 0，走正常权限提示）。
fn fail_safe_confirm(agent: Agent) -> ExitCode {
    let verdict = Verdict::confirm("configuration error; fail-safe");
    ExitCode::from(channel::emit(&verdict, agent) as u8)
}
