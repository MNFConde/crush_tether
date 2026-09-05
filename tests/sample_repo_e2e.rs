//! 验收（M2.7）：样例仓库端到端——草案推荐值在默认包下生效展示 + 自定义
//! 规则（覆盖 / 增删两种写法）改变裁决全链路（发现 → 合并 → 归一 → 查表）。

mod common;

use common::{TempDir, run_check, run_check_env};

/// 三个「可改回」项按草案推荐值在默认包（引导生成）下生效展示：
/// `go run` 落 confirm、`git reset` 取 confirm 档（--hard 升 deny）、
/// `-h` 保留 confirm.flag（更正登记 5）。
#[test]
fn default_package_recommended_values_in_effect() {
    let proj = TempDir::new("m27-defaults"); // 空仓库 → 首跑引导默认包
    let cases: [(&str, i32, &str); 7] = [
        ("git status", 0, "{\"decision\":\"allow\"}"),
        // -h 保留 confirm.flag：status 命中 allow.sub，-h 命中 confirm.flag → 合成 confirm
        ("git status -h", 0, ""),
        // git reset 取 confirm 档（软/mixed 走确认）
        ("git reset", 0, ""),
        // --hard 独入 deny.flag：confirm.sub + deny.flag 双命中按 precedence 合成 deny
        ("git reset --hard", 2, ""),
        ("git push", 2, ""),
        // go run 执行任意代码不入 allow → 落顶层 default confirm
        ("go run x", 0, ""),
        ("cargo clippy", 0, "{\"decision\":\"allow\"}"),
    ];
    for (cmd, want_code, want_stdout) in cases {
        let r = run_check(proj.path(), cmd);
        assert_eq!(r.code, want_code, "{cmd}: stderr={}", r.stderr);
        assert_eq!(r.stdout.trim(), want_stdout, "{cmd}");
    }
    assert!(proj.path().join(".crush-tether/rules.toml").is_file());
}

/// 覆盖写法（数组 = 整表覆盖）：样例仓库把 `git status` 从默认包的 allow
/// 改写为 deny，并验证自定义包整体生效。
#[test]
fn custom_override_rule_changes_verdict() {
    let proj = TempDir::new("m27-override");
    let dir = proj.path().join(".crush-tether");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("rules.toml"),
        concat!(
            "version = 1\n",
            "default = \"confirm\"\n",
            "[local]\n",
            "allow = [\"ls\"]\n",
            "[local.git]\n",
            "deny.sub = [\"status\"]\n",
        ),
    )
    .unwrap();

    let r = run_check(proj.path(), "git status");
    assert_eq!(r.code, 2, "自定义 deny.sub 覆盖默认包的 allow 语义");
    let r = run_check(proj.path(), "ls");
    assert_eq!(
        r.stdout.trim(),
        "{\"decision\":\"allow\"}",
        "同包其余规则照常"
    );
}

/// 增删写法（inline table = 继承低层并增删）：用户层配置 `-h` confirm，
/// 项目层 `{ remove = ["-h"] }` 剔除之——`git status -h` 由 confirm 变 allow；
/// 项目层 `{ add = ["wget"] }` 反向追加 confirm 桶。
#[test]
fn custom_delta_rule_across_layers_changes_verdict() {
    let proj = TempDir::new("m27-delta-proj");
    let user = TempDir::new("m27-delta-user");

    // 用户层（父类）
    let user_cfg = user.path().join(".config").join("crush-tether");
    std::fs::create_dir_all(&user_cfg).unwrap();
    std::fs::write(
        user_cfg.join("rules.toml"),
        concat!(
            "version = 1\n",
            "default = \"confirm\"\n",
            "[local]\n",
            "allow = [\"ls\"]\n",
            "confirm = [\"curl\"]\n",
            "[local.git]\n",
            "allow.sub = [\"status\"]\n",
            "confirm.flag = [\"-h\"]\n",
        ),
    )
    .unwrap();

    // 项目层（子类）：只写增删，其余继承用户层
    let dir = proj.path().join(".crush-tether");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("rules.toml"),
        concat!(
            "version = 1\n",
            "[local]\n",
            "confirm = { add = [\"wget\"] }\n",
            "[local.git]\n",
            "confirm.flag = { remove = [\"-h\"] }\n",
        ),
    )
    .unwrap();

    let home = user.path().to_str().expect("utf-8");
    let envs = [("USERPROFILE", home), ("HOME", home)];

    // 关键对照：若无跨层合并（或 remove 未生效），此命令为 confirm 静默；
    // 剔除生效后合成只剩 allow.sub → allow JSON。
    let r = run_check_env(proj.path(), &[], "git status -h", &envs);
    assert_eq!(
        r.stdout.trim(),
        "{\"decision\":\"allow\"}",
        "项目层 remove 剔除用户层 -h confirm；stderr={}",
        r.stderr
    );

    // 继承不受项目层 delta 影响：curl 仍命中用户层 confirm。
    let r = run_check_env(proj.path(), &[], "curl", &envs);
    assert_eq!(r.code, 0);
    assert!(r.stdout.trim().is_empty(), "curl → confirm 静默");

    // 项目层 add 反向追加：wget 进入 confirm 桶 → 静默（非 allow）。
    let r = run_check_env(proj.path(), &[], "wget", &envs);
    assert_eq!(r.code, 0);
    assert!(r.stdout.trim().is_empty(), "wget 经项目层 add 进入 confirm");
}
