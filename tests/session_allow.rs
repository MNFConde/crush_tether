//! 验收（M8.6 部分 3）：会话内临时放行——本会话人工批准过的 confirm 命令
//! 免再问，按触发原因匹配（entry 条目粒度 / whole 整命令粒度）；deny 永不
//! 放行；跨会话不串；`CRUSH_TETHER_SESSION_ALLOW=off` 关闭；serve 内存态
//! 与降级态文件形态（serve 不可达）行为一致。
//!
//! 无头定性约束：confirm 无头 = 拒绝（三 agent 已定性），会话放行不在无头
//! 流量生长——本测试直接以 hook 载荷模拟「弹窗批准后执行」的因果（批准 =
//! PostToolUse 事件到达），与交互形态的判定路径同源。

mod common;

use common::{TempDir, run_hook_payload, run_init, spawn_serve};

fn pre_payload(command: &str, session: &str, tool_use: &str) -> String {
    format!(
        r#"{{"hook_event_name":"PreToolUse","session_id":"{session}","tool_use_id":"{tool_use}","tool_input":{{"command":"{command}"}}}}"#
    )
}

fn post_payload(command: &str, session: &str, tool_use: &str) -> String {
    format!(
        r#"{{"hook_event_name":"PostToolUse","session_id":"{session}","tool_use_id":"{tool_use}","tool_input":{{"command":"{command}"}},"tool_response":{{"isError":false}}}}"#
    )
}

fn hook(project: &std::path::Path, payload: &str, envs: &[(&str, &str)]) -> common::CheckRun {
    run_hook_payload(project, payload, envs)
}

fn write_rules(proj: &TempDir, body: &str) {
    std::fs::write(proj.path().join(".crush-tether").join("rules.toml"), body)
        .expect("write rules.toml");
}

/// entry 粒度端到端（serve 内存态）：查表条目触发的 confirm，批准一次后
/// 本会话免问；跨会话不串。
#[test]
fn entry_cause_session_allow_in_serve() {
    let proj = TempDir::new("m86-sa-serve");
    run_init(proj.path(), &[], &[]);
    write_rules(
        &proj,
        "version = 1\ndefault = \"allow\"\n[local.git]\nconfirm.sub = [\"push\"]\n",
    );
    let _serve = spawn_serve(proj.path(), "20", None, &[]);

    // 首次：confirm（crush 静默 exit 0）。
    let r = hook(
        proj.path(),
        &pre_payload("git push origin", "s1", "t1"),
        &[],
    );
    assert_eq!(r.code, 0, "首次应 confirm；stdout={}", r.stdout);
    assert!(
        r.stdout.trim().is_empty(),
        "crush confirm 静默；got {}",
        r.stdout
    );

    // 批准后执行（PostToolUse 到达）。
    let r = hook(
        proj.path(),
        &post_payload("git push origin", "s1", "t1"),
        &[],
    );
    assert_eq!(r.code, 0);

    // 同会话再来：便签命中 → allow。
    let r = hook(
        proj.path(),
        &pre_payload("git push origin", "s1", "t2"),
        &[],
    );
    assert_eq!(r.code, 0);
    assert_eq!(
        r.stdout.trim(),
        "{\"decision\":\"allow\"}",
        "便签命中应放行"
    );

    // 跨会话不串：s2 首次仍 confirm。
    let r = hook(
        proj.path(),
        &pre_payload("git push origin", "s2", "t9"),
        &[],
    );
    assert_eq!(r.code, 0);
    assert!(r.stdout.trim().is_empty(), "跨会话不得串用便签");
}

/// deny 永不被便签救：deny 裁决不参与 pending/便签（红线）。
#[test]
fn deny_is_never_rescued() {
    let proj = TempDir::new("m86-sa-deny");
    run_init(proj.path(), &[], &[]);
    write_rules(
        &proj,
        "version = 1\ndefault = \"allow\"\n[local]\ndeny = [\"sudo\"]\n",
    );
    let _serve = spawn_serve(proj.path(), "20", None, &[]);

    let r = hook(proj.path(), &pre_payload("sudo x", "s1", "t1"), &[]);
    assert_eq!(r.code, 2, "deny = exit 2");
    // 伪对账（即便误发 post）后再问：仍 deny。
    let _ = hook(proj.path(), &post_payload("sudo x", "s1", "t1"), &[]);
    let r = hook(proj.path(), &pre_payload("sudo x", "s1", "t2"), &[]);
    assert_eq!(r.code, 2, "deny 红线不可被会话放行覆盖");
}

