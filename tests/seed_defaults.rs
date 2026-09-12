//! 验收（M2.6 → P8/M8.1）：默认配置包由 `init` 显式生成（D-09——自动生成
//! 已移除，三层皆缺 = 裸兜底 confirm + 提示）；三层发现与字段级继承端到端
//! （项目 > 用户 > 全局）；损坏 fail-safe；模板与 design.md 示例逐行一致
//! （文档 = 单一事实源）。

mod common;

use common::{TempDir, run_check, run_check_env, run_init};
use crush_tether::config::seed::{DEFAULT_KNOWLEDGE_TOML, DEFAULT_RULES_TOML};
use std::path::Path;

const DESIGN_MD: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/doc/design.md");

fn extract_example(md: &str, section: &str) -> String {
    let section = md.split(section).nth(1).expect("section exists");
    let start = section.find("```toml").expect("toml block") + "```toml".len();
    let rest = &section[start..];
    let end = rest.find("```").expect("closed");
    rest[..end].to_string()
}

#[test]
fn templates_match_design_md_examples_byte_for_byte() {
    // 行尾归一：checkout 环境可能把任一侧转成 CRLF（如 CI Windows runner
    // autocrlf=true 时 include_str! 嵌入的模板），护栏语义是逐行内容一致，
    // 换行符不入比对。
    let md = std::fs::read_to_string(Path::new(DESIGN_MD))
        .expect("read design.md")
        .replace('\r', "");
    let rules = DEFAULT_RULES_TOML.replace('\r', "");
    let knowledge = DEFAULT_KNOWLEDGE_TOML.replace('\r', "");
    assert_eq!(
        rules.trim(),
        extract_example(&md, "### `rules.toml` 结构").trim(),
        "默认 rules.toml 模板必须与 design.md 示例一致"
    );
    assert_eq!(
        knowledge.trim(),
        extract_example(&md, "### 命令知识库（bucket 框架，定稿）").trim(),
        "默认 knowledge.toml 模板必须与 design.md 示例一致"
    );
}

#[test]
fn init_project_generates_default_pack_and_gates() {
    let proj = TempDir::new("init-proj");
    let r = run_init(proj.path(), &[], &[]);
    assert_eq!(r.code, 0, "{}", r.stderr);
    assert!(r.stdout.contains("wrote 3"), "{}", r.stdout);
    let dir = proj.path().join(".crush-tether");
    assert!(dir.join("rules.toml").is_file());
    assert!(dir.join("knowledge.toml").is_file());
    assert!(dir.join("rules.rhai").is_file());
    // init 后按默认包裁决：ls ∈ [local].allow → 放行。
    let r = run_check(proj.path(), "ls");
    assert_eq!(r.code, 0);
    assert_eq!(
        r.stdout.trim(),
        "{\"decision\":\"allow\"}",
        "init 后按默认包裁决"
    );
    assert!(
        !r.stderr.contains("no config found"),
        "有配置不再提示 init：{}",
        r.stderr
    );
    // 幂等：二次 init 不覆写（已存在全跳过）。
    let r = run_init(proj.path(), &[], &[]);
    assert_eq!(r.code, 0);
    assert!(r.stdout.contains("already present"), "{}", r.stdout);
}

#[test]
fn no_config_falls_back_to_bare_confirm_with_hint() {
    // D-09：三层皆缺 → 裸兜底 confirm（静默 exit 0）+ stderr 提示 init，
    // 不再静默落盘任何文件。
    let proj = TempDir::new("bare");
    let r = run_check(proj.path(), "git status");
    assert_eq!(r.code, 0, "裸兜底 confirm = 静默 exit 0：{}", r.stderr);
    assert!(r.stdout.trim().is_empty());
    assert!(
        r.stderr.contains("no config found") && r.stderr.contains("init"),
        "三层皆缺应提示 init：{}",
        r.stderr
    );
    assert!(
        !proj.path().join(".crush-tether").exists(),
        "自动生成已移除：check 不写配置"
    );
}

#[test]
fn init_never_touches_existing_files() {
    let proj = TempDir::new("init-respect");
    let dir = proj.path().join(".crush-tether");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("rules.toml"), "version = 1\ndefault = \"deny\"\n").unwrap();
    let r = run_init(proj.path(), &[], &[]);
    assert_eq!(r.code, 0, "{}", r.stderr);
    assert!(r.stdout.contains("wrote 2"), "只补缺失件：{}", r.stdout);
    assert_eq!(
        std::fs::read_to_string(dir.join("rules.toml")).unwrap(),
        "version = 1\ndefault = \"deny\"\n",
        "已存在文件原样保留"
    );
}

#[test]
fn init_user_layer_writes_and_gates() {
    let proj = TempDir::new("init-user");
    let user_home = TempDir::new("init-user-home");
    let up = user_home.path().to_string_lossy().into_owned();
    let r = run_init(
        proj.path(),
        &["--user"],
        &[("USERPROFILE", up.as_str()), ("HOME", up.as_str())],
    );
    assert_eq!(r.code, 0, "{}", r.stderr);
    assert!(
        user_home
            .path()
            .join(".config")
            .join("crush-tether")
            .join("rules.toml")
            .is_file(),
        "--user 应写入 <home>/.config/crush-tether/"
    );
    // 用户层默认包生效：ls 放行（项目层无配置，全局层被隔离清空）。
    let r = run_check_env(
        proj.path(),
        &[],
        "ls",
        &[("USERPROFILE", up.as_str()), ("HOME", up.as_str())],
    );
    assert_eq!(
        r.stdout.trim(),
        "{\"decision\":\"allow\"}",
        "用户层默认包裁决：{}",
        r.stderr
    );
}

