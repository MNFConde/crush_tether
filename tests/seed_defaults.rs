//! 验收（M2.6）：默认配置生成 v1——三层皆缺才生成；损坏不生成；幂等且
//! 并发收敛；模板与 design.md 示例逐行一致（文档 = 单一事实源）。

mod common;

use common::{TempDir, run_check};
use crush_tether::config::seed::{DEFAULT_KNOWLEDGE_TOML, DEFAULT_RULES_TOML};
use std::path::Path;

const DESIGN_MD: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/doc/design.md");

fn extract_example(md: &str, section: &str) -> String {
    let section = md.split(section).nth(1).expect("section exists");
    let start = section.find("```toml").expect("toml block") + "```toml".len();
    let rest = &section[start..];
    let end = rest.find("```").expect("closed");
    rest[..end].to_string()
}

#[test]
fn templates_match_design_md_examples_byte_for_byte() {
    // 行尾归一：Windows 工作区 design.md 为 CRLF、模板为 LF，逐行内容必须一致。
    let md = std::fs::read_to_string(Path::new(DESIGN_MD))
        .expect("read design.md")
        .replace('\r', "");
    assert_eq!(
        DEFAULT_RULES_TOML.trim(),
        extract_example(&md, "### `rules.toml` 结构").trim(),
        "默认 rules.toml 模板必须与 design.md 示例一致"
    );
    assert_eq!(
        DEFAULT_KNOWLEDGE_TOML.trim(),
        extract_example(&md, "### 命令知识库（bucket 框架，定稿）").trim(),
        "默认 knowledge.toml 模板必须与 design.md 示例一致"
    );
}

#[test]
fn all_absent_project_bootstraps_defaults_end_to_end() {
    let proj = TempDir::new("bootstrap");
    // 默认包 [local].allow 含 ls → 引导后 ls 放行。
    let r = run_check(proj.path(), "ls");
    assert_eq!(r.code, 0);
    assert_eq!(
        r.stdout.trim(),
        "{\"decision\":\"allow\"}",
        "引导后按默认包裁决"
    );
    assert!(
        r.stderr.contains("seeded defaults"),
        "首次运行应有引导告警；got: {}",
        r.stderr
    );
    let dir = proj.path().join(".crush-tether");
    assert!(dir.join("rules.toml").is_file());
    assert!(dir.join("knowledge.toml").is_file());
    // 二次运行：不再引导（幂等），裁决一致。
    let r2 = run_check(proj.path(), "ls");
    assert_eq!(r2.stdout.trim(), "{\"decision\":\"allow\"}");
    assert!(!r2.stderr.contains("seeded defaults"), "第二次运行不再生成");
}

#[test]
fn existing_project_layer_is_respected_no_seeding() {
    let proj = TempDir::new("respect");
    let dir = proj.path().join(".crush-tether");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("rules.toml"), "version = 1\ndefault = \"deny\"\n").unwrap();
    let r = run_check(proj.path(), "ls");
    assert_eq!(r.code, 2, "项目层 default=deny 生效：ls 落 deny exit 2");
    assert!(r.stdout.trim().is_empty());
    assert!(
        !r.stderr.contains("seeded"),
        "任一层有效配置即尊重现状，不生成；got: {}",
        r.stderr
    );
    assert!(
        !dir.join("knowledge.toml").exists(),
        "不生成缺失的默认包成员（尊重现状）"
    );
}

#[test]
fn broken_project_layer_fails_safe_without_seeding() {
    let proj = TempDir::new("broken");
    let dir = proj.path().join(".crush-tether");
    std::fs::create_dir_all(&dir).unwrap();
    let broken = "version = 1\nalow = [\"ls\"]";
    std::fs::write(dir.join("rules.toml"), broken).unwrap();
    let before = std::fs::read(dir.join("rules.toml")).unwrap();

    let r = run_check(proj.path(), "ls");
    assert_eq!(r.code, 0, "损坏 → fail-safe confirm（静默 exit 0）");
    assert!(r.stderr.contains("fail-safe confirm"), "got: {}", r.stderr);
    assert_eq!(
        std::fs::read(dir.join("rules.toml")).unwrap(),
        before,
        "损坏文件原样保留（D-03：不生成、不留档、不动原文件）"
    );
    assert!(!dir.join("knowledge.toml").exists(), "损坏不触发生成");
}
