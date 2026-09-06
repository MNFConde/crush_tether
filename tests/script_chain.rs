//! 验收：脚本层链——用户层先执行、项目层最后执行可作最终裁决
//! （design.md「配置拆分」：脚本层同文件按优先级，项目脚本最后执行）。
//!
//! run_mode_env 把 USERPROFILE/HOME 隔离到临时项目，故「用户层」即
//! `<project>/.config/crush-tether/rules.rhai`。

mod common;

use common::{TempDir, run_mode_env};

fn user_script(proj: &TempDir, body: &str) {
    let dir = proj.path().join(".config").join("crush-tether");
    std::fs::create_dir_all(&dir).expect("mkdir user config");
    std::fs::write(dir.join("rules.rhai"), body).expect("write user rules.rhai");
}

fn project_layer(proj: &TempDir) -> std::path::PathBuf {
    let cfg = proj.path().join(".crush-tether");
    std::fs::create_dir_all(&cfg).expect("mkdir project config");
    std::fs::write(
        cfg.join("rules.toml"),
        "version = 1\ndefault = \"confirm\"\n[local]\nallow = [\"ls\"]\n",
    )
    .expect("write rules.toml");
    cfg
}

#[test]
fn user_script_runs_first_and_project_script_has_final_say() {
    let proj = TempDir::new("chain-order");
    project_layer(&proj);
    user_script(
        &proj,
        "fn check(ctx) {\n    if ctx.bin == \"ls\" { return decision::CONFIRM; }\n    decision::PASS\n}\n",
    );

    // 仅用户层：ls 查表 allow → 用户脚本升级 confirm（crush 静默 exit 0）。
    let r = run_mode_env(proj.path(), "check", &[], "ls", &[]);
    assert_eq!(r.code, 0, "{}", r.stderr);
    assert!(
        r.stdout.trim().is_empty(),
        "用户层改判 confirm：{}",
        r.stdout
    );

    // 加项目脚本（deny ls）：项目层最后执行 → 最终 deny（可作最终裁决）。
    std::fs::write(
        proj.path().join(".crush-tether").join("rules.rhai"),
        "fn check(ctx) {\n    if ctx.bin == \"ls\" { return decision::DENY; }\n    decision::PASS\n}\n",
    )
    .expect("write project rules.rhai");
    let r = run_mode_env(proj.path(), "check", &[], "ls", &[]);
    assert_eq!(r.code, 2, "项目层脚本最终裁决 deny：{}", r.stderr);

    // 日志：生效裁决出自脚本（source.layer=script），script.file 区分两层。
    let raw = std::fs::read_to_string(proj.path().join(".crush-tether").join("decisions.jsonl"))
        .expect("decisions.jsonl exists");
    let deny_line = raw
        .lines()
        .find(|l| l.contains("\"decision\":\"deny\""))
        .expect("deny verdict logged");
    assert!(deny_line.contains("\"layer\":\"script\""), "{deny_line}");
    assert!(
        deny_line.contains("\"script\":{\"file\":\"rules.rhai\""),
        "script.file 留痕：{deny_line}"
    );
}

#[test]
fn broken_user_script_fails_safe_confirm() {
    let proj = TempDir::new("chain-broken");
    project_layer(&proj);
    // 用户层脚本语法错误：整链拒载 → fail-safe confirm（绝不带病运行）。
    user_script(&proj, "fn check(ctx) { {{{ not rhai");
    let r = run_mode_env(proj.path(), "check", &[], "ls", &[]);
    assert_eq!(r.code, 0);
    assert!(r.stdout.trim().is_empty(), "fail-safe confirm 无输出");
    assert!(
        r.stderr.contains("script layer failed to load"),
        "stderr 告警：{}",
        r.stderr
    );
}
