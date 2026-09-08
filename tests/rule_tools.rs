//! 验收（M7.1）：规则测试工具四件套——`explain` 全溯源、`check --batch`
//! 裁决表、`check --cases` 断言对账、`repl` 调试器（端到端子进程驱动）。
//! 同时钉死 M7.0 四形态语义在工具面上的可见性（读豁免 / 写逃逸标记）。

mod common;

use common::TempDir;

/// 临时项目：cp 在 allow 桶 + 知识库 write_position 标注（M7.0 读写分离
/// 语义的最小配置；deny 节供非 allow 路径断言；无脚本层——TOML 自足）。
fn project_with(tag: &str) -> TempDir {
    let proj = TempDir::new(tag);
    let cfg = proj.path().join(".crush-tether");
    std::fs::create_dir_all(&cfg).expect("create .crush-tether");
    std::fs::write(
        cfg.join("rules.toml"),
        concat!(
            "version = 1\n",
            "default = \"confirm\"\n",
            "[local]\n",
            "allow = [\"cp\"]\n",
            "deny = [\"sudo\"]\n",
            "[local.git]\n",
            "deny.sub = [\"push\"]\n",
        ),
    )
    .expect("write rules.toml");
    std::fs::write(
        cfg.join("knowledge.toml"),
        "version = 1\n[cp]\nwrite_position = \"last\"\n",
    )
    .expect("write knowledge.toml");
    proj
}

/// explain 驱动：命令作位置参数（stdin 关闭——explain 不消费 hook 输入）。
fn run_explain(project: &TempDir, command: &str) -> common::CheckRun {
    let out = std::process::Command::new(common::BIN)
        .args([
            "explain",
            command,
            "--project",
            &project.path().to_string_lossy(),
        ])
        .env_remove("CRUSH_TETHER_CONFIG")
        .env_remove("CRUSH_PROJECT_DIR")
        .output()
        .expect("spawn explain");
    common::CheckRun {
        stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
        code: out.status.code().unwrap_or(-1),
    }
}

#[test]
fn explain_shows_read_exemption_and_write_escape() {
    let proj = project_with("m71-explain");
    // 读外写内：写扫描集 = [.]，逃逸检查 pass → allow。
    let r = run_explain(&proj, "cp ../other/f.txt .");
    assert!(r.stdout.contains("=> allow"), "{}", r.stdout);
    assert!(
        r.stdout.contains("scan = [.]"),
        "写扫描集可见: {}",
        r.stdout
    );
    assert!(r.stdout.contains("escape check: pass"));
    assert!(
        r.stdout
            .contains("lookup: allow <- project.allow (token: cp)")
    );
    // 读内写外：写扫描集 = 目标词，ESCAPES → confirm + reason。
    let r = run_explain(&proj, "cp f.txt ../outside/dst.txt");
    assert!(r.stdout.contains("=> confirm"), "{}", r.stdout);
    assert!(r.stdout.contains("scan = [../outside/dst.txt]"));
    assert!(r.stdout.contains("ESCAPES"));
    assert!(r.stdout.contains("write target escapes repository"));
    // 溯源面：命中层与桶可见（allow 命中被写逃逸降级为 confirm，
    // source.entry 仍指向 allow 桶——产出档与命中桶并列可读）。
    assert!(
        r.stdout
            .contains("lookup: confirm <- project.allow (token: cp)"),
        "{}",
        r.stdout
    );
}

#[test]
fn explain_reports_missing_command_and_config_error() {
    let proj = project_with("m71-noarg");
    // 无位置参数：usage 提示 + exit 2。
    let out = std::process::Command::new(common::BIN)
        .args(["explain", "--project", &proj.path().to_string_lossy()])
        .output()
        .expect("spawn explain");
    assert_eq!(out.status.code(), Some(2));
    // 坏配置：stderr 报错 + exit 2。
    let bad = TempDir::new("m71-badcfg");
    let cfg = bad.path().join(".crush-tether");
    std::fs::create_dir_all(&cfg).expect("mkdir");
    std::fs::write(cfg.join("rules.toml"), "version = 99").expect("write bad rules");
    let out = std::process::Command::new(common::BIN)
        .args(["explain", "ls", "--project", &bad.path().to_string_lossy()])
        .env_remove("CRUSH_TETHER_CONFIG")
        .output()
        .expect("spawn explain");
    assert_eq!(out.status.code(), Some(2));
    assert!(!out.stderr.is_empty(), "配置错误必须有 stderr 输出");
}

