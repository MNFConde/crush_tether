//! M3.3 回归驱动：用默认包模板装配「查表 + 脚本 + 定稿点」分类器，与
//! 二进制管线（engine::decide_with + script::finalize）完全一致。

use std::path::{Path, PathBuf};
use std::sync::Arc;

use crush_tether::config::merge;
use crush_tether::config::seed::{
    DEFAULT_KNOWLEDGE_TOML, DEFAULT_RULES_LUA, DEFAULT_RULES_RHAI, DEFAULT_RULES_TOML,
};
use crush_tether::config::{Layers, RulesFile};
use crush_tether::knowledge::KnowledgeBase;
use crush_tether::lookup::RuleLookup;
use crush_tether::model::{Decision, Verdict};
use crush_tether::script::{LuaEngine, RhaiEngine, RuleEngine};

/// 仓库根：词法判断基准。取真实 manifest 目录（CARGO_MANIFEST_DIR）——
/// 两平台皆为真绝对路径；此前硬编码 `D:/...` 在 Linux 是相对路径，会把
/// `inside_repo` 的相对分支带进双前置误判（CI linux 逃逸用例红灯根因，
/// 2026-09-13 修正）。
pub const PROJECT: &str = env!("CARGO_MANIFEST_DIR");

/// 用默认包模板跑一次完整管线，返回组合裁决档位（脚本层 = Rhai 默认模板）。
pub fn decide(cmd: &str) -> Decision {
    let kb = Arc::new(KnowledgeBase::parse_toml(DEFAULT_KNOWLEDGE_TOML).expect("default kb parse"));
    let script = RhaiEngine::compile(
        DEFAULT_RULES_RHAI,
        PathBuf::from(PROJECT),
        Some(kb.clone()),
        script_allow_for(&kb),
    )
    .expect("default rules.rhai compiles");
    decide_with_engine(cmd, &script)
}

/// 同 [`decide`]，脚本层换 Lua 模板（默认包双模板等价性的验收载体）。
#[allow(dead_code)] // 仅部分测试目标消费（guard_regression 等不触及 lua 路径）
pub fn decide_lua(cmd: &str) -> Decision {
    let kb = Arc::new(KnowledgeBase::parse_toml(DEFAULT_KNOWLEDGE_TOML).expect("default kb parse"));
    let script = LuaEngine::compile(
        DEFAULT_RULES_LUA,
        PathBuf::from(PROJECT),
        Some(kb.clone()),
        script_allow_for(&kb),
    )
    .expect("default rules.lua compiles");
    decide_with_engine(cmd, &script)
}

fn script_allow_for(kb: &Arc<KnowledgeBase>) -> crush_tether::config::merge::ScriptAllowDecls {
    let rules = RulesFile::parse_toml(DEFAULT_RULES_TOML).expect("default rules parse");
    let lookup = RuleLookup::new(
        merge(Layers {
            global: None,
            user: None,
            project: Some(&rules),
        }),
        Some(kb),
    );
    lookup.script_allow().clone()
}

fn decide_with_engine(cmd: &str, script: &dyn RuleEngine) -> Decision {
    let rules = RulesFile::parse_toml(DEFAULT_RULES_TOML).expect("default rules parse");
    let kb = Arc::new(KnowledgeBase::parse_toml(DEFAULT_KNOWLEDGE_TOML).expect("default kb parse"));
    let lookup = RuleLookup::new(
        merge(Layers {
            global: None,
            user: None,
            project: Some(&rules),
        }),
        Some(&kb),
    );

    let verdict = crush_tether::engine::decide_with(cmd, Path::new(PROJECT), &|c, b, p, pipe| {
        let v0 = lookup.classify(c, b, p);
        let escape = |cc: &crush_tether::cmd_parse::SimpleCommand, bb: Option<&Path>, pp: &Path| {
            lookup.write_target_escapes_with_base(cc, bb, pp)
        };
        let (decision, reason) = match script.evaluate(c, v0.decision, p, pipe) {
            // 与 main.rs 相同：定稿点唯一放行出口。
            Ok(outcome) => crush_tether::script::finalize(
                v0.decision,
                outcome,
                script.decls(),
                c,
                b,
                p,
                &escape,
            ),
            Err(_) => (
                Decision::Confirm,
                Some("script evaluation failed; fail-safe".into()),
            ),
        };
        Verdict {
            decision,
            reason: reason.or(v0.reason),
        }
    });
    verdict.decision
}
