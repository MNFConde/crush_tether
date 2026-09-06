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
    let proj = TempDir::new("m43-load");
    let cfg = proj.path().join(".crush-tether");
    std::fs::create_dir_all(&cfg).expect("mkdir");
    std::fs::write(
        cfg.join("rules.toml"),
        "version = 1\ndefault = \"confirm\"\n[local]\nallow = [\"ls\"]\n",
    )
    .expect("write rules");
    std::fs::write(cfg.join("knowledge.toml"), "version = 1\n").expect("write kb");
    // serve 冷启动（hook 触发 spawn）→ load 事件行。
    let r = run_mode_env(
        proj.path(),
        "hook",
        &[],
        "ls",
        &[("CRUSH_TETHER_IDLE_EXIT", "2")],
    );
    assert_eq!(r.code, 0);
    let lines = log_of(proj.path());
    let load = lines
        .iter()
        .find(|v| v["type"] == "load")
        .expect("load event");
    assert!(load.get("lint").is_some(), "{load}");
    assert_eq!(load["kb"], serde_json::json!(["main"]));

    // 热重载 → 下一请求触发主线程消费重载信号：第二条 load 事件行 + 新规则生效。
    std::fs::write(
        cfg.join("rules.toml"),
        "version = 1\ndefault = \"confirm\"\n[local]\ndeny = [\"ls\"]\n",
    )
    .expect("write new rules");
    std::thread::sleep(std::time::Duration::from_millis(1500));
    let r = run_mode_env(proj.path(), "hook", &[], "ls", &[]);
    assert_eq!(r.code, 2, "重载后新规则生效 → deny");
    let lines = log_of(proj.path());
    let loads = lines.iter().filter(|v| v["type"] == "load").count();
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
