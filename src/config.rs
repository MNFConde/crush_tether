//! 配置加载（声明层 TOML）。
//!
//! 脚本层（Rhai/Lua）与三层 merge 属 P2/P3 里程碑；当前阶段仅保留模块
//! 占位与可序列化的声明层骨架，规则链内置判定表见 `engine`。

use serde::Deserialize;

/// 声明层规则（`.crush-tether/rules.toml` 的 `[[rules]]`）。
#[derive(Debug, Clone, Deserialize)]
pub struct DeclaredRule {
    /// 匹配的命令二进制名（精确匹配首词元）。
    pub bin: String,
    /// 可选子命令匹配。
    #[serde(default)]
    pub subcommand: Option<String>,
    /// 命中后的三档决策。
    pub then: String,
}

/// rules.toml 顶层结构（当前仅声明规则；命令集合/安全配置随 P2 扩展）。
#[derive(Debug, Clone, Default, Deserialize)]
pub struct RulesConfig {
    #[serde(default, rename = "rule")]
    pub rules: Vec<DeclaredRule>,
}

impl RulesConfig {
    pub fn parse_toml(text: &str) -> Result<Self, toml::de::Error> {
        toml::from_str(text)
    }
}
