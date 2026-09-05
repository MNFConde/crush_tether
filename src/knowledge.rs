//! 命令知识库（`knowledge.toml`，bucket 框架 main）：命令世界的通用**事实**，
//! 不产生任何裁决（D-01）。
//!
//! 语义单一事实源：`doc/design.md`「命令知识库（bucket 框架，草案）」。
//!
//! - 条目文法：一命令一表头 `[bin]`；`sub` / `flag` 是仅有的两个保留结构键
//!   （点号键打开子条目空间，值用单行 inline table）；其余键为槽位。
//! - 槽位封闭集（v1 共 10 个，槽位跟着消费机制走，D-06）：
//!   - 运行时归一组（引擎判定路径消费）：`alias_of`（命令/子命令）、
//!     `same_flag`（flag）、`takes_value`（flag）。
//!   - lint+脚本数据源组：`may_write`（命令/子命令）、`write_flags`
//!     （命令/子命令）、`write_tokens`（子命令）、`write_arg_count`（子命令）、
//!     `irreversible`（flag）。
//!   - lint 提示组：`delegates`（命令）。登记后置组：`wraps`（命令）。
//! - 归一语义：命令别名改名（`pip3` → `pip`，参数原样）；子命令别名 =
//!   bin+子命令 → 目标 bin（`npm exec foo` → `npx foo`）；flag 归一到等价类
//!   规范形（`--force` ≡ `-f`）。只做名字改写，绝不做语义变换。
//! - 加载期防环：alias / same_flag 链出现环即配置错误（`a→b→a` 拒载）。

use std::collections::{BTreeMap, HashMap, HashSet};
use std::fmt;

use serde::Deserialize;
use serde::de::{self, Deserializer, MapAccess, Visitor};

use crate::config::ConfigError;

/// 本构建支持的知识库 schema 版本。
pub const SUPPORTED_VERSION: u64 = 1;

/// 一份完整的 `knowledge.toml`。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct KnowledgeBase {
    pub version: u64,
    pub bins: BTreeMap<String, BinEntry>,
}

/// 一个命令的知识条目（`[git]`）。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BinEntry {
    /// 命令级别名（`pip3` → `pip`）。
    pub alias_of: Option<String>,
    /// 有写的可能（lint 建议；脚本数据源）。
    pub may_write: Option<bool>,
    /// 带这些 flag 才会写（lint；脚本数据源）。
    pub write_flags: Option<Vec<String>>,
    /// 委托执行项目内文件中的命令（lint 提示）。
    pub delegates: Option<String>,
    /// 包装壳（v1 仅登记）。
    pub wraps: Option<String>,
    /// 子命令条目（`sub.exec = { alias_of = "npx" }`）。
    pub subs: BTreeMap<String, SubEntry>,
    /// flag 条目（`flag."--force" = { same_flag = "-f" }`）。
    pub flags: BTreeMap<String, FlagEntry>,
}

/// 一个子命令的知识条目（`[git] sub.branch = { ... }`）。
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SubEntry {
    /// 子命令级别名：bin+子命令 → 目标 bin（`npm exec` → `npx`）。
    pub alias_of: Option<String>,
    pub may_write: Option<bool>,
    pub write_flags: Option<Vec<String>>,
    /// 这些 token 出现即写形态（默认 rules.rhai 数据源）。
    pub write_tokens: Option<Vec<String>>,
    /// 位置参数 ≥N 即写形态。
    pub write_arg_count: Option<u64>,
}

/// 一个 flag 的知识条目（`[git] flag."--hard" = { ... }`）。
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FlagEntry {
    /// 与另一 flag 等价（`--force` ≡ `-f`，目标为规范形）。
    pub same_flag: Option<String>,
    /// 该 flag 后跟一个值（引擎分解值边界）。
    pub takes_value: Option<bool>,
    /// 破坏性/不可逆参数（lint；脚本数据源）。
    pub irreversible: Option<bool>,
}

impl KnowledgeBase {
    /// 解析并做加载期校验（version、alias/same_flag 链防环）。
    pub fn parse_toml(text: &str) -> Result<Self, ConfigError> {
        let kb: Self = toml::from_str(text)?;
        if kb.version != SUPPORTED_VERSION {
            return Err(ConfigError::Semantic(format!(
                "knowledge schema version {} is not supported by this build (expected \
                 version {SUPPORTED_VERSION}); upgrade crush-tether or regenerate the file",
                kb.version
            )));
        }
        kb.validate()?;
        Ok(kb)
    }

