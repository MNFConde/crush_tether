---
type: project_topic
status: active
summary: M7.3 agent hook 兼容性测试的方法论与过程沉淀：判定准则（hook 执行以探针 dump 物理副作用为准，日志计数与 tool result 回执不可尽信）、mock 三坑（schema 必填字段/工具名环境差异/OpenAI SSE）、灰度可官方 env 钉死（DISABLE_GROWTHBOOK）、crush run 悬案插桩排查全记录（三假设判据/五处插桩/证据代码位置）与三次更正史链条。确定性结论的现役事实在 doc/agent-compat-matrix.md。
tags: [crush_tether, hook, testing, mock, claude-code, crush, methodology]
contains: [lesson, decision, pattern]
created: 2026-09-09
updated: 2026-09-09
related: [doc/agent-compat-matrix.md, doc/design.md, script/mock_llm.py, script/hook_probe.py]
authoring_mode: ai_generated
---

# agent hook 兼容性测试：方法论、教训与排查记录

> 确定性事实的现役版本（矩阵/覆盖口径/配方）在 `doc/agent-compat-matrix.md`；本文沉淀**过程史、更正链条与可复用方法论**。逐日流水见 LOG，里程碑口径见 ROADMAP M7.3 条。

## 三次更正史（链条摘要）

1. **初判（2026-09-08）**「headless（`claude -p`/`crush run`）hooks 整体不加载」——判定依据是 `Registered 0 hooks` 日志计数。
2. **一更（09-09 受控重测）**：claude 侧推翻——headless 为**条件性加载**（GrowthBook 灰度冷↔热翻动，冷窗口静默缺席）；日志计数与执行管线脱节（执行时仍打 0）。crush 侧当时仍判「run 固有不执行」。
3. **二更（09-09 深夜插桩）**：crush 侧推翻——`crush run` **一直正常执行 hooks**，根因是我方 mock 的工具调用缺必填 `description` 被 fantasy 静默拒绝（详见下文排查记录）；「TUI 触发/run 不触发」从头是「真实模型 vs mock」混杂。
4. **三更（09-09 深夜官方文档+实测）**：claude 灰度可用 `DISABLE_GROWTHBOOK=1` 官方钉死——CI 两侧统一为正向硬断言，灰度翻动不再是不稳定源。

教训母题：**误判从不来自测不到，而来自信错了信号**（日志计数、tool result 回执）；唯一可信判据是物理副作用。

## 判定准则（lessons）

- hook 是否执行，以**探针 dump 物理副作用**为唯一判据；`Registered/Found 0 hooks` 计数、`Hook completed` 缺席均不可单独定论。
- mock 驱动下「有 tool result」≠「工具执行过」——参数校验失败、工具名缺席都会静默回 error tool result，无任何日志告警。
- 「对话正常 + exit 0」不等于链路健康：mock 的松回包逻辑（见任意 tool result 即回包）会把失败伪装成成功。

## mock 三坑（固化于 script/mock_llm.py，改前必读）

1. 工具参数必须含 agent schema 全部必填字段（缺 `description` → fantasy 静默拒绝）。
2. 工具名从请求 tools 列表自适应选取（Windows 下 claude `-p` 可能提供 `PowerShell` 无 `Bash`）。
3. OpenAI 协议 `stream=true` 必须回 SSE 分块，回 JSON 得 unexpected EOF。

## 灰度机制（decision + 事实）

- claude-code 的 hook 加载受 GrowthBook 灰度翻动：冷缓存窗口（`cold GrowthBook cache, no payload yet` 日志）静默缺席。
- 官方 env `DISABLE_GROWTHBOOK=1`（及 `CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC`/`DISABLE_TELEMETRY`/`DO_NOT_TRACK`）禁用灰度拉取、落二进制内置默认值；hooks 的内置默认=开（实测）。CI 配方钉死此 env。
- `CLAUDE_CODE_ENABLE_FUNCTION_HOOKS` 控制的是插件 hooks 模块加载（`tengu_plugin_hooks_modules` flag），与 settings hooks 执行无关。

