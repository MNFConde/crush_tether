//! M9.3 验收：命令级可恢复性槽位——irreversible_gate 升 deny、显式收录
//! confirm 桶的标注 bin 同样升 deny、lint 两告警、knowledge 解析与未知键拒载。

mod common;
mod fixture;

use common::{TempDir, run_check};
use crush_tether::config::{Layers, RulesFile, merge};
use crush_tether::knowledge::KnowledgeBase;
use crush_tether::lint::lint_file;
use crush_tether::lookup::RuleLookup;
use crush_tether::model::Decision;
use fixture::{decide, decide_lua};

#[test]
fn unindexed_irreversible_bin_upgrades_confirm_to_deny() {
    // parted/wipefs 未入桶 → 查表 default confirm → gate 升 deny。
    assert_eq!(decide("parted /dev/sda"), Decision::Deny);
    assert_eq!(decide("wipefs /dev/sdb"), Decision::Deny);
    assert_eq!(decide_lua("parted /dev/sda"), Decision::Deny, "双引擎等价");
}

#[test]
fn non_marked_bins_keep_confirm() {
    // 未标注的未知命令仍落 confirm；curl 维持 confirm（语义不变）。
    assert_eq!(decide("mysterycmd x"), Decision::Confirm);
    assert_eq!(decide("curl x"), Decision::Confirm);
}

#[test]
fn indexed_confirm_marked_irreversible_also_denies() {
    // 已显式收录 confirm 桶的 bin 被 kb 标注 → 同样升 deny（策略一致）。
    let proj = TempDir::new("gate-indexed");
    let cfg = proj.path().join(".crush-tether");
    std::fs::create_dir_all(&cfg).expect("create cfg");
    std::fs::write(
        cfg.join("rules.toml"),
        "version = 1\ndefault = \"confirm\"\n[local]\nconfirm = [\"parted\"]\n",
    )
    .expect("write rules");
    std::fs::write(
        cfg.join("rules.rhai"),
        concat!(
            "rule(\"irreversible_gate\", 15, |ctx| {\n",
            "    if ctx.verdict == decision::CONFIRM && kb_bin_irreversible(ctx.bin) {\n",
            "        return decision::DENY;\n",
            "    }\n",
            "    decision::PASS\n",
            "});"
        ),
    )
    .expect("write script");
    std::fs::write(
        cfg.join("knowledge.toml"),
        "version = 1\n[parted]\nirreversible = true\n",
    )
    .expect("write kb");
    let r = run_check(proj.path(), "parted /dev/sda");
    assert_eq!(r.code, 2, "显式 confirm 桶 + kb 标注 → gate 升 deny");
}

#[test]
fn allow_bucket_verdict_not_touched_by_gate() {
    // gate 只作用于 confirm 基线：allow 命中不受影响（局部配置自定策略）。
    // 注意：这正是 lint allow-irreversible 告警要提示用户收紧的形态。
    let rules = RulesFile::parse_toml(concat!(
        "version = 1\n",
        "default = \"confirm\"\n",
        "[local]\n",
        "allow = [\"shred\"]\n",
    ))
    .expect("rules parse");
    let kb = KnowledgeBase::parse_toml("version = 1\n[shred]\nirreversible = true\n").unwrap();
    let lookup = RuleLookup::new(
        merge(Layers {
            global: None,
            user: None,
            project: Some(&rules),
        }),
        Some(&kb),
    );
    let cmd = crush_tether::cmd_parse::flatten_commands("shred x")
        .expect("parses")
        .remove(0);
    let proj = std::path::Path::new("D:/code/tmp/recoverability-proj");
    assert_eq!(
        lookup.classify(&cmd, Some(proj), proj).decision,
        Decision::Allow
    );
}

#[test]
fn lint_warns_on_allow_and_script_allow_of_irreversible_bin() {
    let rules = RulesFile::parse_toml(concat!(
        "version = 1\n",
        "default = \"confirm\"\n",
        "[local]\n",
        "allow = [\"parted\"]\n",
        "script_allow = [\"wipefs\"]\n",
    ))
    .expect("rules parse");
    let kb = KnowledgeBase::parse_toml(
        "version = 1\n[parted]\nirreversible = true\n[wipefs]\nirreversible = true\n",
    )
    .expect("kb parse");
    let lints = lint_file(&rules, Some(&kb), &["wipefs".to_string()]);
    let codes: Vec<&str> = lints.iter().map(|l| l.code).collect();
    assert!(codes.contains(&"allow-irreversible"), "{codes:?}");
    assert!(codes.contains(&"script-allow-irreversible"), "{codes:?}");
}

#[test]
fn bin_level_irreversible_parses_and_unknown_keys_rejected() {
    let kb = KnowledgeBase::parse_toml("version = 1\n[foo]\nirreversible = true\n").unwrap();
    assert_eq!(kb.bins["foo"].irreversible, Some(true));
    // 槽位拼写错误拒载（封闭集）。
    assert!(
        KnowledgeBase::parse_toml("version = 1\n[foo]\nirreversable = true\n").is_err(),
        "未知键拒载"
    );
}
