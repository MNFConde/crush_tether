//! 验收（M2.3）：check 模式端到端——三层发现 → 字段级继承合并 → 查表裁决。
//!
//! 用 CRUSH_PROJECT_DIR 指向临时样例仓库（含 `.crush-tether/rules.toml`），
//! 子进程环境隔离（HOME/USERPROFILE 同指临时目录，排除真实用户层干扰）。

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

const BIN: &str = env!("CARGO_BIN_EXE_crush-tether");

struct TempProject(PathBuf);

impl TempProject {
    fn new(tag: &str, rules: &str) -> Self {
        let d = std::env::temp_dir().join(format!("crush-tether-m23-{}-{tag}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        let cfg = d.join(".crush-tether");
        std::fs::create_dir_all(&cfg).expect("create .crush-tether");
        std::fs::write(cfg.join("rules.toml"), rules).expect("write rules.toml");
        TempProject(d)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempProject {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// 在样例仓库内跑一次 check，返回 (stdout, exit code)。
fn run_check(project: &Path, command: &str) -> (String, i32) {
    let mut child = Command::new(BIN)
        .args(["check", "--agent", "crush"])
        .env("CRUSH_PROJECT_DIR", project)
        .env("USERPROFILE", project)
        .env("HOME", project)
        .env_remove("CRUSH_TETHER_CONFIG")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn crush-tether check");
    let payload = format!("{{\"tool_input\":{{\"command\":\"{command}\"}}}}");
    child
        .stdin
        .take()
        .expect("stdin piped")
        .write_all(payload.as_bytes())
        .expect("write hook input");
    let out = child.wait_with_output().expect("wait check process");
    (
        String::from_utf8_lossy(&out.stdout).into_owned(),
        out.status.code().unwrap_or(-1),
    )
}

const RULES: &str = concat!(
    "version = 1\n",
    "default = \"confirm\"\n",
    "[local]\n",
    "allow = [\"ls\"]\n",
    "deny = [\"sudo\"]\n",
);

#[test]
fn discovered_rules_drive_check_mode_end_to_end() {
    let proj = TempProject::new("e2e", RULES);

    // 命中 [local].allow → allow JSON（此前经内置判定表，现走查表）。
    let (stdout, code) = run_check(proj.path(), "ls");
    assert_eq!(code, 0);
    assert_eq!(stdout.trim(), "{\"decision\":\"allow\"}");

    // 命中 deny → exit 2、无 stdout。
    let (stdout, code) = run_check(proj.path(), "sudo rm -rf x");
    assert_eq!(code, 2, "deny = exit 2");
    assert!(stdout.trim().is_empty());

    // 未命中 → default confirm：静默 exit 0（走 agent 正常权限提示）。
    let (stdout, code) = run_check(proj.path(), "mystery-cli x");
    assert_eq!(code, 0);
    assert!(stdout.trim().is_empty());
}

#[test]
fn knowledge_alias_normalization_end_to_end() {
    // 规则 allow pip；知识库 pip3 → pip；`pip3 --version` 经归一命中 allow。
    let proj = TempProject::new(
        "kb",
        concat!(
            "version = 1\n",
            "default = \"confirm\"\n",
            "[local]\n",
            "allow = [\"pip\"]\n",
        ),
    );
    std::fs::write(
        proj.path().join(".crush-tether").join("knowledge.toml"),
        "version = 1\n[pip3]\nalias_of = \"pip\"\n",
    )
    .expect("write knowledge.toml");

    let (stdout, code) = run_check(proj.path(), "pip3 --version");
    assert_eq!(code, 0);
    assert_eq!(
        stdout.trim(),
        "{\"decision\":\"allow\"}",
        "别名归一后命中 allow"
    );

    // 无归一即不可命中的对照：npm 不在 allow → default confirm。
    let (stdout, code) = run_check(proj.path(), "npm --version");
    assert_eq!(code, 0);
    assert!(stdout.trim().is_empty(), "未命中走 default confirm（静默）");
}

#[test]
fn broken_knowledge_degrades_to_literal_lookup_not_fail_safe() {
    // 知识库损坏 ≠ 规则损坏：判定不受影响（按字面查表），仅 stderr 告警。
    let proj = TempProject::new(
        "kb-broken",
        concat!(
            "version = 1\n",
            "default = \"confirm\"\n",
            "[local]\n",
            "allow = [\"pip3\"]\n",
        ),
    );
    std::fs::write(
        proj.path().join(".crush-tether").join("knowledge.toml"),
        "version = 1\n[pip3]\nalist_of = \"pip\"\n",
    )
    .expect("write broken knowledge.toml");

    let (stdout, code) = run_check(proj.path(), "pip3 --version");
    assert_eq!(code, 0);
    assert_eq!(
        stdout.trim(),
        "{\"decision\":\"allow\"}",
        "知识库损坏只降级归一能力，裁决仍按字面配置生效"
    );
}
