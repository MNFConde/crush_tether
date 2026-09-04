//! Agent 适配层：从 agent 的 hook 调用中取命令、按契约输出裁决。
//!
//! channel 只做「拿命令 / 输出裁决」；分类逻辑在 engine，与 agent 无关。

use std::io::Read;

use serde_json::Value;

use crate::model::{Decision, Verdict};

/// 支持的 agent 适配器。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Agent {
    Crush,
    ClaudeCode,
}

impl Agent {
    /// 从 `--agent` 参数解析。
    pub fn parse(s: &str) -> Option<Self> {
        match s.to_ascii_lowercase().as_str() {
            "crush" => Some(Agent::Crush),
            "claude" | "claudecode" | "claude-code" => Some(Agent::ClaudeCode),
            _ => None,
        }
    }
}

/// 从 stdin JSON / 环境变量提取命令正文与项目根。
///
/// Crush：stdin `tool_input.command`，env `CRUSH_PROJECT_DIR`。
/// ClaudeCode：stdin `tool_input.command`，env `CLAUDE_PROJECT_DIR`（无对应 stdin 键时回退）。
pub struct HookInput {
    pub command: String,
    pub project_dir: Option<String>,
}

pub fn read_hook_input(agent: Agent) -> Option<HookInput> {
    let mut buf = String::new();
    std::io::stdin().read_to_string(&mut buf).ok()?;

    let json: Value = serde_json::from_str(&buf).ok()?;
    let command = json
        .pointer("/tool_input/command")
        .and_then(Value::as_str)
        .map(String::from)?;

    let env_key = match agent {
        Agent::Crush => "CRUSH_PROJECT_DIR",
        Agent::ClaudeCode => "CLAUDE_PROJECT_DIR",
    };
    let project_dir = std::env::var(env_key).ok().filter(|s| !s.is_empty());

    Some(HookInput {
        command,
        project_dir,
    })
}

pub fn emit(verdict: &Verdict, agent: Agent) -> i32 {
    match verdict.decision {
        Decision::Allow => {
            match agent {
                Agent::Crush => println!("{{\"decision\":\"allow\"}}"),
                Agent::ClaudeCode => println!(
                    "{{\"hookSpecificOutput\":{{\"hookEventName\":\"PreToolUse\",\"permissionDecision\":\"allow\"}}}}"
                ),
            }
            0
        }
        // confirm：不输出 JSON，走正常权限提示。
        Decision::Confirm => 0,
        Decision::Deny => {
            if let Some(reason) = &verdict.reason {
                eprintln!("{reason}");
            }
            2
        }
    }
}
