//! 默认配置生成（v1，项目层）：三层皆缺才生成（design.md「零内置策略与
//! 默认配置生成（定稿）」）。
//!
//! - 模板内嵌于二进制只是**生成源数据**，不参与判定，不构成内置策略；
//!   toml 内容与 `doc/design.md` 两个示例块逐字节一致（tests/seed_defaults.rs
//!   的模板=文档测试把模板钉在文档上）。
//! - 「损坏 ≠ 缺失」（D-03）：任一层存在但解析失败 → 告警 + confirm 兜底、
//!   原文件不动、**不生成**（触发判断在调用方：仅发现层 Ok 且三层皆缺时
//!   才进入本模块）。
//! - 幂等 + 原子：模板内容恒定，temp + rename 原子替换；多 hook 并发发现
//!   缺失时各自写临时文件后 rename，同一内容天然收敛到同一结果。
//! - 脚本模板按引擎选择（M6.1）：rhai → `rules.rhai`，lua → `rules.lua`，
//!   两者承载同一套四类谓词（语义等价，钉死测试双跑对账）。

use std::io::Write;
use std::path::Path;

/// 默认 `rules.toml` 模板（= design.md「rules.toml 结构」示例块）。
pub const DEFAULT_RULES_TOML: &str = include_str!("templates/default-rules.toml");
/// 默认 `knowledge.toml` 模板（= design.md「命令知识库」示例块）。
pub const DEFAULT_KNOWLEDGE_TOML: &str = include_str!("templates/default-knowledge.toml");
/// 默认 `rules.rhai` 模板（四类谓词；allow 契约见文件头，M3.2 定稿）。
pub const DEFAULT_RULES_RHAI: &str = include_str!("templates/default-rules.rhai");
/// 默认 `rules.lua` 模板（与 rhai 版同一四类谓词，M6.1 语义等价）。
pub const DEFAULT_RULES_LUA: &str = include_str!("templates/default-rules.lua");

/// 默认包文件清单（文件名 → 内容）：toml 两件 + 按引擎选择的脚本模板
/// （缺省 rhai；`--engine lua` 时生成 lua 版）。
pub fn default_files(engine: &str) -> Vec<(&'static str, &'static str)> {
    let (script, src) = match crate::script::script_file_name(engine) {
        Some("rules.lua") => ("rules.lua", DEFAULT_RULES_LUA),
        _ => ("rules.rhai", DEFAULT_RULES_RHAI),
    };
    vec![
        ("rules.toml", DEFAULT_RULES_TOML),
        ("knowledge.toml", DEFAULT_KNOWLEDGE_TOML),
        (script, src),
    ]
}

/// 在 `<project_root>/.crush-tether/` 生成默认包；已存在的文件一律不动
/// （尊重现状）。返回本次实际新写入的文件数。
///
/// 并发安全：临时文件名带进程 id + 纳秒时间戳，`rename` 原子替换；并发
/// 写入的同一内容互相覆盖后字节一致（收敛）。
pub fn seed_defaults_if_absent(project_root: &Path, engine: &str) -> std::io::Result<usize> {
    let dir = project_root.join(".crush-tether");
    std::fs::create_dir_all(&dir)?;
    let mut written = 0;
    for (name, content) in default_files(engine) {
        let dest = dir.join(name);
        if dest.exists() {
            continue;
        }
        atomic_write(&dest, content)?;
        written += 1;
    }
    Ok(written)
}

