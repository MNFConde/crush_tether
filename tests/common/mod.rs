#![allow(dead_code)]
//! 集成测试公共助手：临时项目目录 + check 模式子进程驱动。
//!
//! 三次法则（`script/AGENTS.md`）：同一验证逻辑第 3 次出现即固化——
//! check 子进程驱动助手自 M2.7 起收敛于此，后续测试一律复用。

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

pub const BIN: &str = env!("CARGO_BIN_EXE_crush-tether");

/// 自清理的临时目录（并行测试用 tag 隔离文件名）。
pub struct TempDir(PathBuf);

impl TempDir {
    pub fn new(tag: &str) -> Self {
        let d =
            std::env::temp_dir().join(format!("crush-tether-test-{}-{tag}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).expect("create temp dir");
        TempDir(d)
    }

    pub fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// 一次 check 运行的可观测输出。
pub struct CheckRun {
    pub stdout: String,
    pub stderr: String,
    pub code: i32,
}

/// 在 `project` 内跑一次 check：环境隔离（CRUSH_PROJECT_DIR=project、
/// USERPROFILE/HOME=project、无 CRUSH_TETHER_CONFIG），`extra_args` 附加
/// CLI 参数（如 `["--config", path]`），stdin 送 hook JSON。
pub fn run_check_with(project: &Path, extra_args: &[&str], command: &str) -> CheckRun {
    run_check_env(project, extra_args, command, &[])
}

/// 同 [`run_check_with`]，额外覆盖环境变量（在隔离缺省之后应用，
/// 如把 HOME 指到另一临时目录模拟用户层）。
pub fn run_check_env(
    project: &Path,
    extra_args: &[&str],
    command: &str,
    envs: &[(&str, &str)],
) -> CheckRun {
    let mut child = Command::new(BIN)
        .args(["check", "--agent", "crush"])
        .args(extra_args)
        .env("CRUSH_PROJECT_DIR", project)
        .env("USERPROFILE", project)
        .env("HOME", project)
        .env_remove("CRUSH_TETHER_CONFIG")
        .envs(envs.iter().copied())
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
    CheckRun {
        stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
        code: out.status.code().unwrap_or(-1),
    }
}

/// 无附加参数的便捷形态。
pub fn run_check(project: &Path, command: &str) -> CheckRun {
    run_check_with(project, &[], command)
}
