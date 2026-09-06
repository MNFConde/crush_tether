//! 验收（M5.1–M5.3）：adapter 契约测试参数化——同一用例集驱动
//! Crush / ClaudeCode / zcode 三 adapter，按各自契约断言输出形态。

mod common;

use common::{BIN, TempDir};
use std::io::Write;
use std::process::{Command, Stdio};

/// 三档裁决共用例集：三 adapter 的裁决必须等价（M5.2）。
const CASES: &[(&str, &str)] = &[
    ("ls", "allow"),
    ("sudo x", "deny"),
    ("rm foo.txt", "confirm"),
];

/// 指定 agent 跑一次 check：构造各 agent 的 stdin 载荷与环境。
fn run_agent(agent: &str, project: &std::path::Path, command: &str) -> common::CheckRun {
    let payload = match agent {
        "claudecode" => serde_json::json!({
            "tool_name": "Bash",
            "tool_input": {"command": command},
            "cwd": project,
        })
        .to_string(),
        "zcode" => serde_json::json!({
            "tool_name": "Bash",
            "tool_input": {"command": command}
        })
        .to_string(),
        _ => serde_json::json!({"tool_input": {"command": command}}).to_string(),
    };
    let mut child = Command::new(BIN)
        .args(["check", "--agent", agent])
        .env("CRUSH_PROJECT_DIR", project)
        .env("USERPROFILE", project)
        .env("HOME", project)
        .env_remove("CLAUDE_PROJECT_DIR")
        .env_remove("ZCODE_PROJECT_DIR")
        .env_remove("CRUSH_TETHER_CONFIG")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn check");
    child
        .stdin
        .take()
        .expect("stdin piped")
        .write_all(payload.as_bytes())
        .expect("write hook input");
    let out = child.wait_with_output().expect("wait");
    common::CheckRun {
        stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
        code: out.status.code().unwrap_or(-1),
    }
}

fn project_with_rules(tag: &str) -> TempDir {
    let proj = TempDir::new(tag);
    let cfg = proj.path().join(".crush-tether");
    std::fs::create_dir_all(&cfg).expect("mkdir");
    std::fs::write(
        cfg.join("rules.toml"),
        "version = 1\ndefault = \"confirm\"\n[local]\nallow = [\"ls\"]\ndeny = [\"sudo\"]\n",
    )
    .expect("write rules");
    proj
}

#[test]
fn shared_cases_drive_all_adapters() {
    for agent in ["crush", "claudecode", "zcode"] {
        let proj = project_with_rules(&format!("m52-{agent}"));
        for (command, decision) in CASES {
            let r = run_agent(agent, proj.path(), command);
            match *decision {
                "allow" => {
                    assert_eq!(r.code, 0, "{agent}/{command}");
                    match agent {
                        "crush" => assert_eq!(
                            r.stdout.trim(),
                            "{\"decision\":\"allow\"}",
                            "{agent}/{command}"
                        ),
                        _ => assert!(
                            r.stdout.contains("\"permissionDecision\":\"allow\""),
                            "{agent}/{command}: {}",
                            r.stdout
                        ),
                    }
                }
                "confirm" => {
                    assert_eq!(r.code, 0, "{agent}/{command}");
                    match agent {
                        // Crush：不输出 JSON，走正常权限流程。
                        "crush" => assert!(
                            r.stdout.trim().is_empty(),
                            "{agent}/{command}: {}",
                            r.stdout
                        ),
                        // ClaudeCode/zcode：吐 ask 信封。
                        _ => assert!(
                            r.stdout.contains("\"permissionDecision\":\"ask\""),
                            "{agent}/{command}: {}",
                            r.stdout
                        ),
                    }
                }
                "deny" => {
                    assert_eq!(r.code, 2, "{agent}/{command}");
                    assert!(r.stdout.trim().is_empty(), "{agent}/{command}");
                }
                other => panic!("unknown expectation {other}"),
            }
        }
    }
}

#[test]
fn claudecode_permission_basis_prefers_stdin_cwd() {
    // 权限基准：stdin cwd 优先，回退 CLAUDE_PROJECT_DIR。
    let proj = project_with_rules("m51-cwd");
    // env 指向别处（无规则）→ stdin cwd 的规则生效。
    let other = TempDir::new("m51-cwd-other");
    let payload = serde_json::json!({
        "tool_name": "Bash",
        "tool_input": {"command": "ls"},
        "cwd": proj.path(),
    })
    .to_string();
    let mut child = Command::new(BIN)
        .args(["check", "--agent", "claudecode"])
        .env("CLAUDE_PROJECT_DIR", other.path())
        .env("USERPROFILE", other.path())
        .env("HOME", other.path())
        .env_remove("CRUSH_PROJECT_DIR")
        .env_remove("CRUSH_TETHER_CONFIG")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn");
    child
        .stdin
        .take()
        .expect("stdin")
        .write_all(payload.as_bytes())
        .expect("write");
    let out = child.wait_with_output().expect("wait");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("\"permissionDecision\":\"allow\""),
        "stdin cwd 应作为权限基准：{stdout}"
    );
}

#[test]
fn zcode_project_dir_alias_chain() {
    // M5.3 防御性适配：ZCODE_PROJECT_DIR 优先，CLAUDE_PROJECT_DIR 回退。
    let proj = project_with_rules("m53-zcode");
    for env_key in ["ZCODE_PROJECT_DIR", "CLAUDE_PROJECT_DIR"] {
        let mut child = Command::new(BIN)
            .args(["check", "--agent", "zcode"])
            .env(env_key, proj.path())
            .env("USERPROFILE", proj.path())
            .env("HOME", proj.path())
            .env_remove("CRUSH_PROJECT_DIR")
            .env_remove("CLAUDE_PROJECT_DIR")
            .env_remove("ZCODE_PROJECT_DIR")
            .env_remove("CRUSH_TETHER_CONFIG")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("spawn");
        let payload = "{\"tool_name\":\"Bash\",\"tool_input\":{\"command\":\"ls\"}}";
        child
            .stdin
            .take()
            .expect("stdin")
            .write_all(payload.as_bytes())
            .expect("write");
        let out = child.wait_with_output().expect("wait");
        let stdout = String::from_utf8_lossy(&out.stdout);
        assert!(
            stdout.contains("\"permissionDecision\":\"allow\""),
            "{env_key} 回退链生效：{stdout}"
        );
    }
}