/// 开关关闭：`CRUSH_TETHER_SESSION_ALLOW=off` 下始终弹窗（不记 pending、
/// 不查便签）。
#[test]
fn switch_off_keeps_asking_every_time() {
    let proj = TempDir::new("m86-sa-off");
    run_init(proj.path(), &[], &[]);
    write_rules(
        &proj,
        "version = 1\ndefault = \"allow\"\n[local.git]\nconfirm.sub = [\"push\"]\n",
    );
    let envs = [("CRUSH_TETHER_SESSION_ALLOW", "off")];
    let _serve = spawn_serve(proj.path(), "20", None, &envs);

    let r = hook(
        proj.path(),
        &pre_payload("git push origin", "s1", "t1"),
        &envs,
    );
    assert_eq!(r.code, 0);
    assert!(r.stdout.trim().is_empty(), "关闭时首问 confirm");
    let _ = hook(
        proj.path(),
        &post_payload("git push origin", "s1", "t1"),
        &envs,
    );
    let r = hook(
        proj.path(),
        &pre_payload("git push origin", "s1", "t2"),
        &envs,
    );
    assert!(r.stdout.trim().is_empty(), "关闭时便签不生效，仍 confirm");
}

/// 降级态文件形态（serve 不可达）：pending → sticker 落
/// session-cache.jsonl，行为与 serve 内存态一致；跨会话不串。
#[test]
fn degraded_file_form_matches_serve_semantics() {
    let proj = TempDir::new("m86-sa-file");
    run_init(proj.path(), &[], &[]);
    write_rules(
        &proj,
        "version = 1\ndefault = \"allow\"\n[local.git]\nconfirm.sub = [\"push\"]\n",
    );
    let envs = [("CRUSH_TETHER_DISABLE_SERVE", "1")];

    // 首次 confirm → 文件记 pending。
    let r = hook(
        proj.path(),
        &pre_payload("git push origin", "s1", "t1"),
        &envs,
    );
    assert_eq!(r.code, 0);
    assert!(r.stdout.trim().is_empty());
    assert!(
        proj.path()
            .join(".crush-tether")
            .join("session-cache.jsonl")
            .exists(),
        "降级态应落会话缓存文件"
    );

    // PostToolUse → pending 转便签。
    let r = hook(
        proj.path(),
        &post_payload("git push origin", "s1", "t1"),
        &envs,
    );
    assert_eq!(r.code, 0);

    // 同会话再问 → 文件便签命中 → allow。
    let r = hook(
        proj.path(),
        &pre_payload("git push origin", "s1", "t2"),
        &envs,
    );
    assert_eq!(
        r.stdout.trim(),
        "{\"decision\":\"allow\"}",
        "文件便签命中放行"
    );

    // 跨会话不串。
    let r = hook(
        proj.path(),
        &pre_payload("git push origin", "s2", "t9"),
        &envs,
    );
    assert!(r.stdout.trim().is_empty(), "跨会话不得串用文件便签");
}

/// whole 粒度（default 兜底无溯源键）：批的是整条命令——同命令放行、
/// 不同命令照旧弹窗。
#[test]
fn whole_cause_granularity_for_default_confirms() {
    let proj = TempDir::new("m86-sa-whole");
    run_init(proj.path(), &[], &[]);
    write_rules(&proj, "version = 1\ndefault = \"confirm\"\n");
    let _serve = spawn_serve(proj.path(), "20", None, &[]);

    let r = hook(
        proj.path(),
        &pre_payload("frobnicate --deep x", "s1", "t1"),
        &[],
    );
    assert_eq!(r.code, 0);
    assert!(r.stdout.trim().is_empty(), "兜底 confirm");
    let _ = hook(
        proj.path(),
        &post_payload("frobnicate --deep x", "s1", "t1"),
        &[],
    );

    // 同整命令 → 放行（whole 键命中）。
    let r = hook(
        proj.path(),
        &pre_payload("frobnicate --deep x", "s1", "t2"),
        &[],
    );
    assert_eq!(r.stdout.trim(), "{\"decision\":\"allow\"}", "同整命令放行");

    // 不同命令 → 不命中（whole 键是完整命令串），照旧 confirm。
    let r = hook(
        proj.path(),
        &pre_payload("bazqux --other y", "s1", "t3"),
        &[],
    );
    assert!(r.stdout.trim().is_empty(), "不同命令不得被 whole 便签波及");
}

/// default 兜底的命令级隔离：批 `frobnicate --deep x` 不得放行同 bin 的
/// 其他形态（兜底 cause 必须整命令粒度，M8.6 补强用例）。
#[test]
fn whole_cause_is_command_scoped_not_bin_scoped() {
    let proj = TempDir::new("m86-sa-whole2");
    run_init(proj.path(), &[], &[]);
    write_rules(&proj, "version = 1\ndefault = \"confirm\"\n");
    let _serve = spawn_serve(proj.path(), "20", None, &[]);

    let r = hook(
        proj.path(),
        &pre_payload("frobnicate --deep x", "s1", "t1"),
        &[],
    );
    assert_eq!(r.code, 0);
    let _ = hook(
        proj.path(),
        &post_payload("frobnicate --deep x", "s1", "t1"),
        &[],
    );

    // 同 bin 不同参数 → 仍 confirm（不得被 bin 级误放行）。
    let r = hook(
        proj.path(),
        &pre_payload("frobnicate evil", "s1", "t2"),
        &[],
    );
    assert!(
        r.stdout.trim().is_empty(),
        "同 bin 不同参数必须仍弹窗；got {}",
        r.stdout
    );
}
