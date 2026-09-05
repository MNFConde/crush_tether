//! M3.3 回归驱动：用默认包模板装配「查表 + 脚本」分类器，与二进制
//! 管线（engine::decide_with）完全一致。

use std::path::{Path, PathBuf};
use std::sync::Arc;

use crush_tether::config::merge;
use crush_tether::config::seed::{DEFAULT_KNOWLEDGE_TOML, DEFAULT_RULES_RHAI, DEFAULT_RULES_TOML};
use crush_tether::config::{Layers, RulesFile};
use crush_tether::knowledge::KnowledgeBase;
use crush_tether::lookup::RuleLookup;
use crush_tether::model::{Decision, Verdict};
use crush_tether::script::{RhaiEngine, RuleEngine};

/// 仓库根：词法判断基准（不要求目录存在）。
pub const PROJECT: &str = "D:/Code/RustCodeProject/mdor";

/// 用默认包模板跑一次完整管线，返回组合裁决档位。
pub fn decide(cmd: &str) -> Decision {
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
    let script = RhaiEngine::compile(DEFAULT_RULES_RHAI, PathBuf::from(PROJECT), Some(kb.clone()))
        .expect("default rules.rhai compiles");

    let verdict = crush_tether::engine::decide_with(cmd, Path::new(PROJECT), &|c, p, pipe| {
        let mut v = lookup.classify(c, p);
        match script.evaluate(c, v.decision, p, pipe) {
            Ok(Some(d)) if d != v.decision => {
                v = Verdict {
                    decision: d,
                    reason: Some("default rules.rhai".into()),
                };
            }
            Err(_) => v = Verdict::confirm("script evaluation failed; fail-safe"),
            _ => {}
        }
        v
    });
    verdict.decision
}
