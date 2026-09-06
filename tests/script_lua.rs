//! M6.1 验收（Lua 引擎）：与 Rhai 同一 RuleEngine trait、限流同等、
//! ctx 封装后词汇约定（`decision.PASS` / nil 等价 / 可选字段空串）在 Lua
//! 侧成立、`--engine lua` 端到端（默认包按引擎生成 rules.lua）。

mod common;

use common::{TempDir, run_mode_env};

fn compile(
    src: &str,
    decls: crush_tether::config::merge::ScriptAllowDecls,
) -> crush_tether::script::LuaEngine {
    crush_tether::script::LuaEngine::compile(
        src,
        std::path::PathBuf::from("D:/code/tmp/proj"),
        None,
        decls,
    )
    .expect("compiles")
}

fn cmd(s: &str) -> crush_tether::cmd_parse::SimpleCommand {
    crush_tether::cmd_parse::flatten_commands(s)
        .expect("parses")
        .into_iter()
        .next()
        .unwrap()
}

const PROJ: &str = "D:/code/tmp/proj";
use crush_tether::model::Decision;
use crush_tether::script::{RuleEngine, ScriptError, ScriptOutcome};
use std::path::Path;

#[test]
fn lua_returns_decision_constants_and_nil_pass() {
    let e = compile(
        concat!(
            "function check(ctx)",
            "  if ctx.bin == \"sudo\" then return decision.DENY end",
            "  if ctx.bin == \"rm\" then return decision.CONFIRM end",
            // 末位 return nil：PASS 的 Lua 侧映射（词汇约定 nil 等价验收点）。
            "  return nil",
            " end",
        ),
        Default::default(),
    );
    assert_eq!(
        e.evaluate(&cmd("sudo x"), Decision::Allow, Path::new(PROJ), false)
            .unwrap(),
        ScriptOutcome::Adjust(Decision::Deny)
    );
    assert_eq!(
        e.evaluate(&cmd("rm x"), Decision::Allow, Path::new(PROJ), false)
            .unwrap(),
        ScriptOutcome::Adjust(Decision::Confirm)
    );
    assert_eq!(
        e.evaluate(&cmd("ls"), Decision::Allow, Path::new(PROJ), false)
            .unwrap(),
        ScriptOutcome::Pass
    );
}

#[test]
fn lua_reads_ctx_fields_and_verdict_equality() {
    // ctx 封装：只读字段（userdata）、verdict 与 decision 常量按变体比较、
    // sub 空串约定、words/args 表访问。
    let e = compile(
        concat!(
            "function check(ctx)",
            "  if ctx.verdict == decision.ALLOW and ctx.writes_redirect then return decision.CONFIRM end",
            "  if ctx.bin == \"git\" and ctx.args[1] == \"push\" then return decision.DENY end",
            "  if ctx.sub == \"\" then return decision.CONFIRM end",
            "  if ctx.words[1] == \"git\" then return decision.PASS end",
            "  return decision.PASS",
            " end",
        ),
        Default::default(),
    );
    assert_eq!(
        e.evaluate(&cmd("ls > o.txt"), Decision::Allow, Path::new(PROJ), false)
            .unwrap(),
        ScriptOutcome::Adjust(Decision::Confirm)
    );
    assert_eq!(
        e.evaluate(&cmd("git push"), Decision::Allow, Path::new(PROJ), false)
            .unwrap(),
        ScriptOutcome::Adjust(Decision::Deny)
    );
    assert_eq!(
        e.evaluate(&cmd("ls"), Decision::Allow, Path::new(PROJ), false)
            .unwrap(),
        ScriptOutcome::Adjust(Decision::Confirm)
    );
    assert_eq!(
        e.evaluate(&cmd("git status"), Decision::Allow, Path::new(PROJ), false)
            .unwrap(),
        ScriptOutcome::Pass
    );
}

#[test]
fn lua_contract_violations() {
    // 裸 allow（decision.ALLOW 或字符串）→ 契约违约；非法字符串 → 违约。
    for src in [
        "function check(ctx) return decision.ALLOW end",
        "function check(ctx) return \"allow\" end",
        "function check(ctx) return \"maybe\" end",
        "function check(ctx) return 42 end",
    ] {
        let e = compile(src, Default::default());
        assert!(
            matches!(
                e.evaluate(&cmd("ls"), Decision::Allow, Path::new(PROJ), false),
                Err(ScriptError::Contract(_))
            ),
            "{src}"
        );
    }
}

#[test]
fn lua_missing_check_is_rejected_at_compile() {
    let r = crush_tether::script::LuaEngine::compile(
        "x = 1",
        std::path::PathBuf::from(PROJ),
        None,
        Default::default(),
    );
    assert!(matches!(r, Err(ScriptError::Compile(_))));
}

#[test]
fn lua_infinite_loop_is_bounded_by_instruction_hook() {
    let e = compile(
        "function check(ctx) while true do end end",
        Default::default(),
    );
    assert!(
        e.evaluate(&cmd("ls"), Decision::Allow, Path::new(PROJ), false)
            .is_err(),
        "指令数限流必须把死循环变成 Err → 上层 confirm"
    );
}

