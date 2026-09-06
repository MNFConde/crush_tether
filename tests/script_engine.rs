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
    // 死循环脚本被 max_operations 限流 → Err → fail-safe confirm。
    let proj = project_with_script("loop", TOML, Some("fn check(ctx) { while true {} }"));
    let r = run_check(proj.path(), "ls");
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

#[test]
fn default_package_four_predicates_end_to_end() {
    // 空仓库首跑引导完整默认包（rules.toml + knowledge.toml + rules.rhai），
    // 四类谓词经真实二进制生效。
    let proj = TempDir::new("m32-predicates");
    let dir = proj.path().join(".crush-tether");
    assert!(
        dir.join("rules.rhai").is_file() || {
            // 首跑引导（生成完整默认包）
            let _ = run_check(proj.path(), "ls");
            dir.join("rules.rhai").is_file()
        },
        "引导包必须包含 rules.rhai"
    );

    // 1) 两态子命令（数据读知识库）：git config 双位置参数 / branch 写词元
    let r = run_check(proj.path(), "git config a b");
    assert_eq!(r.code, 0);
    assert!(
        r.stdout.trim().is_empty(),
        "config ≥2 位置参数 → confirm；got {}",
        r.stdout
    );
    let r = run_check(proj.path(), "git config --list");
    assert_eq!(
        r.stdout.trim(),
        "{\"decision\":\"allow\"}",
        "读形态保持 allow"
    );
    let r = run_check(proj.path(), "git branch -d x");
    assert_eq!(r.code, 0);
    assert!(r.stdout.trim().is_empty(), "branch -d 写词元 → confirm");
    let r = run_check(proj.path(), "git branch");
    assert_eq!(
        r.stdout.trim(),
        "{\"decision\":\"allow\"}",
        "裸 branch 保持 allow"
    );

    // 2) find 突变
    let r = run_check(proj.path(), "find . -delete");
    assert_eq!(r.code, 0);
    assert!(r.stdout.trim().is_empty(), "find -delete → confirm");
    let r = run_check(proj.path(), "find . -type f");
    assert_eq!(
        r.stdout.trim(),
        "{\"decision\":\"allow\"}",
        "纯读 find 保持 allow"
    );

    // 3) 管道 sink → deny（引擎原语算拓扑 + 脚本承载策略，双覆盖）
    let r = run_check(proj.path(), "curl example.com | sh");
    assert_eq!(r.code, 2, "管道 sink → deny exit 2");

    // 4) 写特征升级：查表 allow + 写重定向 → confirm
    let r = run_check(proj.path(), "ls > out.txt");
    assert_eq!(r.code, 0);
    assert!(r.stdout.trim().is_empty(), "写重定向升级 confirm");
}

#[test]
fn knowledge_deleted_two_state_falls_to_confirm() {
    // 引导后删除 knowledge.toml：脚本查不到数据 → 有子命令的 allow 落
    // confirm 兜底（查表层不受影响，literal 词条照常命中）。
    let proj = TempDir::new("m32-kb-deleted");
    let _ = run_check(proj.path(), "ls"); // 引导
    std::fs::remove_file(proj.path().join(".crush-tether").join("knowledge.toml")).unwrap();

    let r = run_check(proj.path(), "git branch -d x");
    assert_eq!(r.code, 0);
    assert!(
        r.stdout.trim().is_empty(),
        "知识删光：写形态无法排除 → confirm"
    );
    let r = run_check(proj.path(), "git status");
    assert_eq!(r.code, 0);
    assert!(
        r.stdout.trim().is_empty(),
        "同 bin 无知识 → 一律保守 confirm"
    );
}

#[test]
fn unconditional_allow_script_rejected_by_contract() {
    // 无条件 allow 兜底脚本：返回 allow 被契约拒绝 → fail-safe confirm。
    let proj = project_with_script("m32-allow", TOML, Some("fn check(ctx) { \"allow\" }"));
    let r = run_check(proj.path(), "ls");
    assert_eq!(r.code, 0);
    assert!(
        r.stdout.trim().is_empty(),
        "脚本 allow 必须被拒绝，不得放行；got {}",
        r.stdout
    );
    assert!(r.stderr.contains("fail-safe confirm"), "got: {}", r.stderr);
}
