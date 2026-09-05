//! 配置层：显式覆盖入口、单文件加载与解析校验。
//!
//! 分层语义（项目 > 用户 > 全局、字段级继承合并）在 M2.2 落地；本模块当前
//! 提供 `--config` / `CRUSH_TETHER_CONFIG` 显式覆盖解析（优先级高于所有层）
//! 与单文件解析。结构与语义单一事实源：`doc/design.md`「配置格式与脚本
//! 边界（草案 v1）」。

pub mod schema;

use std::fmt;
use std::path::{Path, PathBuf};

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
