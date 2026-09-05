//! 验收（M3.1）：脚本层端到端——沙箱限流兜底、越权 API 不可达、脚本改判
//! 全链路、`--engine` 参数校验。

mod common;

use common::{TempDir, run_check, run_check_with};

fn project_with_script(tag: &str, rules: &str, script: Option<&str>) -> TempDir {
    let proj = TempDir::new(tag);
    let cfg = proj.path().join(".crush-tether");
    std::fs::create_dir_all(&cfg).expect("create .crush-tether");
    std::fs::write(cfg.join("rules.toml"), rules).expect("write rules.toml");
    if let Some(src) = script {
        std::fs::write(cfg.join("rules.rhai"), src).expect("write rules.rhai");
    }
    proj
}

const TOML: &str = concat!(
    "version = 1\n",
    "default = \"confirm\"\n",
    "[local]\n",
    "allow = [\"ls\"]\n",
);

#[test]
fn script_overrides_verdict_end_to_end() {
    // TOML 层放行 ls；脚本把带 --delete 的 ls 升级为 deny（显式枚举写法）。
    let proj = project_with_script(
        "override",
        TOML,
        Some(concat!(
            "fn check(ctx) {",
            "  if ctx.bin == \"ls\" && ctx.args.contains(\"--delete\") { \"deny\" }",
            "  else { \"\" }",
            "}"
        )),
    );
    let r = run_check(proj.path(), "ls");
    assert_eq!(
        r.stdout.trim(),
        "{\"decision\":\"allow\"}",
        "脚本无意见 → 查表裁决"
    );
    let r = run_check(proj.path(), "ls --delete x");
    assert_eq!(r.code, 2, "脚本 deny → exit 2");
    assert!(r.stderr.contains("rules.rhai"), "deny 原因标注脚本来源");
}

#[test]
fn infinite_loop_script_is_bounded_to_confirm() {
    // 死循环脚本被 max_operations 限流 → Err → fail-safe confirm（有界时间）。
    let proj = project_with_script("loop", TOML, Some("fn check(ctx) { while true {} }"));
    let t0 = std::time::Instant::now();
    let r = run_check(proj.path(), "ls");
    assert!(
        t0.elapsed() < std::time::Duration::from_secs(5),
        "必须有界返回"
    );
    assert_eq!(r.code, 0);
    assert!(r.stdout.trim().is_empty(), "fail-safe confirm：不得放行");
    assert!(
        r.stderr.contains("fail-safe confirm"),
        "stderr 告警限流兜底；got: {}",
        r.stderr
    );
}

#[test]
fn broken_script_fails_safe() {
    // 语法错误在编译期暴露 → fail-safe confirm（脚本产生裁决，不能跳过）。
    let proj = project_with_script("broken", TOML, Some("fn check(ctx) { let = ; }"));
    let r = run_check(proj.path(), "ls");
    assert_eq!(r.code, 0);
    assert!(r.stdout.trim().is_empty());
    assert!(
        r.stderr.contains("fail-safe confirm") && r.stderr.contains("rules.rhai"),
        "got: {}",
        r.stderr
    );
}

#[test]
fn no_script_toml_only_still_works() {
    let proj = project_with_script("noscript", TOML, None);
    let r = run_check(proj.path(), "ls");
    assert_eq!(r.stdout.trim(), "{\"decision\":\"allow\"}", "TOML 自足");
}

#[test]
fn unsupported_engine_fails_safe() {
    let proj = project_with_script("engine", TOML, None);
    let r = run_check_with(proj.path(), &["--engine", "lua"], "ls");
    assert_eq!(r.code, 0);
    assert!(
        r.stdout.trim().is_empty(),
        "未知引擎 → confirm，不静默回退 rhai"
    );
    assert!(r.stderr.contains("unsupported engine"), "got: {}", r.stderr);

    let r = run_check_with(proj.path(), &["--engine", "rhai"], "ls");
    assert_eq!(
        r.stdout.trim(),
        "{\"decision\":\"allow\"}",
        "显式 rhai 正常"
    );
}