#[test]
fn batch_renders_verdict_table() {
    let proj = project_with("m71-batch");
    let mut child = std::process::Command::new(common::BIN)
        .args([
            "check",
            "--batch",
            "--project",
            &proj.path().to_string_lossy(),
        ])
        .env_remove("CRUSH_TETHER_CONFIG")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .spawn()
        .expect("spawn batch");
    use std::io::Write;
    child
        .stdin
        .as_mut()
        .expect("stdin")
        .write_all(b"cp a b\ngit push\n\nsudo x\n")
        .expect("write lines");
    let out = child.wait_with_output().expect("wait");
    let stdout = String::from_utf8_lossy(&out.stdout);
    let lines: Vec<&str> = stdout.lines().collect();
    assert_eq!(lines.len(), 3, "空行跳过: {stdout}");
    assert!(lines[0].starts_with("ALLOW"), "{stdout}");
    assert!(lines[1].starts_with("DENY"), "{stdout}");
    assert!(lines[2].starts_with("DENY"), "{stdout}");
    assert_eq!(out.status.code(), Some(0), "batch 恒 exit 0");
}

#[test]
fn cases_file_reconciles_with_exit_codes() {
    let proj = project_with("m71-cases");
    let cases = proj.path().join("cases.toml");
    std::fs::write(
        &cases,
        concat!(
            "version = 1\n",
            "[[case]]\ncmd = \"cp a b\"\nexpect = \"allow\"\n",
            "[[case]]\ncmd = \"cp a ../outside\"\nexpect = \"confirm\"\n",
            "[[case]]\ncmd = \"sudo x\"\nexpect = \"deny\"\n",
            "[[case]]\ncmd = \"cp a b\"\nexpect = \"deny\"\n", // 故意失败
        ),
    )
    .expect("write cases");
    let out = std::process::Command::new(common::BIN)
        .args([
            "check",
            "--cases",
            &cases.to_string_lossy(),
            "--project",
            &proj.path().to_string_lossy(),
        ])
        .env_remove("CRUSH_TETHER_CONFIG")
        .output()
        .expect("spawn cases");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert_eq!(stdout.matches("PASS").count(), 3, "{stdout}");
    assert_eq!(stdout.matches("FAIL").count(), 1, "{stdout}");
    assert!(stdout.contains("3/4 passed"), "{stdout}");
    assert_eq!(out.status.code(), Some(1), "任一失败 → exit 1");

    // 全过 → exit 0。
    std::fs::write(
        &cases,
        "version = 1\n[[case]]\ncmd = \"cp a b\"\nexpect = \"allow\"\n",
    )
    .expect("rewrite cases");
    let out = std::process::Command::new(common::BIN)
        .args([
            "check",
            "--cases",
            &cases.to_string_lossy(),
            "--project",
            &proj.path().to_string_lossy(),
        ])
        .env_remove("CRUSH_TETHER_CONFIG")
        .output()
        .expect("spawn cases");
    assert_eq!(out.status.code(), Some(0), "全过 → exit 0");
}

#[test]
fn repl_repeats_last_on_empty_line() {
    let proj = project_with("m71-repl");
    let mut child = std::process::Command::new(common::BIN)
        .args(["repl", "--project", &proj.path().to_string_lossy()])
        .env_remove("CRUSH_TETHER_CONFIG")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()
        .expect("spawn repl");
    use std::io::Write;
    child
        .stdin
        .as_mut()
        .expect("stdin")
        .write_all(b"cp a b\n\n:q\n")
        .expect("write repl input");
    let out = child.wait_with_output().expect("wait");
    let stdout = String::from_utf8_lossy(&out.stdout);
    // 空行重复上一条：`cp a b` 被评估两次（两次 `=> allow`）。
    assert_eq!(
        stdout.matches("=> allow").count(),
        2,
        "空行重复上一条: {stdout}"
    );
    assert_eq!(out.status.code(), Some(0));
}
