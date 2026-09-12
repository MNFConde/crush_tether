-- crush-tether 默认脚本层（rules.lua）——声明式规则函数形态（M8.6）
--
-- 契约（v1 定稿）：脚本只上调、不放行——规则函数返回 decision.PASS（无
-- 意见，交下一个规则）、decision.CONFIRM 或 decision.DENY；返回
-- decision.ALLOW 会被引擎以契约违约拒绝（fail-safe confirm）。无条件兜底
-- 因此被结构性禁止；本默认脚本亦不含任何命令枚举放行。返回 nil 与
-- decision.PASS 等价；决策值常量见全局 decision 表；ctx.sub 无子命令时为 ""。
--
-- 书写形态（双形态并存，见 doc/design.md「声明式规则函数」）：
-- - `rule(名字, 优先级, 函数)`：引擎注入的注册器——加载期收集，按优先级
--   升序组装（数值小先执行，同值按注册顺序 = 定义顺序），运行时逐个调用、
--   表态即短路。每个具名规则在 decisions.jsonl `script.rule` 与 explain
--   中可追溯。
-- - 旧形态 `function check(ctx)`：整链单入口，仍被兼容执行（文件内无
--   rule() 注册时生效）。
--
-- 数据驱动的细分分支可返回 `confirm_as(子名)`：固定映射 confirm，溯源名
-- 拼为 `规则名:子名`（如 two_state:-d——本会话批准的粒度可细到 token）。
--
-- 可用原语见 doc/design.md「DSL 引擎（定稿）」；本文件是数据文件，在
-- crush-tether 二进制内的沙箱执行（限流 + 库白名单 + 无 IO API）。

-- 位置参数计数：args[1] 是子命令本身，不计；以 - 开头的词元是 flag，不计。
local function positional_count(ctx)
    local n = 0
    for i = 2, #ctx.args do
        if ctx.args[i]:sub(1, 1) ~= "-" then
            n = n + 1
        end
    end
    return n
end

-- ── pipe_sink（优先级 10，最先执行）：管道 sink 与参数内管道 → deny ──
-- 管道拓扑由引擎原语计算（ctx.pipe_to_shell），脚本只承载策略；
-- curl/wget 参数含 | 同样按管道 sink 处理（design.md 四类谓词之参数内容检查）。
rule("pipe_sink", 10, function(ctx)
    if ctx.pipe_to_shell then
        return decision.DENY
    end
    if ctx.bin == "curl" or ctx.bin == "wget" then
        for _, w in ipairs(ctx.args) do
            if w:find("|", 1, true) then
                return decision.DENY
            end
        end
    end
    return decision.PASS
end)

-- ── find_mutator（20）：find 的 -delete / -exec 族可绕过 rm 门 ──
rule("find_mutator", 20, function(ctx)
    if ctx.bin == "find" then
        for _, w in ipairs(ctx.args) do
            if w:sub(1, 7) == "-delete" or w == "-exec" or w == "-execdir"
                or w == "-ok" or w == "-okdir" then
                return decision.CONFIRM
            end
        end
    end
    return decision.PASS
end)

-- ── two_state（30）：两态子命令，数据读知识库（write_tokens /
-- write_arg_count）──知识库在位时按数据细化（未覆盖的 bin/sub 不升级）；
-- 知识库整体删光（kb_present 失效）→ 两态判定无法进行 → 有子命令的
-- allow 一律 confirm 兜底（查表层不受影响）。命中写特征 token 时以
-- confirm_as(token) 上报子名（粒度细到 token：批准 `-d` 不放行 `-D`）。
rule("two_state", 30, function(ctx)
    if ctx.sub ~= "" and ctx.verdict == decision.ALLOW then
        if not kb_present() then
            return decision.CONFIRM
        end
        for _, t in ipairs(kb_write_tokens(ctx.bin, ctx.sub)) do
            for _, a in ipairs(ctx.args) do
                if a == t then
                    return confirm_as(t)
                end
            end
        end
        local n = kb_write_arg_count(ctx.bin, ctx.sub)
        if n > 0 and positional_count(ctx) >= n then
            return decision.CONFIRM
        end
    end
    return decision.PASS
end)

-- ── write_redirect（40，最后执行）：查表放行 + 写重定向 → 降为 confirm ──
rule("write_redirect", 40, function(ctx)
    if ctx.verdict == decision.ALLOW and ctx.writes_redirect then
        return decision.CONFIRM
    end
    return decision.PASS
end)
