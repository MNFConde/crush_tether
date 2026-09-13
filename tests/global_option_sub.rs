//! M9.1 验收：前置全局选项不再顶掉子命令槽——查表、两态判定、flag 桶
//! 三面的端到端断言（默认包 fixture 双引擎 + 自定义工程脚本 + kb 登记）。

mod common;
mod fixture;

use common::{TempDir, run_check};
use crush_tether::model::Decision;
use fixture::{decide, decide_lua};

#[test]
fn global_option_value_skipped_sub_recognized() {
    // kb 登记 -C takes_value：值词元跳过，status/push 正确识别为子命令。
    assert_eq!(decide("git -C D:/x status --short"), Decision::Allow);
    assert_eq!(decide("git --no-pager log -n 3"), Decision::Allow);
    assert_eq!(decide("git -C D:/x push origin main"), Decision::Deny);
}

#[test]
fn leading_value_flag_hits_confirm_bucket() {
    // -c 在 confirm.flag：M9.1 起前导 flag 真正进入 flag 查表（旧规则下
    // -c 被吞进 sub 槽，永远打不中）——allow.sub + confirm.flag 合成 confirm。
    assert_eq!(decide("git -c core.autocrlf=false log"), Decision::Confirm);
}

#[test]
fn two_state_sees_corrected_sub() {
    // config 写形态（≥2 位置参数）经修正 sub 生效；单参数读形态不升级。
    assert_eq!(decide("git -C D:/x config a b"), Decision::Confirm);
    assert_eq!(decide("git -C D:/x config user.name"), Decision::Allow);
}

#[test]
fn unregistered_value_flag_falls_to_default_fail_safe() {
    // kb 未登记 -X：值词元被当子命令 → default confirm（解析改动不误放）。
    assert_eq!(decide("git -X D:/x status"), Decision::Confirm);
}

#[test]
fn lua_engine_matches_rhai_on_global_option_forms() {
    // 双模板在新增形态上等价（positional_count/kb_takes_value 双侧同语义）。
    for c in [
        "git -C D:/x status --short",
        "git -C D:/x config a b",
        "git -C D:/x config user.name",
        "git --no-pager log",
        "git -c k=v log",
    ] {
        assert_eq!(decide(c), decide_lua(c), "rhai/lua 等价: {c}");
    }
}

#[test]
fn ctx_sub_reaches_script_corrected() {
    // 自定义脚本断言 ctx.sub 看到的是修正后的子命令（而非 -C）。
    let proj = TempDir::new("ctxsub");
    let cfg = proj.path().join(".crush-tether");
    std::fs::create_dir_all(&cfg).expect("create cfg");
    std::fs::write(
        cfg.join("rules.toml"),
        concat!(
            "version = 1\n",
            "default = \"confirm\"\n",
            "[local]\n",
            "allow = []\n",
            "[local.git]\n",
            "allow.sub = [\"status\"]\n",
        ),
    )
    .expect("write rules.toml");
    std::fs::write(
        cfg.join("rules.rhai"),
        "rule(\"probe\", 10, |ctx| if ctx.sub == \"status\" { decision::DENY } else { decision::PASS });",
    )
    .expect("write rules.rhai");
    std::fs::write(
        cfg.join("knowledge.toml"),
        "version = 1\n[git]\nflag.\"-C\" = { takes_value = true }\n",
    )
    .expect("write knowledge.toml");
    let r = run_check(proj.path(), "git -C D:/x status");
    assert_eq!(r.code, 2, "脚本看到修正后的 ctx.sub=status → DENY 生效");
}
