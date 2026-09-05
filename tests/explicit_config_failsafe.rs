//! 验收（M2.1）：显式覆盖配置（`--config`）损坏/缺失时 → stderr 告警 +
//! fail-safe confirm；有效时正常放行进引擎。
//!
//! 端到端驱动真实二进制（check 模式），观察 stdout/stderr 与退出码：
//! confirm 在两 agent 契约下均为静默 exit 0（无 allow JSON 输出）。

mod common;

use common::{TempDir, run_check_with};
use std::path::PathBuf;

/// 在临时项目内写一份显式覆盖配置（`None` = 指向不存在的文件），
/// 经 `--config` 跑一次 check。
fn run_with_config(tag: &str, config_toml: Option<&str>) -> common::CheckRun {
    let proj = TempDir::new(tag);
    let config_path: PathBuf = proj.path().join("override.toml");
    match config_toml {
        Some(src) => std::fs::write(&config_path, src).expect("write config fixture"),
        None => {
            let _ = std::fs::remove_file(&config_path);
        }
    }
    let extra = ["--config", config_path.to_str().expect("utf-8 path")];
    run_check_with(proj.path(), &extra, "ls")
}

#[test]
fn broken_explicit_config_fails_safe_to_confirm() {
    let r = run_with_config("broken", Some("version = 1\n[local]\nalow = [\"ls\"]"));
    assert_eq!(r.code, 0, "confirm = silent exit 0");
    assert!(
        r.stdout.trim().is_empty(),
        "must not emit allow JSON; got: {}",
        r.stdout
    );
    assert!(
        r.stderr.contains("fail-safe confirm") && r.stderr.contains("alow"),
        "stderr must warn with the locating error; got: {}",
        r.stderr
    );
}

#[test]
fn missing_explicit_config_file_fails_safe_to_confirm() {
    let r = run_with_config("missing", None);
    assert_eq!(r.code, 0);
    assert!(r.stdout.trim().is_empty(), "must not emit allow JSON");
    assert!(
        r.stderr.contains("fail-safe confirm"),
        "explicit-but-missing is a config error, not a silent fallback; got: {}",
        r.stderr
    );
}

#[test]
fn valid_explicit_config_proceeds_into_engine() {
    let r = run_with_config(
        "valid",
        Some(concat!(
            "version = 1\n",
            "default = \"confirm\"\n",
            "[local]\n",
            "allow = [\"ls\"]\n",
        )),
    );
    assert_eq!(r.code, 0);
    assert_eq!(
        r.stdout.trim(),
        "{\"decision\":\"allow\"}",
        "`ls` 命中 [local].allow → 正常 allow"
    );
}