    /// 加载期防环：命令别名链、子命令别名→命令链、same_flag 链走到不动点，
    /// 重访节点即环（`a→b→a` 报配置错误）。
    pub fn validate(&self) -> Result<(), ConfigError> {
        for bin in self.bins.keys() {
            let mut seen = vec![bin.clone()];
            let mut cur = bin.clone();
            while let Some((next, _)) = self.step(&cur, None) {
                if seen.contains(&next) {
                    return Err(ConfigError::Semantic(format!(
                        "knowledge alias cycle: {} → {}",
                        seen.join(" → "),
                        next
                    )));
                }
                cur = next.clone();
                seen.push(next);
            }
        }
        // 子命令别名会吸收 sub 槽位、命令别名保留 sub——归一状态里 sub 只减
        // 不增，任何环都必然退化为纯命令别名环，已被上面的命令链校验覆盖。
        for (bin, entry) in &self.bins {
            for flag in entry.flags.keys() {
                let mut seen = vec![flag.clone()];
                let mut cur = flag.clone();
                while let Some(next) = entry.flags.get(&cur).and_then(|f| f.same_flag.as_ref()) {
                    if seen.contains(next) {
                        return Err(ConfigError::Semantic(format!(
                            "knowledge same_flag cycle in [{bin}]: {} → {next}",
                            seen.join(" → ")
                        )));
                    }
                    cur = next.clone();
                    seen.push(next.clone());
                }
            }
        }
        Ok(())
    }

    /// 归一步进：子命令别名优先（子命令槽位被目标 bin 吸收），其次命令别名
    /// （子命令原样保留）。无别名边 → None。
    fn step(&self, bin: &str, sub: Option<&str>) -> Option<(String, Option<String>)> {
        let entry = self.bins.get(bin)?;
        if let Some(s) = sub
            && let Some(se) = entry.subs.get(s)
            && let Some(t) = &se.alias_of
        {
            return Some((t.clone(), None));
        }
        entry
            .alias_of
            .as_ref()
            .map(|t| (t.clone(), sub.map(String::from)))
    }

    /// 归一规范形映射（链式到不动点的预计算；运行时只消费等价类，D-01）。
    pub fn canon_maps(&self) -> CanonMaps {
        let mut bin = HashMap::new();
        let mut sub_alias = HashMap::new();
        let mut flag = HashMap::new();
        let mut takes_value: HashMap<String, HashSet<String>> = HashMap::new();

        for b in self.bins.keys() {
            let mut cur = b.clone();
            while let Some((next, _)) = self.step(&cur, None) {
                cur = next;
            }
            if &cur != b {
                bin.insert(b.clone(), cur);
            }
        }
        for (b, entry) in &self.bins {
            for (s, se) in &entry.subs {
                if let Some(t) = &se.alias_of {
                    sub_alias.insert((b.clone(), s.clone()), t.clone());
                }
            }
        }
        for (b, entry) in &self.bins {
            let mut fm = HashMap::new();
            let mut tv = HashSet::new();
            for f in entry.flags.keys() {
                let mut cur = f.clone();
                while let Some(next) = entry.flags.get(&cur).and_then(|x| x.same_flag.as_ref()) {
                    cur = next.clone();
                }
                if &cur != f {
                    fm.insert(f.clone(), cur.clone());
                }
                if entry.flags[f].takes_value == Some(true) {
                    tv.insert(cur);
                }
            }
            if !fm.is_empty() {
                flag.insert(b.clone(), fm);
            }
            if !tv.is_empty() {
                takes_value.insert(b.clone(), tv);
            }
        }
        CanonMaps {
            bin,
            sub_alias,
            flag,
            takes_value,
        }
    }
}

/// 归一规范形映射（空映射 = 无知识库，按字面查表）。
#[derive(Debug, Clone, Default)]
pub struct CanonMaps {
    /// bin 别名 → 规范形（链尾）。
    pub bin: HashMap<String, String>,
    /// (bin, sub) → 目标 bin（子命令槽位被吸收）。
    pub sub_alias: HashMap<(String, String), String>,
    /// bin → flag 别名 → 规范形。
    pub flag: HashMap<String, HashMap<String, String>>,
    /// bin → 取值的 flag 集合（以规范形记录）。
    pub takes_value: HashMap<String, HashSet<String>>,
}

impl CanonMaps {
    pub fn empty() -> Self {
        Self::default()
    }

