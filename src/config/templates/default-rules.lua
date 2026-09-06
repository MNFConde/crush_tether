-- crush-tether 默认脚本层（rules.lua）
--
-- 契约（v1 定稿）：脚本只上调、不放行——返回 decision.PASS（无意见，保留
-- 查表裁决）、decision.CONFIRM 或 decision.DENY；返回 decision.ALLOW 会被
-- 引擎以契约违约拒绝（fail-safe confirm）。无条件兜底因此被结构性禁止；
-- 本默认脚本亦不含任何命令枚举放行。返回 nil 与 decision.PASS 等价；
-- 决策值常量见全局 decision 表；ctx.sub 无子命令时为 ""。
-- 可用原语见 doc/design.md「DSL 引擎（定稿）」；本文件是数据文件，在
-- crush-tether 二进制内的沙箱执行（限流 + 库白名单 + 无 IO API）。

local function positional_count(ctx)
    -- 位置参数计数：args[1] 是子命令本身，不计；以 - 开头的词元是 flag，不计。
    local n = 0
    for i = 2, #ctx.args do
        if ctx.args[i]:sub(1, 1) ~= "-" then
            n = n + 1
        end
    end
    return n
end

function check(ctx)
    -- ── 1) 两态子命令：数据读知识库（write_tokens / write_arg_count）──
    -- 知识库在位时按数据细化（未覆盖的 bin/sub 不升级）；
    -- 知识库整体删光（kb_present 失效）→ 两态判定无法进行 → 有子命令的
    -- allow 一律 confirm 兜底（查表层不受影响）。
    if ctx.sub ~= "" and ctx.verdict == decision.ALLOW then
        if not kb_present() then
            return decision.CONFIRM
        end
        for _, t in ipairs(kb_write_tokens(ctx.bin, ctx.sub)) do
            for _, a in ipairs(ctx.args) do
                if a == t then
                    return decision.CONFIRM
                end
            end
        end
        local n = kb_write_arg_count(ctx.bin, ctx.sub)
        if n > 0 and positional_count(ctx) >= n then
            return decision.CONFIRM
        end
    end

    -- ── 2) find 突变参数：-delete / -exec 族可绕过 rm 门 ──
    if ctx.bin == "find" then
        for _, w in ipairs(ctx.args) do
            if w:sub(1, 7) == "-delete" or w == "-exec" or w == "-execdir"
                or w == "-ok" or w == "-okdir" then
                return decision.CONFIRM
            end
        end
    end

    -- ── 3) 管道 sink 与参数内管道：curl|sh 类 → deny ──
    -- 管道拓扑由引擎原语计算（ctx.pipe_to_shell），脚本只承载策略；
    -- curl/wget 参数含 | 同样按管道 sink 处理（design.md 四类谓词之参数内容检查）。
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

    -- ── 4) 写特征升级：查表放行 + 写重定向 → 降为 confirm ──
    if ctx.verdict == decision.ALLOW and ctx.writes_redirect then
        return decision.CONFIRM
    end

    return decision.PASS
end