#[test]
fn init_rejects_user_and_global_together() {
    let proj = TempDir::new("init-mutex");
    let r = run_init(proj.path(), &["--user", "--global"], &[]);
    assert_eq!(r.code, 2);
    assert!(r.stderr.contains("mutually exclusive"), "{}", r.stderr);
}

#[test]
fn three_layer_precedence_end_to_end() {
    // 效力顺序项目 > 用户 > 全局（D-09 全局层落地后的端到端钉死）：
    // 全局 default=deny → 用户 allow=[ls] 覆盖之 → 项目 deny=[ls] 再覆盖。
    let proj = TempDir::new("prec");
    let user_home = TempDir::new("prec-home");
    let sys_dir = TempDir::new("prec-sys");
    let up = user_home.path().to_string_lossy().into_owned();
    let gp = sys_dir.path().to_string_lossy().into_owned();
    std::fs::write(
        sys_dir.path().join("rules.toml"),
        "version = 1\ndefault = \"deny\"",
    )
    .unwrap();

    // ① 只全局：default=deny → ls 阻断（exit 2；项目层与用户层皆缺）。
    let r = run_check_env(
        proj.path(),
        &[],
        "ls",
        &[("CRUSH_TETHER_GLOBAL_DIR", gp.as_str())],
    );
    assert_eq!(r.code, 2, "全局 default=deny 兜底：{}", r.stderr);

    // ② +用户层 allow=[ls]：用户覆盖全局 default → 放行。
    let user_cfg = user_home.path().join(".config").join("crush-tether");
    std::fs::create_dir_all(&user_cfg).unwrap();
    std::fs::write(
        user_cfg.join("rules.toml"),
        "version = 1\n[local]\nallow = [\"ls\"]",
    )
    .unwrap();
    let both = &[
        ("USERPROFILE", up.as_str()),
        ("HOME", up.as_str()),
        ("CRUSH_TETHER_GLOBAL_DIR", gp.as_str()),
    ];
    let r = run_check_env(proj.path(), &[], "ls", both);
    assert_eq!(
        r.stdout.trim(),
        "{\"decision\":\"allow\"}",
        "用户层 allow 覆盖全局 default：{}",
        r.stderr
    );

    // ③ +项目层 deny=[ls]：项目覆盖用户 allow（项目 allow 未定义 → 继承
    // 用户，字段级继承的双向验证），多命中 precedence deny 优先 → 阻断。
    let proj_cfg = proj.path().join(".crush-tether");
    std::fs::create_dir_all(&proj_cfg).unwrap();
    std::fs::write(
        proj_cfg.join("rules.toml"),
        "version = 1\n[local]\ndeny = [\"ls\"]",
    )
    .unwrap();
    let r = run_check_env(proj.path(), &[], "ls", both);
    assert_eq!(r.code, 2, "项目层 deny 覆盖用户 allow：{}", r.stderr);
}

#[test]
fn existing_project_layer_is_respected_no_generation() {
    let proj = TempDir::new("respect");
    let dir = proj.path().join(".crush-tether");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("rules.toml"), "version = 1\ndefault = \"deny\"\n").unwrap();
    let r = run_check(proj.path(), "ls");
    assert_eq!(r.code, 2, "项目层 default=deny 生效：ls 落 deny exit 2");
    assert!(r.stdout.trim().is_empty());
    assert!(
        !r.stderr.contains("no config found"),
        "任一层有效配置即尊重现状；got: {}",
        r.stderr
    );
    assert!(
        !dir.join("knowledge.toml").exists(),
        "不生成缺失的默认包成员（尊重现状）"
    );
}

#[test]
fn broken_project_layer_fails_safe_without_seeding() {
    let proj = TempDir::new("broken");
    let dir = proj.path().join(".crush-tether");
    std::fs::create_dir_all(&dir).unwrap();
    let broken = "version = 1\nalow = [\"ls\"]";
    std::fs::write(dir.join("rules.toml"), broken).unwrap();
    let before = std::fs::read(dir.join("rules.toml")).unwrap();

    let r = run_check(proj.path(), "ls");
    assert_eq!(r.code, 0, "损坏 → fail-safe confirm（静默 exit 0）");
    assert!(r.stderr.contains("fail-safe confirm"), "got: {}", r.stderr);
    assert_eq!(
        std::fs::read(dir.join("rules.toml")).unwrap(),
        before,
        "损坏文件原样保留（D-03：不生成、不留档、不动原文件）"
    );
    assert!(!dir.join("knowledge.toml").exists(), "损坏不触发任何生成");
}

#[test]
fn broken_global_layer_fails_safe_end_to_end() {
    // 全局层同受 D-03 约束：坏全局层 → 整体 Err → fail-safe confirm，
    // 绝不带坏层静默裁决。
    let proj = TempDir::new("broken-sys");
    let sys_dir = TempDir::new("broken-sys-dir");
    std::fs::write(sys_dir.path().join("rules.toml"), "version = 1\nalow = []").unwrap();
    let gp = sys_dir.path().to_string_lossy().into_owned();
    let r = run_check_env(
        proj.path(),
        &[],
        "ls",
        &[("CRUSH_TETHER_GLOBAL_DIR", gp.as_str())],
    );
    assert_eq!(r.code, 0, "损坏 → fail-safe confirm（静默 exit 0）");
    assert!(r.stderr.contains("fail-safe confirm"), "got: {}", r.stderr);
}
