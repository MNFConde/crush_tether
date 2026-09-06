//! 验收（M4.3）：裁决日志落盘——JSONL 字段与 design.md 示例一致；
//! `type:"load"` 事件行冷/热路径留痕；日志开关（ADR-07 默认开）。

mod common;

use common::{TempDir, run_mode_env};

fn log_of(project: &std::path::Path) -> Vec<serde_json::Value> {
    let path = project.join(".crush-tether").join("decisions.jsonl");
    let raw = std::fs::read_to_string(path).expect("decisions.jsonl exists");
    raw.lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).expect("each line is JSON"))
        .collect()
}

#[test]
fn verdict_log_fields_match_design_example() {
    let proj = TempDir::new("m43-log");
    let cfg = proj.path().join(".crush-tether");
    std::fs::create_dir_all(&cfg).expect("mkdir");
    std::fs::write(
        cfg.join("rules.toml"),
        "version = 1\ndefault = \"confirm\"\n[local]\nallow = [\"ls\"]\ndeny = [\"sudo\"]\n",
    )
    .expect("write rules");
    // 知识库在位：kb = ["main"]。
    std::fs::write(cfg.join("knowledge.toml"), "version = 1\n").expect("write kb");

    let r = run_mode_env(proj.path(), "check", &[], "ls", &[]);
    assert_eq!(r.code, 0);
    let lines = log_of(proj.path());
    let v = lines
        .iter()
        .find(|v| v["decision"] == "allow")
        .expect("allow verdict logged");
    // design.md 示例字段全集。
    for key in [
        "ts",
        "mode",
        "agent",
        "command",
        "decision",
        "reason",
        "source",
        "kb",
        "normalized",
        "script",
    ] {
        assert!(v.get(key).is_some(), "missing `{key}` in {v}");
    }
    assert_eq!(v["mode"], "check");
    assert_eq!(v["agent"], "crush");
    assert_eq!(v["command"], "ls");
    assert_eq!(v["source"]["layer"], "project", "{v}");
    assert_eq!(v["source"]["entry"], "allow");
    assert_eq!(v["source"]["match"], "ls");
    assert_eq!(v["source"]["file"], ".crush-tether/rules.toml");
    assert_eq!(v["kb"], serde_json::json!(["main"]));
    assert_eq!(v["normalized"], serde_json::Value::Null);

    // deny 命中头部裸列表：entry = <bucket>，match = bin。
    let r = run_mode_env(proj.path(), "check", &[], "sudo x", &[]);
    assert_eq!(r.code, 2, "deny → exit 2");
    let lines = log_of(proj.path());
    let v = lines
        .iter()
        .find(|v| v["decision"] == "deny")
        .expect("deny verdict logged");
    assert_eq!(v["source"]["entry"], "deny", "{v}");
    assert_eq!(v["source"]["match"], "sudo");
    // 命令节命中则为 <bin>.<bucket>.<dim>（见 script_allow 相关用例）。
}

#[test]
fn load_event_logged_on_serve_start_and_hot_reload() {
    use std::time::{Duration, Instant};

    use common::spawn_serve;

    let proj = TempDir::new("m43-load");
    let cfg = proj.path().join(".crush-tether");
    std::fs::create_dir_all(&cfg).expect("mkdir");
    std::fs::write(
        cfg.join("rules.toml"),
        "version = 1\ndefault = \"confirm\"\n[local]\nallow = [\"ls\"]\n",
    )
    .expect("write rules");
    std::fs::write(cfg.join("knowledge.toml"), "version = 1\n").expect("write kb");

    // 驻留 serve（60s idle），在同一实例生命周期内验证冷/热两条 load 事件行。
    let _serve = spawn_serve(proj.path(), "60");

    // 冷启动：serve 首次加载留 load 事件行（hook 首请求等就绪 + 出裁决）。
    let r = run_mode_env(proj.path(), "hook", &[], "ls", &[]);
    assert_eq!(r.code, 0);
    let load = log_of(proj.path())
        .into_iter()
        .find(|v| v["type"] == "load")
        .expect("冷启动 load 事件");
    assert!(load.get("lint").is_some(), "{load}");
    assert_eq!(load["kb"], serde_json::json!(["main"]));

    // 热重载：改规则 → deny；重载在 serve 主线程请求间隙消费信号，load
    // 事件行先于该请求应答落盘（冷热路径都留痕，D-07）。
    std::fs::write(
        cfg.join("rules.toml"),
        "version = 1\ndefault = \"confirm\"\n[local]\ndeny = [\"ls\"]\n",
    )
    .expect("write new rules");
    // watcher debounce（600ms）窗口内的请求仍用旧快照：轮询到新规则生效为止。
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let r = run_mode_env(proj.path(), "hook", &[], "ls", &[]);
        if r.code == 2 {
            break;
        }
        assert!(Instant::now() < deadline, "重载后新规则应生效 → deny");
        std::thread::sleep(Duration::from_millis(200));
    }
    let loads = log_of(proj.path())
        .iter()
        .filter(|v| v["type"] == "load")
        .count();
    assert!(loads >= 2, "热重载留第二条 load 事件：{loads}");
}

