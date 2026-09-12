//! 三层配置发现：项目 > 用户 > 全局。
//!
//! - 项目层：`<project_root>/.crush-tether/rules.toml`；项目根
//!   `CRUSH_PROJECT_DIR` 优先，缺失时从 cwd 逐级上溯找最近 `.git` 或
//!   `.crush-tether/`（design.md「配置分层与优先级」）。
//! - 用户层：`~/.config/crush-tether/rules.toml`。
//! - 全局层：`/etc/crush-tether/rules.toml`（Unix）或
//!   `%PROGRAMDATA%\crush-tether\rules.toml`（Windows）；环境变量
//!   `CRUSH_TETHER_GLOBAL_DIR` 优先（测试/便携覆盖，P8/M8.1 定稿 D-09）。
//!
//! 损坏 ≠ 缺失（D-03）：任一层「存在但加载/解析失败」→ 整体 Err，调用方
//! 告警 + fail-safe confirm；绝不带着坏层静默用其余层裁决。

use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::config::{LoadError, RulesFile};
use crate::knowledge::KnowledgeBase;

/// 发现到的三层配置（`None` = 该层无配置文件）。
#[derive(Debug)]
pub struct FoundLayers {
    /// 全局层规则（系统路径或 `CRUSH_TETHER_GLOBAL_DIR`，见 [`global_dir`]）。
    pub global: Option<RulesFile>,
    /// 用户层规则（`~/.config/crush-tether/rules.toml`）。
    pub user: Option<RulesFile>,
    /// 项目层规则（`<project>/.crush-tether/rules.toml`）。
    pub project: Option<RulesFile>,
    /// 知识库 main：项目层 `.crush-tether/knowledge.toml`（v1 单文件，随 init
    /// 默认包落盘）。`Arc` 共享：RuleSet 装配的多处消费免深克隆。
    pub knowledge: Option<Arc<KnowledgeBase>>,
}

/// 全局层配置目录（P8/M8.1 定稿）：`CRUSH_TETHER_GLOBAL_DIR` 优先（测试/
/// 便携覆盖），否则系统路径——Unix `/etc/crush-tether`，Windows
/// `%PROGRAMDATA%\crush-tether`；平台变量缺失 → None（该层跳过）。
pub fn global_dir() -> Option<PathBuf> {
    if let Some(d) = std::env::var_os("CRUSH_TETHER_GLOBAL_DIR")
        && !d.is_empty()
    {
        return Some(PathBuf::from(d));
    }
    #[cfg(windows)]
    {
        std::env::var_os("PROGRAMDATA").map(|d| PathBuf::from(d).join("crush-tether"))
    }
    #[cfg(not(windows))]
    {
        Some(PathBuf::from("/etc/crush-tether"))
    }
}

