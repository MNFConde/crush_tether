//! 验收（M4.1）：命名端点 serve + hook connect-or-spawn。
//!
//! - 独占 bind 单实例裁定：并发冷启动惊群收敛单实例（输者静默退出 0）。
//! - idle 退出：连接归零空闲超 grace 自动退出。
//! - hook 主路径：connect-or-spawn 出裁决；降级路径仍出裁决绝不放行。
//! - benchmark：双跑（in-process vs serve 路径）diff 为空。
//!
//! 测试钩子：`CRUSH_TETHER_IDLE_EXIT` 覆盖 spawn 的 serve 空闲退出秒数
//! （测试结束快速自清理）；`CRUSH_TETHER_DISABLE_SERVE=1` 强制降级路径。

mod common;

use std::time::{Duration, Instant};

use common::{KillOnDrop, TempDir, run_mode_env, spawn_serve};

const CHECK_INTERVAL: Duration = Duration::from_millis(100);

fn try_wait_all(children: &mut [&mut KillOnDrop]) -> bool {
    children
        .iter_mut()
        .all(|c| c.0.try_wait().expect("try_wait").is_some())
}

#[test]
fn thundering_herd_converges_to_single_instance() {
    let proj = TempDir::new("m41-herd");
    // c1/c2/c3 之一是赢家；c2/c3 已确认退出。c1 持有候选赢家，Drop 时回收。
    let _c1 = spawn_serve(proj.path(), "2", None);
    let mut c2 = spawn_serve(proj.path(), "2", None);
    let mut c3 = spawn_serve(proj.path(), "2", None);

    // 输者（除赢家外的两个）应在有界时间内静默退出。
    let deadline = Instant::now() + Duration::from_secs(10);
    while Instant::now() < deadline {
        if try_wait_all(&mut [&mut c2, &mut c3]) {
            break;
        }
        std::thread::sleep(CHECK_INTERVAL);
    }
    let code2 = c2.0.try_wait().expect("try_wait").and_then(|s| s.code());
    let code3 = c3.0.try_wait().expect("try_wait").and_then(|s| s.code());
    assert_eq!(code2, Some(0), "输者静默退出 0");
    assert_eq!(code3, Some(0), "输者静默退出 0");

    // 赢家存活着服务本项目：hook 路径连上并出裁决（ls → 默认包 allow）。
    let r = run_mode_env(proj.path(), "hook", &[], "ls", &[]);
    assert_eq!(
        r.stdout.trim(),
        "{\"decision\":\"allow\"}",
        "hook 经 serve 出裁决"
    );
    // 清理由 KillOnDrop 兜底（idle-exit 亦会回收）。
}

#[test]
fn serve_exits_after_idle_grace() {
    let proj = TempDir::new("m41-idle");
    let mut c = spawn_serve(proj.path(), "1", None);
    let deadline = Instant::now() + Duration::from_secs(8);
    let mut exited = None;
    while Instant::now() < deadline {
        if let Some(status) = c.0.try_wait().expect("try_wait") {
            exited = Some(status.code());
            break;
        }
        std::thread::sleep(CHECK_INTERVAL);
    }
    assert_eq!(exited, Some(Some(0)), "空闲超 grace 自动退出（exit 0）");
}

#[test]
fn hook_mode_end_to_end_via_serve() {
    let proj = TempDir::new("m41-hook");
    // 预写项目层规则：ls 入 deny，验证 serve 装载的是项目配置。
    let cfg = proj.path().join(".crush-tether");
    std::fs::create_dir_all(&cfg).expect("mkdir");
    std::fs::write(
        cfg.join("rules.toml"),
        "version = 1\ndefault = \"confirm\"\n[local]\ndeny = [\"ls\"]\n",
    )
    .expect("write rules");
    // CRUSH_TETHER_IDLE_EXIT=1：hook spawn 的 serve 快速自清理。
    let r = run_mode_env(
        proj.path(),
        "hook",
        &[],
        "ls",
        &[("CRUSH_TETHER_IDLE_EXIT", "1")],
    );
    assert_eq!(r.code, 2, "hook 全链路：serve 装载项目规则 → deny exit 2");
    // 再跑一条 confirm（静默 exit 0）。
    let r = run_mode_env(
        proj.path(),
        "hook",
        &[],
        "curl http://x.com",
        &[("CRUSH_TETHER_IDLE_EXIT", "1")],
    );
    assert!(r.stdout.trim().is_empty(), "confirm 静默");
    assert_eq!(r.code, 0);
}

#[test]
fn degraded_hook_path_still_decides_never_allows_blindly() {
    let proj = TempDir::new("m41-degrade");
    let cfg = proj.path().join(".crush-tether");
    std::fs::create_dir_all(&cfg).expect("mkdir");
    // 禁用 serve（逃生口/测试钩子）→ hook 降级 in-process，deny 仍生效。
    std::fs::write(
        cfg.join("rules.toml"),
        "version = 1\ndefault = \"confirm\"\n[local]\ndeny = [\"ls\"]\n",
    )
    .expect("write rules");
    let r = run_mode_env(
        proj.path(),
        "hook",
        &[],
        "ls",
        &[("CRUSH_TETHER_DISABLE_SERVE", "1")],
    );
    assert_eq!(r.code, 2, "降级路径仍出裁决");
    // 损坏配置：降级路径 fail-safe confirm，绝不放行。
    std::fs::write(cfg.join("rules.toml"), "not toml {{{{").expect("write broken");
    let r = run_mode_env(
        proj.path(),
        "hook",
        &[],
        "ls",
        &[("CRUSH_TETHER_DISABLE_SERVE", "1")],
    );
    assert!(r.stdout.trim().is_empty(), "fail-safe confirm 静默");
    assert_eq!(r.code, 0);
    assert!(r.stderr.contains("failed to load"), "{}", r.stderr);
}

#[test]
fn benchmark_double_run_diff_is_empty() {
    let proj = TempDir::new("m41-bench");
    // hook 路径（必要时 spawn serve）与 in-process 全量管线同裁决。
    let r = run_mode_env(
        proj.path(),
        "benchmark",
        &[],
        "ls",
        &[("CRUSH_TETHER_IDLE_EXIT", "1")],
    );
    assert!(r.stdout.contains("\"match\":true"), "{}", r.stdout);
    assert_eq!(r.code, 0, "benchmark 双跑 diff 为空");
}

#[test]
fn serve_path_honors_explicit_config() {
    // serve 与 hook 同带 --config：端点名含 config 维度，serve 加载显式
    // 覆盖裁决（deny）——显式配置接入 serve 主路径的端到端验证。
    let proj = TempDir::new("m41-explicit-serve");
    let ext = proj.path().join("override-rules.toml");
    std::fs::write(
        &ext,
        "version = 1\ndefault = \"confirm\"\n[local]\ndeny = [\"ls\"]\n",
    )
    .expect("write override rules");
    let _serve = spawn_serve(proj.path(), "30", Some(ext.to_string_lossy().as_ref()));

    let r = run_mode_env(
        proj.path(),
        "hook",
        &["--config", ext.to_string_lossy().as_ref()],
        "ls",
        &[],
    );
    assert_eq!(r.code, 2, "serve 按显式配置裁决 → deny：{}", r.stderr);
}