#[test]
fn log_can_be_disabled_via_env() {
    let proj = TempDir::new("m43-off");
    let cfg = proj.path().join(".crush-tether");
    std::fs::create_dir_all(&cfg).expect("mkdir");
    std::fs::write(
        cfg.join("rules.toml"),
        "version = 1\ndefault = \"confirm\"\n[local]\nallow = [\"ls\"]\n",
    )
    .expect("write rules");
    let r = run_mode_env(
        proj.path(),
        "check",
        &[],
        "ls",
        &[("CRUSH_TETHER_LOG", "off")],
    );
    assert_eq!(r.code, 0);
    assert!(
        !proj
            .path()
            .join(".crush-tether")
            .join("decisions.jsonl")
            .exists(),
        "关闭后不落盘"
    );
}

#[test]
fn explicit_config_layer_traced_in_source() {
    let proj = TempDir::new("m43-explicit");
    // 日志落盘目录（显式覆盖跳过引导生成，需自备目录）。
    std::fs::create_dir_all(proj.path().join(".crush-tether")).expect("mkdir");
    // 显式覆盖文件走 --config：source.layer 应为 explicit，file = 显式路径。
    let ext = proj.path().join("override-rules.toml");
    std::fs::write(
        &ext,
        "version = 1\ndefault = \"confirm\"\n[local]\nallow = [\"ls\"]\n",
    )
    .expect("write override rules");
    let ext_str = ext.to_string_lossy().into_owned();
    let r = run_mode_env(proj.path(), "check", &["--config", &ext_str], "ls", &[]);
    assert_eq!(r.code, 0);
    let v = log_of(proj.path())
        .into_iter()
        .find(|v| v["decision"] == "allow")
        .expect("allow verdict logged");
    assert_eq!(v["source"]["layer"], "explicit", "{v}");
    assert_eq!(v["source"]["file"], ext_str, "{v}");
    assert_eq!(v["source"]["entry"], "allow", "{v}");
}

#[test]
fn script_overridden_verdict_traces_to_script_layer() {
    let proj = TempDir::new("m43-script");
    let cfg = proj.path().join(".crush-tether");
    std::fs::create_dir_all(&cfg).expect("mkdir");
    std::fs::write(
        cfg.join("rules.toml"),
        "version = 1\ndefault = \"confirm\"\n[local]\nallow = [\"ls\"]\n",
    )
    .expect("write rules");
    // 脚本把 ls 改判 deny（上调合法）：生效裁决出自脚本 → layer=script。
    std::fs::write(
        cfg.join("rules.rhai"),
        "fn check(ctx) {\n    if ctx.bin == \"ls\" { return decision::DENY; }\n    decision::PASS\n}\n",
    )
    .expect("write rules.rhai");
    let r = run_mode_env(proj.path(), "check", &[], "ls", &[]);
    assert_eq!(r.code, 2, "脚本改判 deny → exit 2");
    let v = log_of(proj.path())
        .into_iter()
        .find(|v| v["decision"] == "deny")
        .expect("deny verdict logged");
    assert_eq!(v["source"]["layer"], "script", "{v}");
    assert_eq!(v["source"]["file"], "rules.rhai", "{v}");
    assert_eq!(v["source"]["entry"], "script", "{v}");
    assert_eq!(v["script"]["file"], "rules.rhai", "{v}");
}
