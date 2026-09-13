//! M9.2 验收：cd 放行 + 段级 cwd 基准——行内 cd 切换写目标解析基准；
//! 项目外/不可解析基准下的写效果被逃逸检查拦截（宁多拦不漏放）。

mod fixture;

use crush_tether::model::Decision;
use fixture::{decide, decide_lua};

// fixture PROJECT = "D:/Code/RustCodeProject/mdor"（词法基准，不要求存在）。

#[test]
fn cd_inside_project_then_write_inside_allows() {
    // 相对 cd 目标 join 项目根 → 基准仍在项目内 → 项目内写放行。
    assert_eq!(decide("cd sub && touch x"), Decision::Allow);
    assert_eq!(
        decide("cd D:/Code/RustCodeProject/mdor && git status"),
        Decision::Allow
    );
}

#[test]
fn cd_outside_then_relative_write_confirms() {
    // 项目外基准下的相对写目标 = 写项目外 → confirm（naive allow cd 的洞）。
    assert_eq!(decide("cd /tmp && touch x"), Decision::Confirm);
    assert_eq!(decide("cd .. && touch x"), Decision::Confirm);
}

#[test]
fn unresolvable_cd_target_poisons_following_writes() {
    // $VAR / `cd -`：展开值与 OLDPWD 静态不可知 → 毒化，其后写效果保守 confirm。
    assert_eq!(decide("cd $DIR && touch x"), Decision::Confirm);
    assert_eq!(decide("cd - && touch x"), Decision::Confirm);
}

#[test]
fn cd_alone_and_reads_are_unaffected() {
    // cd 本身放行；读类不受基准影响（读路径豁免逃逸检查）。
    assert_eq!(decide("cd /tmp"), Decision::Allow);
    assert_eq!(decide("cd /tmp && ls"), Decision::Allow);
    assert_eq!(decide("cd /tmp && cat /etc/hosts"), Decision::Allow);
}

#[test]
fn subshell_cd_does_not_leak_out() {
    // 子 shell 内 cd 出项目：出组弹栈，其后写目标回项目内基准 → allow。
    assert_eq!(decide("(cd /tmp); touch x"), Decision::Allow);
    // 非子 shell 的 `;` 分隔：cd 持续生效 → 写目标在项目外 → confirm。
    assert_eq!(decide("cd /tmp; touch x"), Decision::Confirm);
}

#[test]
fn command_substitution_inner_commands_are_judged() {
    // M9.2 旁路修复：$( ) 内层命令原先被整体丢弃，现在入列裁决。
    assert_eq!(decide("echo $(sudo rm x)"), Decision::Deny);
}

#[test]
fn cd_then_confirm_bucket_still_confirms() {
    // 段级基准只影响逃逸检查，不改动桶判定：rm 仍在 confirm 桶。
    assert_eq!(decide("cd /tmp && rm x"), Decision::Confirm);
}

#[test]
fn lua_engine_matches_rhai_on_cd_forms() {
    // 双模板对 cd 形态等价（基准逻辑在引擎层，模板仅承载桶表）。
    for c in [
        "cd sub && touch x",
        "cd /tmp && touch x",
        "cd $DIR && touch x",
        "(cd /tmp); touch x",
        "cd /tmp; touch x",
    ] {
        assert_eq!(decide(c), decide_lua(c), "rhai/lua 等价: {c}");
    }
}
