//! 验收（M2.1）：`doc/design.md`「rules.toml 结构（草案）」示例整体可解析。
//!
//! 示例块从 design.md 现场提取——文档是单一事实源，解析器与其漂移时本测试
//! 即失败，修复方向以文档为准（实现期就地修订文档须同步改本测试的断言）。

use std::path::Path;

use crush_tether::config::{ListField, RulesFile};
use crush_tether::model::Decision;

const DESIGN_MD: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/doc/design.md");

/// 提取「`rules.toml` 结构（草案）」小节的第一个 ```toml 代码块。
fn extract_rules_example(md: &str) -> String {
    let section = md
        .split("### `rules.toml` 结构")
        .nth(1)
        .expect("design.md contains the rules.toml structure section");
    let start = section.find("```toml").expect("section opens a toml block") + "```toml".len();
    let rest = &section[start..];
    let end = rest.find("```").expect("toml block is closed");
    rest[..end].to_string()
}

/// 提取「命令知识库（bucket 框架，定稿）」小节的第一个 ```toml 代码块。
fn extract_knowledge_example(md: &str) -> String {
    let section = md
        .split("### 命令知识库（bucket 框架，定稿）")
        .nth(1)
        .expect("design.md contains the knowledge base section");
    let start = section.find("```toml").expect("section opens a toml block") + "```toml".len();
    let rest = &section[start..];
    let end = rest.find("```").expect("toml block is closed");
    rest[..end].to_string()
}

fn sub_list(bucket: Option<&crush_tether::config::BucketSpec>, dim: SubFlag) -> &[String] {
    let b = bucket.unwrap_or_else(|| panic!("bucket missing"));
    let field = match dim {
        SubFlag::Sub => &b.sub,
        SubFlag::Flag => &b.flag,
    };
    match field
        .as_ref()
        .unwrap_or_else(|| panic!("dimension missing"))
    {
        ListField::Set(v) => v,
        ListField::Delta { .. } => panic!("example uses plain arrays here"),
    }
}

enum SubFlag {
    Sub,
    Flag,
}

#[test]
fn design_md_rules_example_parses() {
    let md = std::fs::read_to_string(Path::new(DESIGN_MD)).expect("read design.md");
    let src = extract_rules_example(&md);
    let f = RulesFile::parse_toml(&src)
        .unwrap_or_else(|e| panic!("design.md example must parse: {e}\n{src}"));

    // 裸键区
    assert_eq!(f.version, 1);
    assert_eq!(f.default, Some(Decision::Confirm));
    assert_eq!(
        f.precedence,
        Some(vec![Decision::Deny, Decision::Confirm, Decision::Allow])
    );

    // [local] 头部裸列表
    assert_eq!(
        f.local.buckets.confirm.as_ref(),
        Some(&ListField::Set(
            ["rm", "pip", "pip3", "npx", "curl", "wget"]
                .iter()
                .map(|s| s.to_string())
                .collect()
        ))
    );
    // [local] deny 裸列表（2026-09-06 补系统级破坏/提权四族，更正登记 13）
    assert_eq!(
        f.local.buckets.deny.as_ref(),
        Some(&ListField::Set(
            [
                "sudo",
                "dd",
                "shutdown",
                "mkfs",
                "mkfs.ext2",
                "mkfs.ext3",
                "mkfs.ext4",
                "mkfs.vfat",
                "mkfs.fat",
                "mkfs.xfs",
                "mkfs.btrfs",
                "mkfs.ntfs"
            ]
            .iter()
            .map(|s| s.to_string())
            .collect()
        ))
    );
    match f.local.buckets.allow.as_ref().expect("[local].allow") {
        ListField::Set(v) => {
            assert!(v.contains(&"ls".to_string()) && v.contains(&"touch".to_string()));
        }
        ListField::Delta { .. } => panic!("head allow is a plain array"),
    }

    // [local.git]
    let git = f.local.commands.get("git").expect("git section");
    assert!(sub_list(git.allow.as_ref(), SubFlag::Sub).contains(&"status".to_string()));
    assert!(sub_list(git.confirm.as_ref(), SubFlag::Sub).contains(&"reset".to_string()));
    assert!(sub_list(git.deny.as_ref(), SubFlag::Sub).contains(&"push".to_string()));
    assert!(sub_list(git.confirm.as_ref(), SubFlag::Flag).contains(&"--output".to_string()));
    assert!(sub_list(git.confirm.as_ref(), SubFlag::Flag).contains(&"-h".to_string()));
    assert_eq!(
        sub_list(git.deny.as_ref(), SubFlag::Flag),
        &["--hard".to_string()]
    );

    // 节内 default：npm/pnpm 放行其余子命令
    for bin in ["npm", "pnpm"] {
        let sec = f
            .local
            .commands
            .get(bin)
            .unwrap_or_else(|| panic!("{bin} section"));
        assert_eq!(sec.default, Some(Decision::Allow), "{bin} default");
    }

    // cargo/go 收窄为命令节 allow.sub
    let cargo = f.local.commands.get("cargo").expect("cargo section");
    assert!(sub_list(cargo.allow.as_ref(), SubFlag::Sub).contains(&"clippy".to_string()));

    // [global] 默认只有空 allow
    assert_eq!(
        f.global.buckets.allow,
        Some(ListField::Set(Vec::new())),
        "global allow defaults to empty"
    );
    assert!(
        f.global.commands.is_empty(),
        "global has no command sections"
    );
}

#[test]
fn design_md_knowledge_example_parses() {
    use crush_tether::knowledge::KnowledgeBase;

    let md = std::fs::read_to_string(Path::new(DESIGN_MD)).expect("read design.md");
    let src = extract_knowledge_example(&md);
    let kb = KnowledgeBase::parse_toml(&src)
        .unwrap_or_else(|e| panic!("design.md knowledge example must parse: {e}\n{src}"));

    // 10 槽位封闭集在示例中的体现（D-06：槽位跟着消费机制走）。
    assert_eq!(kb.version, 1);
    assert_eq!(kb.bins["npx"].may_write, Some(true));
    assert_eq!(kb.bins["npm"].subs["exec"].alias_of.as_deref(), Some("npx"));
    assert_eq!(kb.bins["pip3"].alias_of.as_deref(), Some("pip"));
    assert_eq!(
        kb.bins["curl"].write_flags,
        Some(vec!["-o".to_string(), "--output".to_string()])
    );
    let git = &kb.bins["git"];
    assert!(git.subs["branch"].write_tokens.is_some());
    // remote/tag 写形态数据（2026-09-06 补齐，更正登记 13）
    assert_eq!(
        git.subs["remote"].write_tokens.as_deref().unwrap_or(&[]),
        [
            "add",
            "set-url",
            "remove",
            "rename",
            "set-head",
            "set-branches"
        ]
    );
    assert_eq!(
        git.subs["tag"].write_tokens.as_deref().unwrap_or(&[]),
        ["-d", "--delete", "-a", "-s", "-m", "-f", "-u", "--annotate"]
    );
    assert_eq!(git.subs["config"].write_arg_count, Some(2));
    assert_eq!(git.flags["--force"].same_flag.as_deref(), Some("-f"));
    assert_eq!(git.flags["--hard"].irreversible, Some(true));
    assert_eq!(kb.bins["make"].delegates.as_deref(), Some("Makefile"));
    assert_eq!(kb.bins["sudo"].wraps.as_deref(), Some("*"));
}
