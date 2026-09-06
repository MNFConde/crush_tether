//! 验收（M4.2）：热重载——改规则文件不重启即生效；坏文件保留旧快照、
//! 无半更新；监听降级路径（stat 三重校验）正确性不损。

mod common;

use std::path::Path;
use std::time::{Duration, Instant};

use common::{TempDir, run_mode_env, spawn_serve};

fn hook(project: &Path, command: &str) -> (String, i32) {
    let r = run_mode_env(project, "hook", &[], command, &[]);
    (r.stdout, r.code)
}

fn wait_reload() {
    // debounce 600ms + 通知传播 + 余量。
    std::thread::sleep(Duration::from_millis(1500));
}

#[test]
fn hot_reload_picks_up_rule_changes_without_restart() {
    let proj = TempDir::new("m42-reload");
    let _serve = spawn_serve(proj.path(), "30");

    // 初始：默认包生成，ls → allow。
    let (out, code) = hook(proj.path(), "ls");
    assert_eq!(out.trim(), "{\"decision\":\"allow\"}", "初始 allow：{code}");

    let rules = proj.path().join(".crush-tether").join("rules.toml");
    // 改规则：ls → deny。不重启 serve。
    std::fs::write(
        &rules,
        "version = 1\ndefault = \"confirm\"\n[local]\ndeny = [\"ls\"]\n",
    )
    .expect("write deny rules");
    wait_reload();
    let (out, code) = hook(proj.path(), "ls");
    assert!(out.trim().is_empty() && code == 2, "改规则即生效 → deny");

    // 坏文件期间：重载失败保留旧快照 → 仍 deny（绝不半更新、绝不误放行）。
    std::fs::write(&rules, "not toml {{{{").expect("write broken rules");
    wait_reload();
    let (out, code) = hook(proj.path(), "ls");
    assert!(
        out.trim().is_empty() && code == 2,
        "坏文件保留旧快照 → 仍 deny"
    );

    // 修复为 allow → 再生效。
    std::fs::write(
        &rules,
        "version = 1\ndefault = \"confirm\"\n[local]\nallow = [\"ls\"]\n",
    )
    .expect("write allow rules");
    wait_reload();
    let (out, _code) = hook(proj.path(), "ls");
    assert_eq!(out.trim(), "{\"decision\":\"allow\"}", "修复后回到 allow");
}

#[test]
fn hot_reload_waits_for_write_quiescence() {
    // debounce：连续写入（编辑器 temp-rename 模拟）聚成一次重载，最终状态生效。
    let proj = TempDir::new("m42-debounce");
    let _serve = spawn_serve(proj.path(), "30");
    // 等 serve 完成引导生成（同步点）。
    let (out, _) = hook(proj.path(), "ls");
    assert_eq!(out.trim(), "{\"decision\":\"allow\"}");
    let rules = proj.path().join(".crush-tether").join("rules.toml");
    for _ in 0..3 {
        std::fs::write(
            &rules,
            "version = 1\ndefault = \"confirm\"\n[local]\nconfirm = [\"ls\"]\n",
        )
        .expect("write");
        std::thread::sleep(Duration::from_millis(100));
    }
    // 在 debounce 窗口内的请求仍用旧快照（默认包 allow），随后新快照生效。
    let deadline = Instant::now() + Duration::from_secs(6);
    loop {
        let (out, code) = hook(proj.path(), "ls");
        if out.trim().is_empty() && code == 0 {
            break; // confirm 生效 → 新快照已换上
        }
        assert!(
            Instant::now() < deadline,
            "debounce 后新快照应生效（confirm 静默）"
        );
        std::thread::sleep(Duration::from_millis(300));
    }
}
