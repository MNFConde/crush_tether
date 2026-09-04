//! bin 入口：check 模式（P1 最小闭环）。
//!
//! 用法：`crush-tether check --agent crush` —— stdin 收 hook JSON，
//! 按三档语义输出 allow JSON / 静默 / exit 2。serve 模式随 P4 落地。

use std::process::ExitCode;

use crush_tether::channel::{self, Agent};
use crush_tether::engine;

fn main() -> ExitCode {
    let mut agent = Agent::Crush;
    let mut mode = String::from("check");

    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "check" | "serve" => mode = arg,
            "--agent" => {
                if let Some(a) = args.next().as_deref().and_then(Agent::parse) {
                    agent = a;
                }
            }
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

    // 项目根：env 注入优先（路径逃逸检查基准）；测试态由 env 决定。
    let verdict = if let Some(dir) = &input.project_dir {
        let prev = std::env::var("CRUSH_PROJECT_DIR").ok();
        unsafe { std::env::set_var("CRUSH_PROJECT_DIR", dir) };
        let v = engine::decide(&input.command);
        match prev {
            Some(p) => unsafe { std::env::set_var("CRUSH_PROJECT_DIR", p) },
            None => unsafe { std::env::remove_var("CRUSH_PROJECT_DIR") },
        }
        v
    } else {
        engine::decide(&input.command)
    };

    ExitCode::from(channel::emit(&verdict, agent) as u8)
}
