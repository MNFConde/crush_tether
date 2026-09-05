//! 验收（M2.3/M2.4）：check 模式端到端——三层发现 → 字段级继承合并 →
//! 知识库归一 → 查表裁决；样例仓库内自定义规则改变裁决。

mod common;

use common::{TempDir, run_check};

const RULES: &str = concat!(
    "version = 1\n",
    "default = \"confirm\"\n",
    "[local]\n",
    "allow = [\"ls\"]\n",
    "deny = [\"sudo\"]\n",
);

fn project_with_rules(tag: &str, rules: &str) -> TempDir {
    let proj = TempDir::new(tag);
    let cfg = proj.path().join(".crush-tether");
    std::fs::create_dir_all(&cfg).expect("create .crush-tether");
    std::fs::write(cfg.join("rules.toml"), rules).expect("write rules.toml");
    proj
}

#[test]
fn discovered_rules_drive_check_mode_end_to_end() {
    let proj = project_with_rules("e2e", RULES);

    // 命中 [local].allow → allow JSON（查表主路径）。
    let r = run_check(proj.path(), "ls");
    assert_eq!(r.code, 0);
    assert_eq!(r.stdout.trim(), "{\"decision\":\"allow\"}");

    // 命中 deny → exit 2、无 stdout。
    let r = run_check(proj.path(), "sudo rm -rf x");
    assert_eq!(r.code, 2, "deny = exit 2");
    assert!(r.stdout.trim().is_empty());

    // 未命中 → default confirm：静默 exit 0（走 agent 正常权限提示）。
    let r = run_check(proj.path(), "mystery-cli x");
    assert_eq!(r.code, 0);
    assert!(r.stdout.trim().is_empty());
}

#[test]
fn knowledge_alias_normalization_end_to_end() {
    // 规则 allow pip；知识库 pip3 → pip；`pip3 --version` 经归一命中 allow。
    let proj = project_with_rules(
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

    let r = run_check(proj.path(), "pip3 --version");
    assert_eq!(r.code, 0);
    assert_eq!(
        r.stdout.trim(),
        "{\"decision\":\"allow\"}",
        "别名归一后命中 allow"
    );

    // 无归一即不可命中的对照：npm 不在 allow → default confirm。
    let r = run_check(proj.path(), "npm --version");
    assert_eq!(r.code, 0);
    assert!(
        r.stdout.trim().is_empty(),
        "未命中走 default confirm（静默）"
    );
}

#[test]
fn broken_knowledge_degrades_to_literal_lookup_not_fail_safe() {
    // 知识库损坏 ≠ 规则损坏：判定不受影响（按字面查表），仅 stderr 告警。
    let proj = project_with_rules(
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

    let r = run_check(proj.path(), "pip3 --version");
    assert_eq!(r.code, 0);
    assert_eq!(
        r.stdout.trim(),
        "{\"decision\":\"allow\"}",
        "知识库损坏只降级归一能力，裁决仍按字面配置生效"
    );
}
