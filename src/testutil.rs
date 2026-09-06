//! lib 内联单测的共享测试基建（`#[cfg(test)]` 专用）。
//!
//! 集成测试（tests/）的助手在 `tests/common/mod.rs`——两者编译上下文
//! 不同、各自收敛；本模块只收 lib 内联单测的第 3 次重复（三次法则）。

use std::path::{Path, PathBuf};

/// 自清理的临时目录（`scope` 区分使用方模块，避免同 tag 撞名）。
pub(crate) struct TempDir(PathBuf);

impl TempDir {
    pub(crate) fn new(scope: &str, tag: &str) -> Self {
        let d =
            std::env::temp_dir().join(format!("crush-tether-{scope}-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).expect("create temp dir");
        TempDir(d)
    }

    pub(crate) fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
