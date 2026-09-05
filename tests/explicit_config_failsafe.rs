//! 验收（M2.1）：显式覆盖配置（`--config`）损坏时 → stderr 告警 + fail-safe
//! confirm；有效时正常放行进引擎。
//!
//! 端到端驱动真实二进制（check 模式），观察 stdout/stderr 与退出码：
//! confirm 在两 agent 契约下均为静默 exit 0（无 allow JSON 输出）。

use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};

const BIN: &str = env!("CARGO_BIN_EXE_crush-tether");

/// 以给定配置内容跑一次 check（stdin 送 hook JSON），返回 (stdout, stderr, exit)。
/// `tag` 用于并行测试间互不冲突的临时文件名。
fn run_check(tag: &str, config_toml: Option<&str>) -> (String, String, i32) {
    let config_path: PathBuf = std::env::temp_dir().join(format!(
        "crush-tether-m21-test-{}-{tag}.toml",
        std::process::id(),
    ));
    match config_toml {
        Some(src) => std::fs::write(&config_path, src).expect("write config fixture"),
        None => {
            let _ = std::fs::remove_file(&config_path);
        }
    }

    let mut child = Command::new(BIN)
        .args(["check", "--agent", "crush", "--config"])
        .arg(&config_path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn crush-tether check");
    child
        .stdin
        .take()
        .expect("stdin piped")
        .write_all(b"{\"tool_input\":{\"command\":\"ls\"}}")
        .expect("write hook input");
    let out = child.wait_with_output().expect("wait check process");

    let _ = std::fs::remove_file(&config_path);
    (
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
        out.status.code().unwrap_or(-1),
    )
}

#[test]
fn broken_explicit_config_fails_safe_to_confirm() {
    let (stdout, stderr, code) = run_check("broken", Some("version = 1\n[local]\nalow = [\"ls\"]"));
    assert_eq!(code, 0, "confirm = silent exit 0");
    assert!(
        stdout.trim().is_empty(),
        "must not emit allow JSON; got: {stdout}"
    );
    assert!(
        stderr.contains("fail-safe confirm") && stderr.contains("alow"),
        "stderr must warn with the locating error; got: {stderr}"
    );
}

#[test]
fn missing_explicit_config_file_fails_safe_to_confirm() {
    let (stdout, stderr, code) = run_check("missing", None);
    assert_eq!(code, 0);
    assert!(
        stdout.trim().is_empty(),
        "must not emit allow JSON; got: {stdout}"
    );
    assert!(
        stderr.contains("fail-safe confirm"),
        "explicit-but-missing is a config error, not a silent fallback; got: {stderr}"
    );
}

#[test]
fn valid_explicit_config_proceeds_into_engine() {
    let (stdout, _stderr, code) = run_check(
        "valid",
        Some(concat!(
            "version = 1\n",
            "default = \"confirm\"\n",
            "[local]\n",
            "allow = [\"ls\"]\n",
        )),
    );
    assert_eq!(code, 0);
    assert_eq!(
        stdout.trim(),
        "{\"decision\":\"allow\"}",
        "`ls` 命中 [local].allow → 正常 allow（查表语义 M2.3 接入；当前由内置表放行同一结论）"
    );
}
