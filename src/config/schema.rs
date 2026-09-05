//! `rules.toml` 数据模型（草案 v1）与解析。
//!
//! 语义单一事实源：`doc/design.md`「配置格式与脚本边界（草案 v1）」。结构要点：
//!
//! - 裸键区：`version`（schema 版本，必填）/ `default`（兜底档）/ `precedence`
//!   （桶间优先级，须为三桶的排列）。
//! - `[local]` / `[global]` 作用域双表：头部裸列表桶（整命令入桶的语法糖）+
//!   任意命令节（`[local.git]`）。
//! - 命令节：`allow` / `confirm` / `deny` 三桶，各含 `sub`（子命令）/ `flag`
//!   两个子键，另有节内 `default` 覆盖顶层兜底。
//! - 列表值双形态（D-02）：数组 = 覆盖定义；`{ add = [...], remove = [...] }`
//!   = 继承低层并增删。
//!
//! 解析严格性：未知键一律报错（toml 消息自带行列定位）——配置拼写错误静默
//! 失效等于策略静默漂移，故不做「忽略未知键」的宽容解析。

use std::collections::BTreeMap;
use std::fmt;

use serde::Deserialize;
use serde::de::{self, Deserializer, MapAccess, SeqAccess, Visitor};

use crate::model::Decision;

/// 本构建支持的配置 schema 版本（`version` 裸键）。
pub const SUPPORTED_VERSION: u64 = 1;

/// 配置解析/校验错误。
#[derive(Debug)]
pub enum ConfigError {
    /// TOML 语法或结构错误（Display 消息自带行列与源行片段，可直接定位）。
    Parse(toml::de::Error),
    /// 结构合法但语义非法：version 不支持、`precedence` 非三桶排列等。
    Semantic(String),
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ConfigError::Parse(e) => write!(f, "{e}"),
            ConfigError::Semantic(msg) => f.write_str(msg),
        }
    }
}

impl std::error::Error for ConfigError {}

impl From<toml::de::Error> for ConfigError {
    fn from(e: toml::de::Error) -> Self {
        ConfigError::Parse(e)
    }
}

/// 一份完整的 `rules.toml`。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RulesFile {
    /// schema 版本；缺失即解析失败（版本号是跨版本迁移的判据，不设隐式默认）。
    pub version: u64,
    /// 顶层兜底档（未命中任何配置时）。
    pub default: Option<Decision>,
    /// 桶间优先级；多命中合成与复合命令组合裁决共用此序（D-04）。
    pub precedence: Option<Vec<Decision>>,
    /// `[local]` 表：效果不出项目，allow 默认带路径逃逸检查。
    pub local: ScopeTable,
    /// `[global]` 表：allow 豁免逃逸检查。
    pub global: ScopeTable,
}

impl RulesFile {
    /// 解析并做语义校验（version 支持、`precedence` 完整性）。
    pub fn parse_toml(text: &str) -> Result<Self, ConfigError> {
        let file: Self = toml::from_str(text)?;
        if file.version != SUPPORTED_VERSION {
            return Err(ConfigError::Semantic(format!(
                "config schema version {} is not supported by this build (expected \
                 version {SUPPORTED_VERSION}); upgrade crush-tether or regenerate the file",
                file.version
            )));
        }
        if let Some(p) = &file.precedence {
            validate_precedence(p)?;
        }
        Ok(file)
    }
}

/// `precedence` 必须恰好列出三桶各一次（部分序未定义，宁严勿略）。
fn validate_precedence(p: &[Decision]) -> Result<(), ConfigError> {
    let complete = p.len() == 3
        && [Decision::Allow, Decision::Confirm, Decision::Deny]
            .iter()
            .all(|d| p.contains(d));
    if complete {
        Ok(())
    } else {
        Err(ConfigError::Semantic(format!(
            "`precedence` must list each of allow/confirm/deny exactly once, got {p:?}"
        )))
    }
}

/// 作用域表（`[local]` 或 `[global]`）：头部裸列表桶 + 任意命令节。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ScopeTable {
    /// 头部裸列表桶：词条是整条命令的 bin 名（语法糖，同层被命令节遮蔽）。
    pub buckets: ScopeBuckets,
    /// 命令节（`[local.git]` 等）；BTreeMap 保证遍历序确定（lint/输出可复现）。
    pub commands: BTreeMap<String, CommandSection>,
}

/// 作用域表的头部裸列表三桶。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ScopeBuckets {
    pub allow: Option<ListField>,
    pub confirm: Option<ListField>,
    pub deny: Option<ListField>,
}

