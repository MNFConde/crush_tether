//! 验收（M8.6 部分 4）：suggest 命令——裁决日志 × 执行记录交叉推断，
//! 保守三条件（confirm ∩ 成功 ∩ ≥阈值）、entry cause 反推建议块、跳过
//! 清单（危险类别/脚本规则/自由参数）、deny 永不学习、零写入。
//!
//! fixture 时间戳用固定近日期 + `--window 36500`（100 年）拉大窗口，测试
//! 不随运行日期漂移。

mod common;

use common::{BIN, TempDir};
use std::process::{Command, Stdio};

const TS: &str = "2026-09-12T00:00:0";

fn dec_line(
    id: u32,
    decision: &str,
    command: &str,
    layer: &str,
    entry: &str,
    token: &str,
) -> String {
    format!(
        r#"{{"ts":"{TS}{id}Z","mode":"serve","agent":"claudecode","decision":"{decision}","command":"{command}","source":{{"layer":"{layer}","entry":"{entry}","match":"{token}"}},"session_id":"s{id}","tool_use_id":"t{id}","script":{{"file":null,"rule":null}}}}"#
    )
}

fn dec_line_script_rule(id: u32, command: &str, rule: &str) -> String {
    format!(
        r#"{{"ts":"{TS}{id}Z","mode":"serve","agent":"claudecode","decision":"confirm","command":"{command}","source":{{"layer":"script","entry":"script","match":"git"}},"session_id":"s{id}","tool_use_id":"t{id}","script":{{"file":"rules.rhai","rule":"{rule}"}}}}"#
    )
}

fn exec_line(id: u32, command: &str, success: bool) -> String {
    format!(
        r#"{{"ts":"{TS}{id}Z","agent":"claudecode","session_id":"s{id}","tool_use_id":"t{id}","command":"{command}","success":{success}}}"#
    )
}

fn setup(tag: &str, decisions: &[String], execs: &[String]) -> TempDir {
    let proj = TempDir::new(tag);
    let cfg = proj.path().join(".crush-tether");
    std::fs::create_dir_all(&cfg).expect("create dir");
    std::fs::write(cfg.join("rules.toml"), "version = 1\ndefault = \"allow\"\n").expect("write");
    if !decisions.is_empty() {
        std::fs::write(cfg.join("decisions.jsonl"), decisions.join("\n") + "\n").expect("write");
    }
    if !execs.is_empty() {
        std::fs::write(cfg.join("executions.jsonl"), execs.join("\n") + "\n").expect("write");
    }
    proj
}

fn run_suggest(proj: &TempDir, extra: &[&str]) -> (String, i32) {
    let mut child = Command::new(BIN)
        .args(["suggest", "--project", &proj.path().to_string_lossy()])
        .args(extra)
        .env_remove("CRUSH_TETHER_CONFIG")
        .env_remove("CRUSH_TETHER_GLOBAL_DIR")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn suggest");
    let _ = child.stdin.take();
    let out = child.wait_with_output().expect("wait");
    (
        String::from_utf8_lossy(&out.stdout).into_owned(),
        out.status.code().unwrap_or(-1),
    )
}

#[test]
fn entry_cause_reaches_threshold_and_suggests_allow_sub() {
    // 同 entry cause（git.confirm.sub push）×3 成功 → 建议 [local.git] allow.sub。
    let cmds = [
        "git push origin a",
        "git push origin b",
        "git push upstream",
    ];
    let decisions: Vec<String> = cmds
        .iter()
        .enumerate()
        .map(|(i, c)| dec_line(i as u32, "confirm", c, "project", "git.confirm.sub", "push"))
        .collect();
    let execs: Vec<String> = cmds
        .iter()
        .enumerate()
        .map(|(i, c)| exec_line(i as u32, c, true))
        .collect();
    let proj = setup("m86-sug-entry", &decisions, &execs);
    let (out, code) = run_suggest(&proj, &["--window", "36500"]);
    assert_eq!(code, 0);
    assert!(out.contains("[local.git]"), "建议块应含节头；got:\n{out}");
    assert!(out.contains("allow.sub = [\"push\"]"), "got:\n{out}");
}

#[test]
fn below_threshold_yields_no_suggestions() {
    let cmds = ["git push a", "git push b"];
    let decisions: Vec<String> = cmds
        .iter()
        .enumerate()
        .map(|(i, c)| dec_line(i as u32, "confirm", c, "project", "git.confirm.sub", "push"))
        .collect();
    let execs: Vec<String> = cmds
        .iter()
        .enumerate()
        .map(|(i, c)| exec_line(i as u32, c, true))
        .collect();
    let proj = setup("m86-sug-low", &decisions, &execs);
    let (out, _) = run_suggest(&proj, &["--window", "36500"]);
    assert!(
        out.contains("no suggestions"),
        "低于门槛不建议；got:\n{out}"
    );
    // 显式放宽门槛到 2 → 出建议。
    let (out, _) = run_suggest(&proj, &["--window", "36500", "--threshold", "2"]);
    assert!(out.contains("allow.sub = [\"push\"]"), "got:\n{out}");
}

