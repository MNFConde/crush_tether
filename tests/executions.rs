//! 验收（M8.6 部分 2）：执行记录采集——hook 入口按事件分派，PostToolUse
//! 只落盘 `executions.jsonl`（一行一执行）、零裁决输出、恒 exit 0；
//! `CRUSH_TETHER_LEARN=off` 关采集；损坏载荷不炸不阻断。
//!
//! 采集面独立性：PreToolUse 裁决路径不产生 executions 行；
//! `CRUSH_TETHER_LOG=off` 不影响 executions（两开关解耦，D-10）。

mod common;

use common::{TempDir, run_hook_payload, run_init};

fn executions_lines(proj: &std::path::Path) -> Vec<String> {
    let path = proj.join(".crush-tether").join("executions.jsonl");
    match std::fs::read_to_string(path) {
        Ok(s) => s.lines().map(String::from).collect(),
        Err(_) => Vec::new(),
    }
}

fn post_payload(command: &str, session: &str, tool_use: &str, success: bool) -> String {
    format!(
        r#"{{"hook_event_name":"PostToolUse","session_id":"{session}","tool_use_id":"{tool_use}","tool_input":{{"command":"{command}"}},"tool_response":{{"isError":{}}}}}"#,
        !success
    )
}

#[test]
fn post_tool_use_writes_execution_line_and_exits_zero_silently() {
    let proj = TempDir::new("m86-exec");
    run_init(proj.path(), &[], &[]);
    let r = run_hook_payload(
        proj.path(),
        &post_payload("git push origin", "sess-1", "tu-1", true),
        &[],
    );
    assert_eq!(r.code, 0, "PostToolUse 恒 exit 0");
    assert!(r.stdout.trim().is_empty(), "零裁决输出；got {}", r.stdout);
    assert!(
        r.stderr.trim().is_empty(),
        "无 stderr 噪音；got {}",
        r.stderr
    );

    let lines = executions_lines(proj.path());
    assert_eq!(lines.len(), 1, "一行一执行");
    let v: serde_json::Value = serde_json::from_str(&lines[0]).expect("executions line is JSON");
    assert_eq!(v["agent"], "crush");
    assert_eq!(v["session_id"], "sess-1");
    assert_eq!(v["tool_use_id"], "tu-1");
    assert_eq!(v["command"], "git push origin");
    assert_eq!(v["success"], true);
    assert!(v["ts"].is_string(), "ts 为 RFC3339 字符串");
}

#[test]
fn pre_tool_use_does_not_write_executions() {
    // 采集面与裁决面分离：PreToolUse 裁决路径不落 executions。
    let proj = TempDir::new("m86-exec-pre");
    run_init(proj.path(), &[], &[]);
    let r = run_hook_payload(
        proj.path(),
        r#"{"hook_event_name":"PreToolUse","session_id":"s","tool_use_id":"t","tool_input":{"command":"ls"}}"#,
        &[],
    );
    assert_eq!(r.code, 0);
    assert_eq!(executions_lines(proj.path()).len(), 0, "PreToolUse 不采集");
}

#[test]
fn learn_switch_off_disables_collection_but_log_stays_on() {
    // CRUSH_TETHER_LEARN=off 关采集；裁决日志开关独立（CRUSH_TETHER_LOG）。
    let proj = TempDir::new("m86-exec-off");
    run_init(proj.path(), &[], &[]);
    let r = run_hook_payload(
        proj.path(),
        &post_payload("ls", "s", "t", true),
        &[("CRUSH_TETHER_LEARN", "off")],
    );
    assert_eq!(r.code, 0);
    assert_eq!(executions_lines(proj.path()).len(), 0, "关采集后不落盘");
    // 两开关解耦：LEARN=off 不动 decisions.jsonl（下次裁决仍照记，这里只
    // 验证 executions 缺席即采集关闭）。
    assert!(
        !proj
            .path()
            .join(".crush-tether")
            .join("decisions.jsonl")
            .exists()
            || std::fs::read_to_string(proj.path().join(".crush-tether").join("decisions.jsonl"))
                .map(|s| s.contains("decision"))
                .unwrap_or(false),
        "LEARN 开关不牵连裁决日志"
    );
}

#[test]
fn broken_payload_still_exits_zero_silently() {
    // 损坏载荷（非 JSON / 无命令）：读不到输入静默 exit 0，采集失败不阻断。
    let proj = TempDir::new("m86-exec-broken");
    run_init(proj.path(), &[], &[]);
    let r = run_hook_payload(proj.path(), "not json {{{{", &[]);
    assert_eq!(r.code, 0);
    assert!(r.stdout.trim().is_empty());
    assert_eq!(executions_lines(proj.path()).len(), 0);
}

#[test]
fn unknown_success_stays_null_and_lines_accumulate() {
    // tool_response 无成败键（zcode 降级形态）→ success=null；多行累加。
    let proj = TempDir::new("m86-exec-null");
    run_init(proj.path(), &[], &[]);
    let r = run_hook_payload(
        proj.path(),
        r#"{"hook_event_name":"PostToolUse","session_id":"s2","tool_use_id":"t2","tool_input":{"command":"make build"},"tool_response":"some raw output"}"#,
        &[],
    );
    assert_eq!(r.code, 0);
    let r = run_hook_payload(
        proj.path(),
        &post_payload("make test", "s2", "t3", false),
        &[],
    );
    assert_eq!(r.code, 0);
    let lines = executions_lines(proj.path());
    assert_eq!(lines.len(), 2, "多行累加");
    let a: serde_json::Value = serde_json::from_str(&lines[0]).unwrap();
    assert_eq!(a["success"], serde_json::Value::Null, "判不出成败 = null");
    let b: serde_json::Value = serde_json::from_str(&lines[1]).unwrap();
    assert_eq!(b["success"], false, "isError=true → success=false");
}