/// 逐层发现并加载。`project_root` / `home` / `global` 显式传入以便测试
/// （调用方经 [`find_project_root`] / [`home_dir`] / [`global_dir`] 解析；
/// None = 跳过该层）。
pub fn discover_layers(
    project_root: Option<&Path>,
    home: Option<&Path>,
    global: Option<&Path>,
) -> Result<FoundLayers, LoadError> {
    let global = match global {
        Some(g) => load_optional(&g.join("rules.toml"))?,
        None => None,
    };
    let user = match home {
        Some(h) => load_optional(&h.join(".config").join("crush-tether").join("rules.toml"))?,
        None => None,
    };
    let knowledge = match project_root {
        Some(p) => load_knowledge(&p.join(".crush-tether").join("knowledge.toml")).map(Arc::new),
        None => None,
    };
    let project = match project_root {
        Some(p) => load_optional(&p.join(".crush-tether").join("rules.toml"))?,
        None => None,
    };
    Ok(FoundLayers {
        global,
        user,
        project,
        knowledge,
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

/// 加载知识库 main。与规则文件不同：**损坏按「缺失 + stderr 告警」处理**，
/// 不触发 fail-safe confirm——知识库只记录事实、不产生裁决，删光/损坏的
/// 后果是归一与语义检查失效（等价命令按字面查表，落 default 兜底），判定
/// 完全不受影响（design.md「删光 = 不做语义检查」）。
fn load_knowledge(path: &Path) -> Option<KnowledgeBase> {
    match std::fs::read_to_string(path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
        Err(e) => {
            eprintln!(
                "crush-tether: knowledge file {} unreadable: {e}; treating as absent",
                path.display()
            );
            None
        }
        Ok(text) => match KnowledgeBase::parse_toml(&text) {
            Ok(kb) => Some(kb),
            Err(e) => {
                eprintln!(
                    "crush-tether: knowledge file {} invalid: {e}; treating as absent",
                    path.display()
                );
                None
            }
        },
    }
}

/// 配置发现用的项目根（项目根解析的单一实现）：
/// `CRUSH_PROJECT_DIR` → `CLAUDE_PROJECT_DIR`（hook 注入，最可靠来源）；
/// 缺失时从 cwd 逐级上溯；都不命中 → cwd。
pub fn find_project_root() -> PathBuf {
    for key in ["CRUSH_PROJECT_DIR", "CLAUDE_PROJECT_DIR"] {
        if let Ok(dir) = std::env::var(key)
            && !dir.is_empty()
        {
            return PathBuf::from(dir);
        }
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

    use crate::testutil::TempDir;

    #[test]
    fn project_root_walks_up_to_nearest_marker() {
        let root = TempDir::new("m22", "walkup");
        let nested = root.path().join("a").join("b");
        std::fs::create_dir_all(&nested).unwrap();
        std::fs::create_dir_all(root.path().join(".git")).unwrap();

        assert_eq!(find_project_root_from(&nested), root.path());
    }

    #[test]
    fn project_root_walks_up_to_crush_tether_dir() {
        let root = TempDir::new("m22", "walkup-ct");
        let nested = root.path().join("x");
        std::fs::create_dir_all(&nested).unwrap();
        std::fs::create_dir_all(root.path().join(".crush-tether")).unwrap();

        assert_eq!(find_project_root_from(&nested), root.path());
    }

    #[test]
    fn project_root_falls_back_to_start_without_marker() {
        let plain = TempDir::new("m22", "plain");
        let nested = plain.path().join("deep");
        std::fs::create_dir_all(&nested).unwrap();

        assert_eq!(find_project_root_from(&nested), nested);
    }

    #[test]
    fn discover_layers_reads_user_and_project_layers() {
        let home = TempDir::new("m22", "home");
        let proj = TempDir::new("m22", "proj");
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

        let found = discover_layers(Some(proj.path()), Some(home.path()), None).unwrap();
        assert!(found.global.is_none(), "未传全局目录则该层跳过");
        assert_eq!(found.user.as_ref().unwrap().default, Some(Decision::Deny));
        let proj_file = found.project.as_ref().unwrap();
        match &proj_file.local.buckets.allow {
            Some(ListField::Set(v)) => assert!(v.contains(&"ls".to_string())),
            other => panic!("project allow should be a Set, got {other:?}"),
        }
    }

    #[test]
    fn discover_layers_reads_global_layer() {
        // P8/M8.1：全局层发现路径定稿（D-09）。
        let sys = TempDir::new("m81", "sys");
        std::fs::write(
            sys.path().join("rules.toml"),
            "version = 1\ndefault = \"deny\"",
        )
        .unwrap();
        let found = discover_layers(None, None, Some(sys.path())).unwrap();
        assert_eq!(found.global.as_ref().unwrap().default, Some(Decision::Deny));
    }

    #[test]
    fn discover_layers_all_none_when_no_files() {
        let home = TempDir::new("m22", "home-empty");
        let proj = TempDir::new("m22", "proj-empty");
        let found = discover_layers(Some(proj.path()), Some(home.path()), None).unwrap();
        assert!(found.global.is_none() && found.user.is_none() && found.project.is_none());
    }

    #[test]
    fn discover_layers_skips_layer_without_home() {
        let proj = TempDir::new("m22", "proj-nohome");
        let found = discover_layers(Some(proj.path()), None, None).unwrap();
        assert!(found.user.is_none());
        assert!(found.project.is_none() && found.global.is_none());
    }

    #[test]
    fn discover_layers_broken_file_is_error_not_absence() {
        // 损坏 ≠ 缺失（D-03）：存在但解析失败 → 整体 Err，调用方 fail-safe。
        let proj = TempDir::new("m22", "proj-broken");
        let proj_cfg = proj.path().join(".crush-tether");
        std::fs::create_dir_all(&proj_cfg).unwrap();
        std::fs::write(proj_cfg.join("rules.toml"), "version = 1\nalow = []").unwrap();

        assert!(discover_layers(Some(proj.path()), None, None).is_err());
    }

    #[test]
    fn discover_layers_broken_global_is_error_not_absence() {
        // 全局层同受 D-03 约束：坏全局层 → 整体 Err，不带坏层静默裁决。
        let sys = TempDir::new("m81", "sys-broken");
        std::fs::write(sys.path().join("rules.toml"), "version = 1\nalow = []").unwrap();
        assert!(discover_layers(None, None, Some(sys.path())).is_err());
    }
}
