//! 配置层：显式覆盖入口、三层发现、字段级继承合并与单文件解析。
//!
//! 结构与语义单一事实源：`doc/design.md`「配置格式与脚本边界（草案 v1）」。
//! - `schema`：rules.toml 数据模型与严格解析（未知键报错可定位）。
//! - `discover`：项目 > 用户 > 全局三层发现（损坏 ≠ 缺失，D-03）。
//! - `merge`：字段级继承合并（未定义即继承 / 定义即覆盖 / 增删，D-02）。
//! - `--config` / `CRUSH_TETHER_CONFIG` 显式覆盖优先于所有层。

pub mod discover;
pub mod merge;
pub mod schema;
pub mod seed;

use std::fmt;
use std::path::{Path, PathBuf};

pub use discover::{
    FoundLayers, discover_layers, find_project_root, find_project_root_from, home_dir,
};
pub use merge::{
    DEFAULT_PRECEDENCE, Dims, LayerLabels, Layers, MergedCommand, MergedRules, MergedScope,
    Provenance, ScopeProvenance, merge, merge_with_labels,
};
pub use schema::{
    BucketSpec, CommandSection, ConfigError, ListField, RulesFile, SUPPORTED_VERSION, ScopeBuckets,
    ScopeTable,
};

/// 显式覆盖路径：`--config` 参数优先，其次 `CRUSH_TETHER_CONFIG` 环境变量；
/// 有显式路径时不再做分层发现（优先级高于所有层，见 design.md「配置分层
/// 与优先级」）。
pub fn explicit_path(cli: Option<&str>) -> Option<PathBuf> {
    cli.map(PathBuf::from)
        .or_else(|| std::env::var_os("CRUSH_TETHER_CONFIG").map(PathBuf::from))
}

/// 单文件加载失败原因。
#[derive(Debug)]
pub enum LoadError {
    /// 文件读取失败（不存在 / 无权限 / 非 UTF-8）。
    Io(std::io::Error),
    /// 内容解析或语义校验失败。
    Parse(ConfigError),
}

impl fmt::Display for LoadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LoadError::Io(e) => write!(f, "io error: {e}"),
            LoadError::Parse(e) => write!(f, "parse error: {e}"),
        }
    }
}

impl std::error::Error for LoadError {}

/// 读取并解析一份规则文件。
pub fn load_file(path: &Path) -> Result<RulesFile, LoadError> {
    let text = std::fs::read_to_string(path).map_err(LoadError::Io)?;
    RulesFile::parse_toml(&text).map_err(LoadError::Parse)
}
