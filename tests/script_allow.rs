//! 验收（M4.0）：script_allow 端到端——local/global 声明作用域化逃逸检查、
//! 加载期拒载 fail-safe、deny 终审（二进制管线全链路）。

mod common;

use common::{TempDir, run_check};

fn project_with(tag: &str, rules: &str, script: &str) -> TempDir {
    let proj = TempDir::new(tag);
    let cfg = proj.path().join(".crush-tether");
    std::fs::create_dir_all(&cfg).expect("create .crush-tether");
    std::fs::write(cfg.join("rules.toml"), rules).expect("write rules.toml");
    std::fs::write(cfg.join("rules.rhai"), script).expect("write rules.rhai");
    proj
}

const BASE_TOML: &str = "version = 1\ndefault = \"confirm\"\n";

/// 项目脚本：仅对「参数逃逸」的 ls 激活（写重定向的正当放行场景参数化同此）。
const ESCAPE_SCRIPT: &str = concat!(
    "fn check(ctx) {",
    "  if ctx.bin == \"ls\" && ctx.args.contains(\"../outside\") { return allow(\"ls\"); }",
    "  \"\"",
    "}"
);

#[test]
fn local_declaration_escape_downgrades_to_confirm() {
    // 声明在 [local]：激活后定稿点复查原始参数，逃逸 → confirm。
    let proj = project_with(
        "m4-local",
        &format!("{BASE_TOML}[local]\nallow = [\"ls\"]\nscript_allow = [\"ls\"]\n"),
        ESCAPE_SCRIPT,
    );
    // 仓库内参数 → 激活生效 → allow。
    let r = run_check(proj.path(), "ls inside.txt");
    assert_eq!(
        r.stdout.trim(),
        "{\"decision\":\"allow\"}",
        "local 声明 + 仓库内参数激活放行"
    );
    // 逃逸参数 → 激活被定稿点降级 confirm（静默 + exit 0）。
    let r = run_check(proj.path(), "ls ../outside");
    assert!(r.stdout.trim().is_empty(), "逃逸激活降 confirm");
    assert_eq!(r.code, 0);
}

#[test]
fn global_declaration_exempts_escape_check() {
    // 声明在 [global]（更强的承诺）：激活后豁免逃逸检查 → allow。
    let proj = project_with(
        "m4-global",
        &format!("{BASE_TOML}[local]\nallow = [\"ls\"]\n[global]\nscript_allow = [\"ls\"]\n"),
        ESCAPE_SCRIPT,
    );
    let r = run_check(proj.path(), "ls ../outside");
    assert_eq!(
        r.stdout.trim(),
        "{\"decision\":\"allow\"}",
        "global 声明豁免逃逸检查"
    );
}

#[test]
fn undeclared_allow_rejects_script_and_fails_safe() {
    // 机制 2 端到端：脚本引用未声明 bin → 拒载整个脚本 → 告警 + confirm 兜底。
    let proj = project_with(
        "m4-reject",
        BASE_TOML,
        concat!(
            "fn check(ctx) {",
            "  if ctx.bin == \"curl\" { return allow(\"curl\"); }",
            "  \"\"",
            "}"
        ),
    );
    let r = run_check(proj.path(), "ls");
    assert!(r.stdout.trim().is_empty(), "拒载后 fail-safe confirm");
    assert!(r.stderr.contains("rejected"), "{}", r.stderr);
}

#[test]
fn table_deny_is_final_over_activation() {
    // deny 终审：查表落 deny 的命令，激活一律无效（exit 2）。
    let proj = project_with(
        "m4-deny",
        concat!(
            "version = 1\n",
            "default = \"confirm\"\n",
            "[local]\n",
            "deny = [\"ls\"]\n",
            "script_allow = [\"ls\"]\n"
        ),
        concat!(
            "fn check(ctx) {",
            "  if ctx.bin == \"ls\" { return allow(\"ls\"); }",
            "  \"\"",
            "}"
        ),
    );
    let r = run_check(proj.path(), "ls");
    assert_eq!(r.code, 2, "deny 终审");
}