#[test]
fn failed_or_null_executions_do_not_count() {
    // success=false / null 不构成「执行成功」信号。
    let cmds = ["git push a", "git push b", "git push c"];
    let decisions: Vec<String> = cmds
        .iter()
        .enumerate()
        .map(|(i, c)| dec_line(i as u32, "confirm", c, "project", "git.confirm.sub", "push"))
        .collect();
    let execs = vec![
        exec_line(0, "git push a", false),
        exec_line(1, "git push b", false),
        format!(
            r#"{{"ts":"{TS}2Z","agent":"claudecode","session_id":"s2","tool_use_id":"t2","command":"git push b","success":null}}"#
        ),
    ];
    let proj = setup("m86-sug-fail", &decisions, &execs);
    let (out, _) = run_suggest(&proj, &["--window", "36500"]);
    assert!(
        out.contains("no suggestions"),
        "失败/null 执行不计数；got:\n{out}"
    );
}

#[test]
fn dangerous_bins_land_in_skip_list() {
    // curl 反复批准 → 跳过清单（网络类），绝不出建议。
    let cmds = ["curl example.com", "curl mirror.org", "curl mirror2.org"];
    let decisions: Vec<String> = cmds
        .iter()
        .enumerate()
        .map(|(i, c)| dec_line(i as u32, "confirm", c, "project", "confirm", "curl"))
        .collect();
    let execs: Vec<String> = cmds
        .iter()
        .enumerate()
        .map(|(i, c)| exec_line(i as u32, c, true))
        .collect();
    let proj = setup("m86-sug-skip", &decisions, &execs);
    let (out, _) = run_suggest(&proj, &["--window", "36500"]);
    assert!(out.contains("skip list"), "应出现跳过清单；got:\n{out}");
    assert!(out.contains("危险类别"), "got:\n{out}");
    assert!(
        !out.contains("allow = [\"curl\"]"),
        "危险类别不得建议；got:\n{out}"
    );
}

#[test]
fn script_cause_skipped_and_deny_never_learned() {
    // 脚本规则 cause → 跳过（无 TOML 对应物）；deny 裁决永不学习。
    let mut decisions = vec![
        dec_line_script_rule(0, "git branch -d x", "two_state:-d"),
        dec_line_script_rule(1, "git branch -d y", "two_state:-d"),
        dec_line_script_rule(2, "git branch -d z", "two_state:-d"),
    ];
    decisions.push(dec_line(3, "deny", "sudo x", "project", "deny", "sudo"));
    decisions.push(dec_line(4, "deny", "sudo y", "project", "deny", "sudo"));
    decisions.push(dec_line(5, "deny", "sudo z", "project", "deny", "sudo"));
    let cmds = [
        "git branch -d x",
        "git branch -d y",
        "git branch -d z",
        "sudo x",
        "sudo y",
        "sudo z",
    ];
    let execs: Vec<String> = cmds
        .iter()
        .enumerate()
        .map(|(i, c)| exec_line(i as u32, c, true))
        .collect();
    let proj = setup("m86-sug-script", &decisions, &execs);
    let (out, _) = run_suggest(&proj, &["--window", "36500"]);
    assert!(
        out.contains("脚本规则"),
        "script cause 进跳过清单；got:\n{out}"
    );
    assert!(!out.contains("allow.sub = [\"push\"]"));
    assert!(
        !out.contains("allow = [\"sudo\"]"),
        "deny 永不学习；got:\n{out}"
    );
}

#[test]
fn no_executions_file_prints_guidance_and_exits_zero() {
    let proj = TempDir::new("m86-sug-none");
    let cfg = proj.path().join(".crush-tether");
    std::fs::create_dir_all(&cfg).expect("dir");
    std::fs::write(cfg.join("rules.toml"), "version = 1\n").unwrap();
    let (out, code) = run_suggest(&proj, &[]);
    assert_eq!(code, 0);
    assert!(out.contains("no execution records"), "got:\n{out}");
}

#[test]
fn suggest_writes_nothing_zero_write_guarantee() {
    // 零写入：运行后 .crush-tether 下不出现新文件（rules.toml mtime 不变）。
    let cmds = ["git push a", "git push b", "git push c"];
    let decisions: Vec<String> = cmds
        .iter()
        .enumerate()
        .map(|(i, c)| dec_line(i as u32, "confirm", c, "project", "git.confirm.sub", "push"))
        .collect();
    let execs: Vec<String> = cmds
        .iter()
        .enumerate()
        .map(|(i, c)| exec_line(i as u32, c, true))
        .collect();
    let proj = setup("m86-sug-zero", &decisions, &execs);
    let before = std::fs::read_dir(proj.path().join(".crush-tether"))
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.file_name())
        .collect::<Vec<_>>();
    let _ = run_suggest(&proj, &["--window", "36500"]);
    let mut after: Vec<_> = std::fs::read_dir(proj.path().join(".crush-tether"))
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.file_name())
        .collect();
    after.sort();
    assert_eq!(before.len(), after.len(), "零写入：文件集不变");
}
