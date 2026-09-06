//! crush_tether —— Crush/ClaudeCode 命令级 bash 权限门。
//!
//! 三档分类（allow / confirm / deny），AST 拉平逐条简单命令判定后组合裁决。
//! 设计单一事实源见 `doc/design.md`。
#![deny(missing_docs)]

pub mod channel;
pub mod cmd_parse;
pub mod config;
pub mod engine;
pub mod knowledge;
pub mod lint;
pub mod lookup;
pub mod model;
pub mod script;
pub mod service;

/// 测试基建（仅测试构建编译，不入公共 API）。
#[cfg(test)]
pub(crate) mod testutil;