    pub fn canon_bin(&self, bin: &str) -> String {
        self.bin
            .get(bin)
            .cloned()
            .unwrap_or_else(|| bin.to_string())
    }

    pub fn canon_flag(&self, bin: &str, flag: &str) -> String {
        self.flag
            .get(bin)
            .and_then(|m| m.get(flag))
            .cloned()
            .unwrap_or_else(|| flag.to_string())
    }

    /// 该 bin 的该 flag（规范形）是否取值。
    pub fn flag_takes_value(&self, bin: &str, canon_flag: &str) -> bool {
        self.takes_value
            .get(bin)
            .is_some_and(|s| s.contains(canon_flag))
    }
}

impl<'de> Deserialize<'de> for KnowledgeBase {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct KbVisitor;

        impl<'de> Visitor<'de> for KbVisitor {
            type Value = KnowledgeBase;

            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("a knowledge base: `version` plus one table per command ([bin])")
            }

            fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
            where
                A: MapAccess<'de>,
            {
                let mut version = None;
                let mut bins = BTreeMap::new();
                while let Some(key) = map.next_key::<String>()? {
                    match key.as_str() {
                        "version" => version = Some(map.next_value()?),
                        // 其余键一律是命令条目；槽位拼写错误在 BinEntry 内报错。
                        _ => {
                            let entry = map.next_value::<BinEntry>()?;
                            bins.insert(key, entry);
                        }
                    }
                }
                let version = version.ok_or_else(|| de::Error::missing_field("version"))?;
                Ok(KnowledgeBase { version, bins })
            }
        }

        deserializer.deserialize_map(KbVisitor)
    }
}

impl<'de> Deserialize<'de> for BinEntry {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct BinEntryVisitor;

        impl<'de> Visitor<'de> for BinEntryVisitor {
            type Value = BinEntry;

            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(
                    "a knowledge entry: alias_of/may_write/write_flags/delegates/wraps \
                     slots plus reserved sub/flag maps",
                )
            }

            fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
            where
                A: MapAccess<'de>,
            {
                const SLOTS: &[&str] = &[
                    "alias_of",
                    "may_write",
                    "write_flags",
                    "delegates",
                    "wraps",
                    "sub",
                    "flag",
                ];
                let mut out = BinEntry::default();
                while let Some(key) = map.next_key::<String>()? {
                    match key.as_str() {
                        "alias_of" => out.alias_of = Some(map.next_value()?),
                        "may_write" => out.may_write = Some(map.next_value()?),
                        "write_flags" => out.write_flags = Some(map.next_value()?),
                        "delegates" => out.delegates = Some(map.next_value()?),
                        "wraps" => out.wraps = Some(map.next_value()?),
                        "sub" => out.subs = map.next_value()?,
                        "flag" => out.flags = map.next_value()?,
                        _ => return Err(de::Error::unknown_field(&key, SLOTS)),
                    }
                }
                Ok(out)
            }
        }

        deserializer.deserialize_map(BinEntryVisitor)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn design_example_slots_parse() {
        let src = concat!(
            "version = 1\n",
            "[npx]\n",
            "may_write = true\n",
            "[npm]\n",
            "sub.exec = { alias_of = \"npx\" }\n",
            "sub.x = { alias_of = \"npx\" }\n",
            "[pip3]\n",
            "alias_of = \"pip\"\n",
            "[curl]\n",
            "may_write = true\n",
            "write_flags = [\"-o\", \"--output\"]\n",
            "[git]\n",
            "sub.branch = { write_tokens = [\"-d\", \"-D\", \"-m\", \"-M\", \"--delete\", \"--move\", \"--create\"] }\n",
            "sub.config = { write_arg_count = 2 }\n",
            "flag.\"--force\" = { same_flag = \"-f\" }\n",
            "flag.\"--hard\" = { irreversible = true }\n",
            "[make]\n",
            "delegates = \"Makefile\"\n",
            "[sudo]\n",
            "wraps = \"*\"\n",
        );
        let kb = KnowledgeBase::parse_toml(src).expect("parses");
        assert_eq!(kb.bins.len(), 7);

        let npm = &kb.bins["npm"];
        assert_eq!(npm.subs["exec"].alias_of.as_deref(), Some("npx"));
        assert_eq!(npm.subs["x"].alias_of.as_deref(), Some("npx"));
        assert_eq!(kb.bins["pip3"].alias_of.as_deref(), Some("pip"));
        assert_eq!(kb.bins["npx"].may_write, Some(true));
        assert_eq!(
            kb.bins["curl"].write_flags,
            Some(vec!["-o".to_string(), "--output".to_string()])
        );
        let git = &kb.bins["git"];
        assert!(
            git.subs["branch"]
                .write_tokens
                .as_deref()
                .is_some_and(|t| t.contains(&"-d".to_string()))
        );
        assert_eq!(git.subs["config"].write_arg_count, Some(2));
        assert_eq!(git.flags["--force"].same_flag.as_deref(), Some("-f"));
        assert_eq!(git.flags["--hard"].irreversible, Some(true));
        assert_eq!(kb.bins["make"].delegates.as_deref(), Some("Makefile"));
        assert_eq!(kb.bins["sudo"].wraps.as_deref(), Some("*"));
        // takes_value + 10 槽位全覆盖检查
        let full = KnowledgeBase::parse_toml(&format!(
            "{src}[xx]\nflag.\"-o\" = {{ takes_value = true }}\n"
        ))
        .expect("parses");
        assert_eq!(full.bins["xx"].flags["-o"].takes_value, Some(true));
    }

    #[test]
    fn missing_version_and_unknown_slot_are_errors() {
        let msg = KnowledgeBase::parse_toml("[pip3]\nalias_of = \"pip\"")
            .expect_err("no version")
            .to_string();
        assert!(msg.contains("missing field `version`"), "{msg}");

        let msg = KnowledgeBase::parse_toml("version = 1\n[git]\nwrite_tokenz = [\"-d\"]")
            .expect_err("typo slot")
            .to_string();
        assert!(msg.contains("unknown field `write_tokenz`"), "{msg}");

        let msg = KnowledgeBase::parse_toml("version = 2")
            .expect_err("future version")
            .to_string();
        assert!(msg.contains("version 2 is not supported"), "{msg}");
    }

    #[test]
    fn alias_cycles_are_rejected_at_load() {
        let msg =
            KnowledgeBase::parse_toml("version = 1\n[a]\nalias_of = \"b\"\n[b]\nalias_of = \"a\"")
                .expect_err("command cycle")
                .to_string();
        assert!(msg.contains("alias cycle"), "{msg}");

        // 子命令别名会吸收 sub 槽位（见 validate 内注释）：不存在独立的
        // 「sub 环」，等价绕回形态（sub → 别 bin → 命令别名绕回）会正常
        // 终止到不动点，不属于环。
        let msg = KnowledgeBase::parse_toml(
            "version = 1\n[g]\nflag.\"--x\" = { same_flag = \"--y\" }\nflag.\"--y\" = { same_flag = \"--x\" }",
        )
        .expect_err("same_flag cycle")
        .to_string();
        assert!(msg.contains("same_flag cycle"), "{msg}");
    }

    #[test]
    fn chain_aliases_resolve_to_fixed_point() {
        let kb =
            KnowledgeBase::parse_toml("version = 1\n[a]\nalias_of = \"b\"\n[b]\nalias_of = \"c\"")
                .expect("parses");
        let maps = kb.canon_maps();
        assert_eq!(maps.canon_bin("a"), "c");
        assert_eq!(maps.canon_bin("b"), "c");
        assert_eq!(maps.canon_bin("c"), "c", "规范形自身不动");
    }

    #[test]
    fn canon_maps_cover_sub_alias_flag_and_takes_value() {
        let kb = KnowledgeBase::parse_toml(concat!(
            "version = 1\n",
            "[npm]\n",
            "sub.exec = { alias_of = \"npx\" }\n",
            "[git]\n",
            "flag.\"--force\" = { same_flag = \"-f\" }\n",
            "flag.\"--output\" = { takes_value = true }\n",
        ))
        .expect("parses");
        let maps = kb.canon_maps();
        assert_eq!(
            maps.sub_alias
                .get(&("npm".into(), "exec".into()))
                .map(String::as_str),
            Some("npx")
        );
        assert_eq!(maps.canon_flag("git", "--force"), "-f");
        assert!(maps.flag_takes_value("git", "--output"));
        assert!(!maps.flag_takes_value("git", "-f"));
        assert!(!maps.flag_takes_value("other", "anything"));
    }

    #[test]
    fn empty_knowledge_base_is_valid() {
        let kb = KnowledgeBase::parse_toml("version = 1").expect("parses");
        assert!(kb.bins.is_empty());
        assert!(kb.validate().is_ok());
    }
}
