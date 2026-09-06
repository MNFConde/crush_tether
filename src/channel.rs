//! Agent 适配层：从 agent 的 hook 调用中取命令、按契约输出裁决。
//!
//! channel 只做「拿命令 / 输出裁决」；分类逻辑在 engine，与 agent 无关。
//! 契约单一事实源：design.md「Agent 适配层（定稿）」。

use std::io::Read;

use serde_json::Value;

use crate::model::{Decision, Verdict};

/// 支持的 agent 适配器。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Agent {
    Crush,
    ClaudeCode,
    /// zcode（M5.3）：hook 协议与 ClaudeCode 同构——信封薄变体复用，
    /// 输入侧按 `ZCODE_PROJECT_DIR`/`CLAUDE_PROJECT_DIR`/stdin `cwd` 容差链
    /// 防御性适配（实机 hook 触发验证待部署时探针，见 ROADMAP M5.3）。
    Zcode,
}

impl Agent {
    /// 从 `--agent` 参数解析。
    pub fn parse(s: &str) -> Option<Self> {
        match s.to_ascii_lowercase().as_str() {
            "crush" => Some(Agent::Crush),
            "claude" | "claudecode" | "claude-code" => Some(Agent::ClaudeCode),
            "zcode" | "zcode-code" => Some(Agent::Zcode),
            _ => None,
        }
    }

    /// 日志 `agent` 字段用名。
    pub fn slug(&self) -> &'static str {
        match self {
            Agent::Crush => "crush",
            Agent::ClaudeCode => "claudecode",
            Agent::Zcode => "zcode",
        }
    }

    /// 项目目录 env 键回退链（zcode 与 ClaudeCode 互为别名，协议同构）。
    fn project_env_keys(&self) -> &'static [&'static str] {
        match self {
            Agent::Crush => &["CRUSH_PROJECT_DIR"],
            Agent::ClaudeCode => &["CLAUDE_PROJECT_DIR", "ZCODE_PROJECT_DIR"],
            Agent::Zcode => &["ZCODE_PROJECT_DIR", "CLAUDE_PROJECT_DIR"],
        }
    }
}

/// 从 stdin JSON / 环境变量提取命令正文与项目根。
///
/// - Crush：stdin `tool_input.command`，env `CRUSH_PROJECT_DIR`。
/// - ClaudeCode：stdin `tool_input.command`；权限基准 stdin `cwd` 优先，
///   回退 `CLAUDE_PROJECT_DIR`（design.md「ClaudeCode 契约（核实）」）。
/// - zcode：同 ClaudeCode 形态，env 键 `ZCODE_PROJECT_DIR` 优先、
///   `CLAUDE_PROJECT_DIR` 回退（模板变量双别名同构）。
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

    let from_stdin = || {
        json.get("cwd")
            .and_then(Value::as_str)
            .map(String::from)
            .filter(|s| !s.is_empty())
    };
    let from_env = || {
        agent
            .project_env_keys()
            .iter()
            .find_map(|k| std::env::var(k).ok().filter(|s| !s.is_empty()))
    };
    // Crush：env 为唯一来源；ClaudeCode：stdin cwd 优先、env 回退；
    // zcode：env 首选（ZCODE_ 别名链），stdin cwd 兜底。
    let project_dir = match agent {
        Agent::Crush => from_env(),
        Agent::ClaudeCode => from_stdin().or_else(from_env),
        Agent::Zcode => from_env().or_else(from_stdin),
    };

    Some(HookInput {
        command,
        project_dir,
    })
}

pub fn emit(verdict: &Verdict, agent: Agent) -> i32 {
    let hook_envelope = |decision: &str| {
        println!(
            "{{\"hookSpecificOutput\":{{\"hookEventName\":\"PreToolUse\",\"permissionDecision\":\"{decision}\"}}}}"
        );
    };
    match verdict.decision {
        Decision::Allow => {
            match agent {
                Agent::Crush => println!("{{\"decision\":\"allow\"}}"),
                Agent::ClaudeCode | Agent::Zcode => hook_envelope("allow"),
            }
            0
        }
        // confirm：Crush 不输出 JSON（走正常权限流程）；ClaudeCode/zcode 吐
        // `permissionDecision:"ask"` 信封（exit 0）。
        Decision::Confirm => {
            if matches!(agent, Agent::ClaudeCode | Agent::Zcode) {
                hook_envelope("ask");
            }
            0
        }
        Decision::Deny => {
            if let Some(reason) = &verdict.reason {
                eprintln!("{reason}");
            }
            2
        }
    }
}