#[test]
fn lua_coroutine_loop_is_bounded_by_global_hook() {
    // 协程内死循环同样受指令预算约束（全局 hook 覆盖脚本自建协程；线程级
    // hook 只挂主线程会逃逸）。判定用副作用标记而非墙钟：循环被终止 →
    // done 未置位 → DENY；若协程逃逸限流 → 循环跑完 → CONFIRM。
    // 语义边界：coroutine.resume 类 pcall 吞协程内错误——脚本不报错、
    // 正常走到返回值（design.md 更正登记 18）。
    let e = compile(
        concat!(
            "function check(ctx)\n",
            "  local done = false\n",
            "  local co = coroutine.create(function()\n",
            "    for i = 1, 2000000 do end\n",
            "    done = true\n",
            "  end)\n",
            "  coroutine.resume(co)\n",
            "  if done then return decision.CONFIRM end\n",
            "  return decision.DENY\n",
            "end\n",
        ),
        Default::default(),
    );
    assert_eq!(
        e.evaluate(&cmd("ls"), Decision::Allow, Path::new(PROJ), false)
            .unwrap(),
        ScriptOutcome::Adjust(Decision::Deny),
        "协程死循环必须在指令预算内被终止（done 不置位）"
    );
}

#[test]
fn lua_deep_recursion_is_bounded() {
    let e = compile(
        "local function f(x) return f(x) end function check(ctx) return f(1) end",
        Default::default(),
    );
    assert!(
        e.evaluate(&cmd("ls"), Decision::Allow, Path::new(PROJ), false)
            .is_err()
    );
}

#[test]
fn lua_sandbox_has_no_host_api() {
    // 库白名单：io/os/package/require 不可达；base 危险全局已消毒。
    for src in [
        "function check(ctx) io.open(\"x\") return nil end",
        "function check(ctx) os.execute(\"x\") return nil end",
        "function check(ctx) dofile(\"x\") return nil end",
        "function check(ctx) loadfile(\"x\") return nil end",
        "function check(ctx) load(\"x\") return nil end",
        "function check(ctx) print(\"hi\") return nil end",
    ] {
        let e = compile(src, Default::default());
        assert!(
            matches!(
                e.evaluate(&cmd("ls"), Decision::Allow, Path::new(PROJ), false),
                Err(ScriptError::Runtime(_))
            ),
            "{src}"
        );
    }
}

#[test]
fn lua_kb_primitives_compose() {
    use std::sync::Arc;
    let kb_src = concat!(
        "version = 1\n",
        "[git]\n",
        "sub.branch = { write_tokens = [\"-d\", \"-D\"] }\n",
        "sub.config = { write_arg_count = 2 }\n",
    );
    let kb = Arc::new(crush_tether::knowledge::KnowledgeBase::parse_toml(kb_src).expect("kb"));
    let e = crush_tether::script::LuaEngine::compile(
        concat!(
            "function check(ctx)",
            "  if ctx.bin == \"git\" and ctx.args[1] == \"branch\" then",
            "    for _, t in ipairs(kb_write_tokens(\"git\", \"branch\")) do",
            "      for _, a in ipairs(ctx.args) do if a == t then return decision.CONFIRM end end",
            "    end",
            "  end",
            "  local n = kb_write_arg_count(\"git\", \"config\")",
            "  if ctx.bin == \"git\" and ctx.args[1] == \"config\" and n > 0 and #ctx.args - 1 >= n then return decision.CONFIRM end",
            "  return decision.PASS",
            " end",
        ),
        std::path::PathBuf::from(PROJ),
        Some(kb),
        Default::default(),
    )
    .expect("compiles");
    assert_eq!(
        e.evaluate(
            &cmd("git branch -d x"),
            Decision::Allow,
            Path::new(PROJ),
            false
        )
        .unwrap(),
        ScriptOutcome::Adjust(Decision::Confirm)
    );
    assert_eq!(
        e.evaluate(&cmd("git branch"), Decision::Allow, Path::new(PROJ), false)
            .unwrap(),
        ScriptOutcome::Pass
    );
    assert_eq!(
        e.evaluate(
            &cmd("git config a b"),
            Decision::Allow,
            Path::new(PROJ),
            false
        )
        .unwrap(),
        ScriptOutcome::Adjust(Decision::Confirm)
    );
    assert_eq!(
        e.evaluate(
            &cmd("git config --list"),
            Decision::Allow,
            Path::new(PROJ),
            false
        )
        .unwrap(),
        ScriptOutcome::Pass
    );
}

#[test]
fn lua_engine_flag_runs_end_to_end_with_seeded_lua_pack() {
    // --engine lua 端到端：默认包按引擎生成 rules.lua（而非 rules.rhai），
    // 脚本层谓词生效（find -delete 升 confirm）。
    let proj = TempDir::new("lua-e2e");
    let r = run_mode_env(
        proj.path(),
        "check",
        &["--engine", "lua"],
        "find . -delete",
        &[],
    );
    assert_eq!(r.code, 0, "{}", r.stderr);
    assert!(r.stdout.trim().is_empty(), "{}", r.stdout);
    assert!(
        proj.path()
            .join(".crush-tether")
            .join("rules.lua")
            .is_file(),
        "lua 默认包应生成 rules.lua"
    );
    assert!(
        !proj
            .path()
            .join(".crush-tether")
            .join("rules.rhai")
            .exists(),
        "rhai 模板不应随 lua 引擎生成"
    );
    // 引擎切换告警：引擎为 rhai 而 rules.lua 在位 → stderr 提示脚本层谓词
    // 对该请求路径不生效；删除后不再误报。
    let r = run_mode_env(proj.path(), "check", &["--engine", "rhai"], "ls", &[]);
    assert!(
        r.stderr.contains("rules.lua present but engine is `rhai`"),
        "引擎切换告警：{}",
        r.stderr
    );
    std::fs::remove_file(proj.path().join(".crush-tether").join("rules.lua")).unwrap();
    let r = run_mode_env(proj.path(), "check", &["--engine", "rhai"], "ls", &[]);
    assert!(
        !r.stderr.contains("present but engine"),
        "无他引擎脚本时不应告警：{}",
        r.stderr
    );
}