## 聚合语义归属（decision）

跨 hook 聚合（exit 2 / JSON deny / allow / halt 的覆盖规则，crush 全序 halt > deny > allow）完全是 agent 侧领域——我方引擎只对自己一个 hook 的裁决负责，用户另装 hook 与我方意见相左时按 agent 语义处理，无需我方验证。design.md 契约节记录该语义仅为适配参考，动态抽查为可选项。真正与我方相关的多 hook 交叉只有 `updated_input` 浅合并（配置序最后者赢——排我方之后的 hook 改写命令会使我方裁决与实际执行脱节，属已登记风险面）。

## crush run 悬案插桩排查记录（已结案 2026-09-09）

### 三假设分支判据

| 插桩日志现象 | 结论 | 下一步 |
|---|---|---|
| `len(preToolHooks)==0` | config 加载层丢 hooks | 追 config/load.go 合并逻辑 |
| len>0 且已包装，但 `hookedTool.Run` 不打印 | run 执行路径绕过 hookedTool | 追 currentAgent 工具链 |
| `hookedTool.Run` 打印但 dump 无 | spawn/shell 层失败 | 追内嵌 POSIX shell 与 PATH |

### 五处插桩点（v0.92.0）

1. `internal/agent/coordinator.go` buildTools 的 hookRunner 构造（`len(preToolHooks)`/isSubAgent）
2. 同文件 `wrapToolsWithHooks` 调用后（包装数/runner_nil）
3. `internal/agent/hooked_tool.go` `wrapToolsWithHooks` 入口（入参计数）
4. 同文件 `hookedTool.Run` 入口（工具名）
5. `internal/agent/agent.go` `SetTools` 与回合快照（total/hooked 计数）

### 排查过程与证据链

1. 编译基线：clone v0.92.0（commit `559ec80`，与 scoop 发布二进制 `go version -m` 的 vcs.revision 一致、旗标同为 CGO_ENABLED=0 + GOEXPERIMENT=greenteagc）→ 编译版 run 触发 hooks——二进制假说出局
2. 2×2 对照（二进制 × `-m`）→ 真判别变量 = mock vs 真实模型
3. 五处插桩：接线全程健康（快照 26/26 hookedTool）但 `hookedTool.Run` 零调用 → 工具从未执行
4. crush.db 会话记录定案：mock 工具调用缺 `description` → fantasy 静默拒绝
5. 修正 mock 后 `crush run -m mockspike` 全链触发（dump + `Hook completed` + `hookedTool.Run` 实跑）

### 证据代码位置（upstream v0.92.0 / fantasy v0.42.0）

- `internal/cmd/root.go`：`useClientServer()` 只认 `CRUSH_CLIENT_SERVER` env，默认本地路径
- `internal/cmd/run.go`：本地路径 `App().RunNonInteractive`；client/server 路径 `connectToServer`
- `internal/app/app.go`：`RunNonInteractive` → `InitCoderAgentNonInteractive` →（有 `-m` 时）`overrideModelsForNonInteractive`（:521/:568）→ `UpdateModels`
- `internal/agent/coordinator.go`：hookRunner 构造读 `Config().Hooks[EventPreToolUse]`；`UpdateModels` = SetModels + buildTools + SetTools
- `internal/agent/agent.go`：回合 `a.tools.Copy()` 快照 → `fantasy.NewAgent(WithTools)`；PrepareStep 内 `prepared.Tools = a.tools.Copy()`
- fantasy `agent.go`：`validateAndRepairToolCall` 按 schema 校验（crush 未设 repair，缺参即 invalid 不分发）；`executeTools` 以 `toolMap[ToolName].Run` 分发

### 重建方式

clone v0.92.0 + 按插桩点重打 slog + `CGO_ENABLED=0 GOEXPERIMENT=greenteagc go build`。