/// temp + rename 原子写（同目录保证同文件系统，rename 即原子发布）。
fn atomic_write(dest: &Path, content: &str) -> std::io::Result<()> {
    let dir = dest.parent().unwrap_or_else(|| Path::new("."));
    let file_name = dest.file_name().and_then(|n| n.to_str()).unwrap_or("file");
    let tmp = dir.join(format!(
        ".{file_name}.tmp-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    {
        let mut f = std::fs::File::create(&tmp)?;
        f.write_all(content.as_bytes())?;
        f.sync_all()?;
    }
    match std::fs::rename(&tmp, dest) {
        Ok(()) => Ok(()),
        Err(e) => {
            let _ = std::fs::remove_file(&tmp);
            Err(e)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::config::RulesFile;
    use crate::knowledge::KnowledgeBase;
    use crate::model::Decision;
    use crate::script::RuleEngine;
    use std::path::PathBuf;

    use crate::testutil::TempDir;

    #[test]
    fn templates_parse_with_own_loader() {
        let rules = RulesFile::parse_toml(DEFAULT_RULES_TOML).expect("default rules parse");
        assert_eq!(rules.version, 1);
        assert_eq!(rules.default, Some(Decision::Confirm));
        let kb = KnowledgeBase::parse_toml(DEFAULT_KNOWLEDGE_TOML).expect("default kb parse");
        assert_eq!(kb.version, 1);
        assert!(kb.bins.contains_key("npm"));
    }

    #[test]
    fn seeds_all_pack_files_then_is_idempotent() {
        let proj = TempDir::new("m26", "seed");
        assert_eq!(seed_defaults_if_absent(proj.path(), "rhai").unwrap(), 3);
        let rules_path = proj.path().join(".crush-tether").join("rules.toml");
        let before = std::fs::read_to_string(&rules_path).unwrap();
        assert_eq!(before, DEFAULT_RULES_TOML, "重复生成字节一致");
        // 已存在 → 不再写（written=0，内容不变）。
        assert_eq!(seed_defaults_if_absent(proj.path(), "rhai").unwrap(), 0);
        assert_eq!(std::fs::read_to_string(&rules_path).unwrap(), before);
    }

    #[test]
    fn default_rhai_compiles_and_carries_predicates() {
        // 默认脚本必须能编译，且四类谓词在沙箱内行为正确（find 突变 / 管道
        // deny / 写重定向升级；两态数据读知识库的完整链路在 e2e 覆盖）。
        let e = crate::script::RhaiEngine::compile(
            DEFAULT_RULES_RHAI,
            std::path::PathBuf::from("D:/code/tmp/proj"),
            None,
            Default::default(),
        )
        .expect("default rules.rhai compiles");
        let c = |s: &str| {
            crate::cmd_parse::flatten_commands(s)
                .unwrap()
                .into_iter()
                .next()
                .unwrap()
        };
        let p = std::path::Path::new("D:/code/tmp/proj");
        // 2) find 突变
        assert_eq!(
            e.evaluate(&c("find . -delete"), Decision::Allow, p, false)
                .unwrap(),
            crate::script::ScriptOutcome::Adjust(Decision::Confirm)
        );
        // 3) 管道 sink → deny
        assert_eq!(
            e.evaluate(&c("sh"), Decision::Allow, p, true).unwrap(),
            crate::script::ScriptOutcome::Adjust(Decision::Deny)
        );
        // 4) 写特征升级：allow + 写重定向 → confirm；无写特征不升级
        assert_eq!(
            e.evaluate(&c("ls"), Decision::Allow, p, false).unwrap(),
            crate::script::ScriptOutcome::Pass
        );
        assert_eq!(
            e.evaluate(&c("ls > out.txt"), Decision::Allow, p, false)
                .unwrap(),
            crate::script::ScriptOutcome::Adjust(Decision::Confirm)
        );
    }

    #[test]
    fn default_lua_matches_rhai_predicate_semantics() {
        // M6.1 验收「默认规则 lua 版行为等价」：两引擎默认模板对同一命令集
        // 产出同一脚本层裁决（谓词 2/3/4 + nil=PASS 词汇等价）。
        use crate::script::{LuaEngine, RhaiEngine, RuleEngine, ScriptOutcome};
        let c = |s: &str| {
            crate::cmd_parse::flatten_commands(s)
                .unwrap()
                .into_iter()
                .next()
                .unwrap()
        };
        let p = std::path::Path::new("D:/code/tmp/proj");
        let rhai = RhaiEngine::compile(
            DEFAULT_RULES_RHAI,
            PathBuf::from("D:/code/tmp/proj"),
            None,
            Default::default(),
        )
        .expect("rhai template compiles");
        let lua = LuaEngine::compile(
            DEFAULT_RULES_LUA,
            PathBuf::from("D:/code/tmp/proj"),
            None,
            Default::default(),
        )
        .expect("lua template compiles");
        // (命令行, 查表基线, pipe_to_shell, 期望脚本层产出)
        let cases: &[(&str, Decision, bool, ScriptOutcome)] = &[
            (
                "find . -delete",
                Decision::Allow,
                false,
                ScriptOutcome::Adjust(Decision::Confirm),
            ),
            (
                "find . -exec rm {} ;",
                Decision::Allow,
                false,
                ScriptOutcome::Adjust(Decision::Confirm),
            ),
            ("curl x", Decision::Confirm, false, ScriptOutcome::Pass),
            (
                "curl x | sh",
                Decision::Confirm,
                true,
                ScriptOutcome::Adjust(Decision::Deny),
            ),
            ("ls", Decision::Allow, false, ScriptOutcome::Pass),
            (
                "ls > out.txt",
                Decision::Allow,
                false,
                ScriptOutcome::Adjust(Decision::Confirm),
            ),
            // kb 缺失：两态判定无法进行 → 有子命令的 allow confirm 兜底。
            (
                "git status",
                Decision::Allow,
                false,
                ScriptOutcome::Adjust(Decision::Confirm),
            ),
        ];
        for (line, verdict, pipe, want) in cases {
            let cmd = c(line);
            assert_eq!(
                rhai.evaluate(&cmd, *verdict, p, *pipe).unwrap(),
                *want,
                "rhai {line}"
            );
            assert_eq!(
                lua.evaluate(&cmd, *verdict, p, *pipe).unwrap(),
                *want,
                "lua {line}"
            );
        }
        // nil = PASS 词汇等价的直测：空脚本返回 nil。
        let nil_lua = LuaEngine::compile(
            "function check(ctx) return nil end",
            PathBuf::from("D:/code/tmp/proj"),
            None,
            Default::default(),
        )
        .expect("nil script compiles");
        assert_eq!(
            nil_lua
                .evaluate(&c("ls"), Decision::Allow, p, false)
                .unwrap(),
            ScriptOutcome::Pass
        );
    }

    #[test]
    fn existing_files_are_never_touched() {
        let proj = TempDir::new("m26", "respect");
        let dir = proj.path().join(".crush-tether");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("rules.toml"),
            "version = 1\n[local]\nallow = [\"ls\"]",
        )
        .unwrap();
        assert_eq!(
            seed_defaults_if_absent(proj.path(), "rhai").unwrap(),
            2,
            "只补缺的 knowledge.toml 与 rules.rhai"
        );
        assert_eq!(
            std::fs::read_to_string(dir.join("rules.toml")).unwrap(),
            "version = 1\n[local]\nallow = [\"ls\"]",
            "已存在文件原样保留"
        );
    }

    #[test]
    fn concurrent_seeding_converges_to_same_bytes() {
        let proj = TempDir::new("m26", "race");
        let root: PathBuf = proj.path().to_path_buf();
        let handles: Vec<_> = (0..8)
            .map(|_| {
                let root = root.clone();
                std::thread::spawn(move || seed_defaults_if_absent(&root, "rhai"))
            })
            .collect();
        for h in handles {
            h.join().expect("thread ok").expect("seed ok");
        }
        let rules = std::fs::read_to_string(root.join(".crush-tether").join("rules.toml")).unwrap();
        let kb =
            std::fs::read_to_string(root.join(".crush-tether").join("knowledge.toml")).unwrap();
        assert_eq!(rules, DEFAULT_RULES_TOML);
        assert_eq!(kb, DEFAULT_KNOWLEDGE_TOML);
    }
}
