//! 三层配置发现：项目 > 用户 > 全局。
//!
//! - 项目层：`<project_root>/.crush-tether/rules.toml`；项目根
//!   `CRUSH_PROJECT_DIR` 优先，缺失时从 cwd 逐级上溯找最近 `.git` 或
//!   `.crush-tether/`（design.md「配置分层与优先级」）。
//! - 用户层：`~/.config/crush-tether/rules.toml`。
//! - 全局层：v1 不设发现路径（合并逻辑与单测就位，路径后期设计）。
//!
//! 损坏 ≠ 缺失（D-03）：任一层「存在但加载/解析失败」→ 整体 Err，调用方
//! 告警 + fail-safe confirm；绝不带着坏层静默用其余层裁决。

use std::path::{Path, PathBuf};

use crate::config::{LoadError, RulesFile};

/// 发现到的三层配置（`None` = 该层无配置文件）。
#[derive(Debug)]
pub struct FoundLayers {
    /// v1 恒 None（无发现路径），为后期设计留位。
    pub global: Option<RulesFile>,
    pub user: Option<RulesFile>,
    pub project: Option<RulesFile>,
}

impl FoundLayers {
    /// 是否三层皆缺（默认配置生成的触发条件，M2.6 消费）。
    pub fn all_absent(&self) -> bool {
        self.global.is_none() && self.user.is_none() && self.project.is_none()
    }
}

/// 逐层发现并加载。`project_root` / `home` 显式传入以便测试（调用方经
/// [`find_project_root`] / [`home_dir`] 解析；None = 跳过该层）。
pub fn discover_layers(
    project_root: Option<&Path>,
    home: Option<&Path>,
) -> Result<FoundLayers, LoadError> {
    let user = match home {
        Some(h) => load_optional(&h.join(".config").join("crush-tether").join("rules.toml"))?,
        None => None,
    };
    let project = match project_root {
        Some(p) => load_optional(&p.join(".crush-tether").join("rules.toml"))?,
        None => None,
    };
    Ok(FoundLayers {
        global: None,
        user,
        project,
    })
}

/// 存在则加载；不存在（NotFound）→ None；其余（解析失败/权限等）→ Err。
fn load_optional(path: &Path) -> Result<Option<RulesFile>, LoadError> {
    match crate::config::load_file(path) {
        Ok(f) => Ok(Some(f)),
        Err(LoadError::Io(e)) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e),
    }
}

/// 配置发现用的项目根：`CRUSH_PROJECT_DIR` 优先（hook 注入，最可靠来源）；
/// 缺失时从 cwd 逐级上溯；都不命中 → cwd。
pub fn find_project_root() -> PathBuf {
    if let Ok(dir) = std::env::var("CRUSH_PROJECT_DIR")
        && !dir.is_empty()
    {
        return PathBuf::from(dir);
    }
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    find_project_root_from(&cwd)
}

/// 从 `start` 逐级上溯，返回第一个含 `.git` 或 `.crush-tether/` 的目录；
/// 上溯到顶都没有 → `start` 原样（配置层按缺失处理）。
pub fn find_project_root_from(start: &Path) -> PathBuf {
    let mut cur = Some(start);
    while let Some(dir) = cur {
        if dir.join(".git").exists() || dir.join(".crush-tether").is_dir() {
            return dir.to_path_buf();
        }
        cur = dir.parent();
    }
    start.to_path_buf()
}

/// 用户主目录（`USERPROFILE` 优先，兼容 `HOME`）；两者皆缺 → None。
pub fn home_dir() -> Option<PathBuf> {
    std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .map(PathBuf::from)
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::config::schema::ListField;
    use crate::model::Decision;

    struct TempDir(PathBuf);

    impl TempDir {
        fn new(tag: &str) -> Self {
            let d =
                std::env::temp_dir().join(format!("crush-tether-m22-{}-{tag}", std::process::id()));
            let _ = std::fs::remove_dir_all(&d);
            std::fs::create_dir_all(&d).expect("create temp dir");
            TempDir(d)
        }

        fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn project_root_walks_up_to_nearest_marker() {
        let root = TempDir::new("walkup");
        let nested = root.path().join("a").join("b");
        std::fs::create_dir_all(&nested).unwrap();
        std::fs::create_dir_all(root.path().join(".git")).unwrap();

        assert_eq!(find_project_root_from(&nested), root.path());
    }

    #[test]
    fn project_root_walks_up_to_crush_tether_dir() {
        let root = TempDir::new("walkup-ct");
        let nested = root.path().join("x");
        std::fs::create_dir_all(&nested).unwrap();
        std::fs::create_dir_all(root.path().join(".crush-tether")).unwrap();

        assert_eq!(find_project_root_from(&nested), root.path());
    }

    #[test]
    fn project_root_falls_back_to_start_without_marker() {
        let plain = TempDir::new("plain");
        let nested = plain.path().join("deep");
        std::fs::create_dir_all(&nested).unwrap();

        assert_eq!(find_project_root_from(&nested), nested);
    }

    #[test]
    fn discover_layers_reads_user_and_project_layers() {
        let home = TempDir::new("home");
        let proj = TempDir::new("proj");
        let user_cfg = home.path().join(".config").join("crush-tether");
        std::fs::create_dir_all(&user_cfg).unwrap();
        std::fs::write(
            user_cfg.join("rules.toml"),
            "version = 1\ndefault = \"deny\"",
        )
        .unwrap();
        let proj_cfg = proj.path().join(".crush-tether");
        std::fs::create_dir_all(&proj_cfg).unwrap();
        std::fs::write(
            proj_cfg.join("rules.toml"),
            "version = 1\n[local]\nallow = [\"ls\"]",
        )
        .unwrap();

        let found = discover_layers(Some(proj.path()), Some(home.path())).unwrap();
        assert!(found.global.is_none(), "v1 全局层无发现路径");
        assert_eq!(found.user.as_ref().unwrap().default, Some(Decision::Deny));
        let proj_file = found.project.as_ref().unwrap();
        match &proj_file.local.buckets.allow {
            Some(ListField::Set(v)) => assert!(v.contains(&"ls".to_string())),
            other => panic!("project allow should be a Set, got {other:?}"),
        }
        assert!(!found.all_absent());
    }

    #[test]
    fn discover_layers_all_absent_when_no_files() {
        let home = TempDir::new("home-empty");
        let proj = TempDir::new("proj-empty");
        let found = discover_layers(Some(proj.path()), Some(home.path())).unwrap();
        assert!(found.all_absent());
    }

    #[test]
    fn discover_layers_skips_layer_without_home() {
        let proj = TempDir::new("proj-nohome");
        let found = discover_layers(Some(proj.path()), None).unwrap();
        assert!(found.user.is_none());
        assert!(found.all_absent());
    }

    #[test]
    fn discover_layers_broken_file_is_error_not_absence() {
        // 损坏 ≠ 缺失（D-03）：存在但解析失败 → 整体 Err，调用方 fail-safe。
        let proj = TempDir::new("proj-broken");
        let proj_cfg = proj.path().join(".crush-tether");
        std::fs::create_dir_all(&proj_cfg).unwrap();
        std::fs::write(proj_cfg.join("rules.toml"), "version = 1\nalow = []").unwrap();

        assert!(discover_layers(Some(proj.path()), None).is_err());
    }
}
