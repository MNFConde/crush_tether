// crush-tether 默认脚本层（rules.rhai）
//
// 契约（v1 定稿）：脚本只上调、不放行——返回 ""（无意见，保留查表裁决）、
// "confirm" 或 "deny"；返回 "allow" 会被引擎以契约违约拒绝（fail-safe
// confirm）。无条件兜底因此被结构性禁止；本默认脚本亦不含任何命令枚举放行。
//
// 本文件是数据文件，在 crush-tether 二进制内的沙箱执行（限流 +
// 无 IO API）；可用原语见 doc/design.md「DSL 引擎（定稿）」。

fn check(ctx) {
    // ── 1) 两态子命令：数据读知识库（write_tokens / write_arg_count）──
    // 知识库在位时按数据细化（未覆盖的 bin/sub 不升级）；
    // 知识库整体删光（kb_present 失效）→ 两态判定无法进行 → 有子命令的
    // allow 一律 confirm 兜底（查表层不受影响）。
    if ctx.sub != () && ctx.verdict == "allow" {
        if !kb_present() { return "confirm"; }
        for t in kb_write_tokens(ctx.bin, ctx.sub) {
            if ctx.args.contains(t) { return "confirm"; }
        }
        let n = kb_write_arg_count(ctx.bin, ctx.sub);
        if n > 0 && positional_count(ctx) >= n { return "confirm"; }
    }

    // ── 2) find 突变参数：-delete / -exec 族可绕过 rm 门 ──
    if ctx.bin == "find" {
        for w in ctx.args {
            if w.starts_with("-delete") || w == "-exec" || w == "-execdir"
                || w == "-ok" || w == "-okdir" { return "confirm"; }
        }
    }

    // ── 3) 管道 sink 与参数内管道：curl|sh 类 → deny ──
    // 管道拓扑由引擎原语计算（ctx.pipe_to_shell），脚本只承载策略；
    // curl/wget 参数含 | 同样按管道 sink 处理（design.md 四类谓词之参数内容检查）。
    if ctx.pipe_to_shell { return "deny"; }
    if ctx.bin == "curl" || ctx.bin == "wget" {
        for w in ctx.args {
            if w.contains("|") { return "deny"; }
        }
    }

    // ── 4) 写特征升级：查表放行 + 写重定向 → 降为 confirm ──
    if ctx.verdict == "allow" && ctx.writes_redirect { return "confirm"; }

    ""
}

// 位置参数计数：args[0] 是子命令本身，不计；以 - 开头的词元是 flag，不计。
fn positional_count(ctx) {
    let n = 0;
    for i in 1..ctx.args.len() {
        if !ctx.args[i].starts_with("-") { n += 1; }
    }
    n
}