/// 一个命令节（`[local.git]`）。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CommandSection {
    /// `allow` 桶（子命令/flag 维度）。
    pub allow: Option<BucketSpec>,
    /// `confirm` 桶。
    pub confirm: Option<BucketSpec>,
    /// `deny` 桶。
    pub deny: Option<BucketSpec>,
    /// 节内兜底档（如 `[local.npm] default = "allow"`）；跨层继承链在 M2.2 合并。
    pub default: Option<Decision>,
}

/// 命令节内一个桶的维度：`sub`（子命令）/ `flag`（flag 词条）。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BucketSpec {
    pub sub: Option<ListField>,
    pub flag: Option<ListField>,
}

/// 列表值双形态（D-02）：`Set` = 数组覆盖定义；`Delta` = 继承低层并增删。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ListField {
    Set(Vec<String>),
    Delta {
        add: Option<Vec<String>>,
        remove: Option<Vec<String>>,
    },
}

impl<'de> Deserialize<'de> for RulesFile {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Raw {
            version: u64,
            #[serde(default)]
            default: Option<Decision>,
            #[serde(default)]
            precedence: Option<Vec<Decision>>,
            #[serde(default)]
            local: ScopeTable,
            #[serde(default)]
            global: ScopeTable,
        }
        Raw::deserialize(deserializer).map(|r| RulesFile {
            version: r.version,
            default: r.default,
            precedence: r.precedence,
            local: r.local,
            global: r.global,
        })
    }
}

impl<'de> Deserialize<'de> for ScopeTable {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct ScopeTableVisitor;

        impl<'de> Visitor<'de> for ScopeTableVisitor {
            type Value = ScopeTable;

            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(
                    "a scope table ([local]/[global]): allow/confirm/deny bare lists plus \
                     per-command sections",
                )
            }

            fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
            where
                A: MapAccess<'de>,
            {
                let mut out = ScopeTable::default();
                while let Some(key) = map.next_key::<String>()? {
                    match key.as_str() {
                        "allow" => out.buckets.allow = Some(map.next_value()?),
                        "confirm" => out.buckets.confirm = Some(map.next_value()?),
                        "deny" => out.buckets.deny = Some(map.next_value()?),
                        // 其余键一律按命令节解析：拼错的桶键或裸数组值在此报错，
                        // 杜绝拼写错误静默变成「新命令节」。
                        _ => {
                            let section = map.next_value::<CommandSection>()?;
                            out.commands.insert(key, section);
                        }
                    }
                }
                Ok(out)
            }
        }

        deserializer.deserialize_map(ScopeTableVisitor)
    }
}

impl<'de> Deserialize<'de> for CommandSection {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct CommandSectionVisitor;

        impl<'de> Visitor<'de> for CommandSectionVisitor {
            type Value = CommandSection;

            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(
                    "a command section: allow/confirm/deny buckets (sub/flag) plus an \
                     optional default",
                )
            }

            fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
            where
                A: MapAccess<'de>,
            {
                const FIELDS: &[&str] = &["allow", "confirm", "deny", "default"];
                let mut out = CommandSection::default();
                while let Some(key) = map.next_key::<String>()? {
                    match key.as_str() {
                        "allow" => out.allow = Some(map.next_value()?),
                        "confirm" => out.confirm = Some(map.next_value()?),
                        "deny" => out.deny = Some(map.next_value()?),
                        "default" => out.default = Some(map.next_value()?),
                        _ => return Err(de::Error::unknown_field(&key, FIELDS)),
                    }
                }
                Ok(out)
            }
        }

        deserializer.deserialize_map(CommandSectionVisitor)
    }
}

impl<'de> Deserialize<'de> for BucketSpec {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct BucketSpecVisitor;

        impl<'de> Visitor<'de> for BucketSpecVisitor {
            type Value = BucketSpec;

            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("a bucket: `sub` and/or `flag` lists")
            }

            fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
            where
                A: MapAccess<'de>,
            {
                const FIELDS: &[&str] = &["sub", "flag"];
                let mut out = BucketSpec::default();
                while let Some(key) = map.next_key::<String>()? {
                    match key.as_str() {
                        "sub" => out.sub = Some(map.next_value()?),
                        "flag" => out.flag = Some(map.next_value()?),
                        _ => return Err(de::Error::unknown_field(&key, FIELDS)),
                    }
                }
                Ok(out)
            }
        }

        deserializer.deserialize_map(BucketSpecVisitor)
    }
}

impl<'de> Deserialize<'de> for ListField {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct ListFieldVisitor;

        impl<'de> Visitor<'de> for ListFieldVisitor {
            type Value = ListField;

            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("an array of tokens, or a table `{ add = [...], remove = [...] }`")
            }

            fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
            where
                A: SeqAccess<'de>,
            {
                let mut items = Vec::new();
                while let Some(t) = seq.next_element::<String>()? {
                    items.push(t);
                }
                Ok(ListField::Set(items))
            }

            fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
            where
                A: MapAccess<'de>,
            {
                let mut add = None;
                let mut remove = None;
                while let Some(key) = map.next_key::<String>()? {
                    match key.as_str() {
                        "add" => add = Some(map.next_value()?),
                        "remove" => remove = Some(map.next_value()?),
                        _ => return Err(de::Error::unknown_field(&key, &["add", "remove"])),
                    }
                }
                Ok(ListField::Delta { add, remove })
            }
        }

        deserializer.deserialize_any(ListFieldVisitor)
    }
}

impl<'de> Deserialize<'de> for Decision {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct DecisionVisitor;

        impl<'de> Visitor<'de> for DecisionVisitor {
            type Value = Decision;

            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("`allow`, `confirm` or `deny`")
            }

            fn visit_str<E: de::Error>(self, s: &str) -> Result<Self::Value, E> {
                match s {
                    "allow" => Ok(Decision::Allow),
                    "confirm" => Ok(Decision::Confirm),
                    "deny" => Ok(Decision::Deny),
                    other => Err(de::Error::unknown_variant(
                        other,
                        &["allow", "confirm", "deny"],
                    )),
                }
            }
        }

        deserializer.deserialize_str(DecisionVisitor)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(s: &str) -> Result<RulesFile, ConfigError> {
        RulesFile::parse_toml(s)
    }

    fn err_of(s: &str) -> String {
        parse(s).expect_err("should fail").to_string()
    }

    fn set(v: &[&str]) -> ListField {
        ListField::Set(v.iter().map(|s| (*s).to_string()).collect())
    }

    #[test]
    fn minimal_file_with_version_only_parses_empty() {
        let f = parse("version = 1").unwrap();
        assert_eq!(f.version, 1);
        assert_eq!(f.default, None);
        assert_eq!(f.precedence, None);
        assert_eq!(f.local, ScopeTable::default());
        assert_eq!(f.global, ScopeTable::default());
    }

    #[test]
    fn missing_version_is_an_error_not_an_implicit_v1() {
        let msg = err_of("default = \"confirm\"");
        assert!(msg.contains("missing field `version`"), "{msg}");
    }

    #[test]
    fn newer_schema_version_is_rejected_explicitly() {
        let msg = err_of("version = 2");
        assert!(msg.contains("version 2 is not supported"), "{msg}");
    }

    #[test]
    fn version_zero_is_rejected() {
        let msg = err_of("version = 0");
        assert!(msg.contains("version 0 is not supported"), "{msg}");
    }

    #[test]
    fn unknown_top_level_key_is_locatable() {
        let msg = err_of("version = 1\nfoo = 1");
        assert!(msg.contains("unknown field `foo`"), "{msg}");
    }

    #[test]
    fn head_buckets_accept_array_form() {
        let f = parse("version = 1\n[local]\nallow = [\"ls\", \"cat\"]").unwrap();
        assert_eq!(f.local.buckets.allow, Some(set(&["ls", "cat"])));
        assert_eq!(f.local.buckets.confirm, None);
        assert_eq!(f.global.buckets.allow, None);
    }

    #[test]
    fn head_buckets_accept_delta_form() {
        let src = "version = 1\n[local]\nallow = { add = [\"jq\"], remove = [\"curl\"] }";
        let f = parse(src).unwrap();
        assert_eq!(
            f.local.buckets.allow,
            Some(ListField::Delta {
                add: Some(vec!["jq".into()]),
                remove: Some(vec!["curl".into()]),
            })
        );
    }

    #[test]
    fn delta_form_rejects_unknown_keys() {
        let msg = err_of("version = 1\n[local]\nallow = { add = [], foo = [] }");
        assert!(msg.contains("unknown field `foo`"), "{msg}");
        assert!(msg.contains("add"), "{msg}");
    }

    #[test]
    fn head_bucket_rejects_non_list_values() {
        let msg = err_of("version = 1\n[local]\nallow = \"ls\"");
        assert!(msg.contains("invalid type: string"), "{msg}");
    }

    #[test]
    fn head_level_bucket_cannot_open_sub_dimension() {
        // sub/flag 只存在于命令节；头部裸列表词条是整命令。
        let msg = err_of("version = 1\n[local]\nallow.sub = [\"x\"]");
        assert!(msg.contains("unknown field `sub`"), "{msg}");
    }

    #[test]
    fn head_level_typo_bucket_is_not_silently_a_command() {
        let msg = err_of("version = 1\n[local]\nalow = [\"ls\"]");
        assert!(msg.contains("alow"), "{msg}");
        assert!(msg.contains("command section"), "{msg}");
    }

    #[test]
    fn command_section_sub_and_flag_dimensions_parse() {
        let src = concat!(
            "version = 1\n",
            "[local.git]\n",
            "allow.sub = [\"status\", \"log\"]\n",
            "confirm.flag = [\"--output\", \"-o\"]\n",
            "deny.flag = [\"--hard\"]\n",
        );
        let f = parse(src).unwrap();
        let git = f.local.commands.get("git").unwrap();
        assert_eq!(
            git.allow.as_ref().unwrap().sub,
            Some(set(&["status", "log"]))
        );
        assert_eq!(
            git.confirm.as_ref().unwrap().flag,
            Some(set(&["--output", "-o"]))
        );
        assert_eq!(git.deny.as_ref().unwrap().flag, Some(set(&["--hard"])));
        assert_eq!(git.default, None);
    }

    #[test]
    fn command_section_inline_default_parses() {
        let src = "version = 1\n[local.npm]\nconfirm.sub = [\"install\"]\ndefault = \"allow\"";
        let f = parse(src).unwrap();
        let npm = f.local.commands.get("npm").unwrap();
        assert_eq!(npm.default, Some(Decision::Allow));
        assert_eq!(npm.confirm.as_ref().unwrap().sub, Some(set(&["install"])));
    }

    #[test]
    fn command_section_rejects_typo_bucket_key() {
        let msg = err_of("version = 1\n[local.git]\nalow = [\"x\"]");
        assert!(msg.contains("unknown field `alow`"), "{msg}");
    }

    #[test]
    fn bucket_rejects_unknown_dimension_key() {
        let msg = err_of("version = 1\n[local.git]\nallow.subs = [\"x\"]");
        assert!(msg.contains("unknown field `subs`"), "{msg}");
    }

    #[test]
    fn empty_command_section_is_valid() {
        let f = parse("version = 1\n[local.git]").unwrap();
        assert_eq!(
            f.local.commands.get("git"),
            Some(&CommandSection::default())
        );
    }

    #[test]
    fn global_table_structurally_identical_to_local() {
        let src = "version = 1\n[global]\nallow = []\n[global.docker]\nallow.sub = [\"ps\"]";
        let f = parse(src).unwrap();
        assert_eq!(f.global.buckets.allow, Some(set(&[])));
        let docker = f.global.commands.get("docker").unwrap();
        assert_eq!(docker.allow.as_ref().unwrap().sub, Some(set(&["ps"])));
    }

    #[test]
    fn quoted_command_names_parse() {
        let f = parse("version = 1\n[local.\"google-chrome\"]\ndefault = \"deny\"").unwrap();
        assert_eq!(
            f.local.commands.get("google-chrome").unwrap().default,
            Some(Decision::Deny)
        );
    }

    #[test]
    fn misplaced_bare_key_after_header_is_locatable() {
        let msg = err_of("version = 1\n[global]\nversion = 2");
        assert!(msg.contains("command section"), "{msg}");
    }

    #[test]
    fn precedence_accepts_any_permutation_of_three_buckets() {
        let f = parse("version = 1\nprecedence = [\"allow\", \"deny\", \"confirm\"]").unwrap();
        assert_eq!(
            f.precedence,
            Some(vec![Decision::Allow, Decision::Deny, Decision::Confirm])
        );
    }

    #[test]
    fn precedence_rejects_missing_bucket() {
        let msg = err_of("version = 1\nprecedence = [\"deny\", \"confirm\"]");
        assert!(msg.contains("precedence"), "{msg}");
    }

    #[test]
    fn precedence_rejects_duplicate_bucket() {
        let msg = err_of("version = 1\nprecedence = [\"deny\", \"deny\", \"allow\"]");
        assert!(msg.contains("precedence"), "{msg}");
    }

    #[test]
    fn precedence_rejects_unknown_bucket_name() {
        let msg = err_of("version = 1\nprecedence = [\"deny\", \"confirm\", \"read\"]");
        assert!(msg.contains("unknown variant `read`"), "{msg}");
    }

    #[test]
    fn default_accepts_three_decisions_and_rejects_others() {
        for (s, want) in [
            ("allow", Decision::Allow),
            ("confirm", Decision::Confirm),
            ("deny", Decision::Deny),
        ] {
            let f = parse(&format!("version = 1\ndefault = \"{s}\"")).unwrap();
            assert_eq!(f.default, Some(want));
        }
        let msg = err_of("version = 1\ndefault = \"Allow\"");
        assert!(msg.contains("unknown variant `Allow`"), "{msg}");
    }
}
