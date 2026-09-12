---
type: project_topic
status: active
summary: M7.3 agent hook 兼容性测试的方法论与过程沉淀：判定准则（hook 执行以探针 dump 物理副作用为准，日志计数与 tool result 回执不可尽信）、mock 四坑（schema 必填字段/工具名环境差异/OpenAI SSE/SSE input 对象校验）、灰度可官方 env 钉死（DISABLE_GROWTHBOOK）、crush run 悬案插桩排查全记录（三假设判据/五处插桩/证据代码位置）、四次更正史链条（含 zcode headless 形态与插件轨/config 轨信任门分化定性）、无头场景组定性（fail-open 按 agent 形态分化 + zcode spawn 精简 env）；M8.6 补：本机用户全局配置是 headless 探针的干扰源（探针核对移 CI 自动化）、PostToolUse 载荷自动抓包模式；M8.x 批量推送后 CI 首跑红灯归因（行为变更拆测试拐杖/env_remove 抵消 env 的假绿测试/heredoc 生成物校验/fail-fast 掩盖）。确定性结论的现役事实在 doc/agent-compat-matrix.md，测试方法与挂账在 doc/test-and-ci.md。
tags: [crush_tether, hook, testing, mock, claude-code, crush, zcode, headless, methodology]
contains: [lesson, decision, pattern]
created: 2026-09-09
updated: 2026-09-12
related: [doc/agent-compat-matrix.md, doc/test-and-ci.md, doc/design.md, script/mock_llm.py, script/hook_probe.py]
authoring_mode: ai_generated
---

# agent hook 兼容性测试：方法论、教训与排查记录

> 确定性事实的现役版本（矩阵/覆盖口径）在 `doc/agent-compat-matrix.md`，测试方法/CI 设计/挂账在 `doc/test-and-ci.md`；本文沉淀**过程史、更正链条与可复用方法论**。逐日流水见 LOG，里程碑口径见 ROADMAP M7.3 条。

## 四次更正史（链条摘要）

1. **初判（2026-09-08）**「headless（`claude -p`/`crush run`）hooks 整体不加载」——判定依据是 `Registered 0 hooks` 日志计数。
2. **一更（09-09 受控重测）**：claude 侧推翻——headless 为**条件性加载**（GrowthBook 灰度冷↔热翻动，冷窗口静默缺席）；日志计数与执行管线脱节（执行时仍打 0）。crush 侧当时仍判「run 固有不执行」。
3. **二更（09-09 深夜插桩）**：crush 侧推翻——`crush run` **一直正常执行 hooks**，根因是我方 mock 的工具调用缺必填 `description` 被 fantasy 静默拒绝（详见下文排查记录）；「TUI 触发/run 不触发」从头是「真实模型 vs mock」混杂。
4. **三更（09-09 深夜官方文档+实测）**：claude 灰度可用 `DISABLE_GROWTHBOOK=1` 官方钉死——CI 两侧统一为正向硬断言，灰度翻动不再是不稳定源。
5. **四更（09-10 深夜）**：zcode 侧推翻——「无 headless 形态」不成立，App 内嵌 CLI（`resources/glm/zcode.cjs`，0.16.5）有 `-p/--prompt` 非交互形态；成因 = 入口不在 PATH（`which zcode` 落空后即下结论，未探 App 安装树）。教训同母题：**「命令不存在」≠「能力不存在」**，发行形态未查全前不下能力结论。

教训母题：**误判从不来自测不到，而来自信错了信号**（日志计数、tool result 回执）；唯一可信判据是物理副作用。

母题二（2026-09-11 补，与四更同构）：**「交互如此 ≠ 无头如此」——能力 × 会话形态是两个独立维度**。fail-open 在交互与无头下同 agent 两种兜底（见下节），此前矩阵只测交互格就默认两形态一致，一测就碎。与「命令不存在≠能力不存在」同根：坐标轴没铺全之前，单点结论不外推。

## 2026-09-11 无头场景组定性（deny / fail-open / rewrite 三家实证）

- **fail-open 按 agent × 形态分化**：claude 交互放行（UI 明示 non-blocking）/ 无头**拒绝**——`claude -p --output-format json` 回执的 `permission_denials` 数组是官方铁证（hook 失联被归入权限拒绝，工具不执行，回合正常收敛）；crush / zcode 无头与交互一致放行。**机制理解**：deny/allow 是 hook「明确表态」，语义跨形态稳定；fail-open 是「hook 失联」后 agent **自己的兜底策略**——交互下兜底=回正常权限流（放行），无头下没有正常权限流可回=保守拒绝。分化只可能出现在兜底类语义上。
- **zcode 无头对 confirm 同样保守拒绝**（正式插件引擎裁 confirm 后工具不执行、回合正常收敛）——无人批准时与 claude 无头 fail-open 行为同构。
- **zcode spawn hook 给精简 env**：插件 hooks.json 的 command 若为 `uv`（shim/wrapper）或依赖 PATH 解析的形式**静默失效**——uv 找不到它管理的 python，连首行诊断文件都写不出；表现极易误判为「语义拒绝」（工具被兜底拒了）。**spawn 可达性二分排障法**：command 换 `--version` 类空参快跑（uv 自身 flag，不依赖环境）——能放行即 spawn 可达、问题在程序；仍拒即 spawn 层失败。修复 = command 用绝对路径解释器（CI 探针插件用运行时 `sys.executable`）。
- **本机实弹的装配教训**：user 级插件注册表（`known_marketplaces.json`/`installed_plugins.json`）在有存量官方插件的环境里**不可整文件重写，只能增量追加/删改自己的条目**（CI 全新环境才可清空重建）；enabledPlugins 增量切换 + 测后备份复原。

## zcode headless 定性记录（2026-09-10 深夜）

- **发现路径**：用户给出 `D:\Software\Scoop\apps\zcode\3.11.2\resources\glm\zcode.cjs` → `--help` 直揭 `-p/--print`、`--mode`、`--settings`、`--max-turns`、`app-server` 等全套 headless 面；`doctor` 确认 CLI 版本轨道 0.16.5（与 App 3.11.2 双轨）。
- **provider 配置 schema**（bundle 逆向 + 试错定位）：用户级 `~/.zcode/cli/config.json` 增 `provider.<id>` 注册表（`kind`/`name`/`options.baseURL`/`options.apiKey`——端点密钥必须在 `options` 下，条目顶层写法被忽略）+ `model.main` 只接受 `"provider/model"` 字符串（对象形态被 schema 静默丢弃，报错仅 "Model config is missing"）；anthropic kind 走 env `ANTHROPIC_API_KEY` 兜底。
- **mock 第四坑**：zcode 的 Vercel AI SDK 严格校验 Anthropic SSE——`content_block_start` 的 tool_use `input` 必须是对象，固化版 mock 回 `""` 即整回合 `AI_TypeValidationError` 失败；claude/crush 对空串宽容。已修 `script/mock_llm.py`（`"input": {}`）。
- **hook 两轨 headless 分化**（本日核心定性）：
  - **插件轨 ✅**：`enabledPlugins` 开启后 `-p` 下 hook 正常拉起，`decisions.jsonl` 落引擎裁决（`echo mock-hook-test → allow`），全程零 UI 零交互——CI 可自动化路径。
  - **config 轨 ❌（headless）**：项目 hooks 声明形状是 `hooks.events.<Event>`（非插件 envelope 的 `hooks.<Event>`，写错仅 `config.file.invalid` 日志）；解析后必挂 `config_project_hooks_pending_trust`，信任由 capable host（Desktop App UI 审查流）授予并按 工作区+声明 digest 持久化于 `~/.zcode/security/workspace-hook-trust-v1.json`；headless CLI 无宿主审查流 → `workspace_hooks_require_trust_capable_host`/`workspace_hooks_feature_disabled`，**不可首授**（已信工作区可复用记录，但声明变 digest 即失效）。
- **入 CI 判定**：能力具备、卡发行——npm 无官方包（`zcode-app-cli`、`zcode-acp-server` 为第三方，后者佐证 headless 生态）；本机仅证 win32-x64 内嵌 bundle；Linux 渠道与插件无人值守 provisioning（`~/.zcode/cli/plugins` 文件级装配）挂 doc/test-and-ci.md §5 待办。

## 判定准则（lessons）

- hook 是否执行，以**探针 dump 物理副作用**为唯一判据；`Registered/Found 0 hooks` 计数、`Hook completed` 缺席均不可单独定论。
- mock 驱动下「有 tool result」≠「工具执行过」——参数校验失败、工具名缺席都会静默回 error tool result，无任何日志告警。
- 「对话正常 + exit 0」不等于链路健康：mock 的松回包逻辑（见任意 tool result 即回包）会把失败伪装成成功。

## 本机探针的配置噪声与 CI 自动抓包（2026-09-12 M8.6）

- **教训：本机用户全局配置是 headless 探针的干扰源**——本机核对 claude PostToolUse 载荷时，用户 `~/.claude` 全局配置的 title 生成模型（`deepseek-v4-flash`）先于主请求发出，本地 mock 不识别该 model 名即 4xx，claude 主链路中止（exit 2、工具未执行、探针零 dump）。与「zcode spawn 精简 env」同族升级版：**本机环境不只是少了东西，还多了用户自己的配置**——headless 探针在本机跑不通时先查全局 settings 注入了什么（title/model/env），不要先怀疑被测链路。
- **pattern：探针核对固化为 CI 持续产物**——「实现期探针核对字段全集」这类一次性核对，直接在 CI 场景/冒烟 settings 给探针挂上目标事件（PostToolUse），next push 首跑自动抓真实载荷到 dump.jsonl：干净环境（无用户配置噪声）+ 每次跑都复验 + 三 agent 同批覆盖。代价是 agent-matrix 的 settings JSON 模板翻倍（claude/crush × ubuntu/windows 文本变体各自维护——python/python3、${ws}/$ws/PWD 差异，改模板要四处对齐）。
- **坑：hook_probe.py post 角色本就 dump stdin**（dump 对非 fail 角色通用，读代码时被 `if role == "post": return 0` 的提前 return 误导为「不 dump」）——「探针不抓包」的结论先看 dump 调用是否在分派之前，再看角色分支。
- **坑：Windows claude 的 bash 类工具名是 `PowerShell`**——hook matcher 必须写 `Bash|PowerShell`（mock 的 pick_tool_name 已自适应工具名，但 hook 的 matcher 是我们写的，漏 PowerShell = PostToolUse 永不触发且无报错）。

## 2026-09-12 CI 首跑红灯归因（M8.1–M8.6 十一提交批量推送后）

- **母题：行为变更拆掉测试的隐形拐杖**——M8.1 拆除「三层皆缺自动生成默认包」后 CI 首跑红了三处：alias 链老测试、zcode 冒烟正向断言。拐杖有两条：CI 上靠自动生成拿到 allow；本地靠 cwd 上溯摸到**开发者自有配置**（仓库根 `.crush-tether/` 被 gitignore = 本地永远有、CI 永远没有）。结论：**改运行时行为（尤其「缺省兜底」类）必须扫一遍「谁在无配置环境下依赖旧兜底」**；本地全绿对 CI 无证明力，干净 worktree（不含 gitignore 件）全量 test 是廉价预演。
- **假绿测试样本（alias 链测试带病绿灯三周）**：`Command::env(k,v)` 之后链式 `env_remove(k)`——std 同键后调用生效，环境变量当场被抹，测试从未测过它名字声称的事，靠上述双拐杖长期绿灯。机理：env 构造顺序错误不报错、只静默降级到 fallback 路径，恰好 fallback 在两个环境都有副作用。
- **CI 资产编辑坑×2**：①给既有 heredoc 加块时**重复插入**（同一 PostToolUse 块 M8.2/M8.4 已加过）+ 丢逗号 → JSON 非法，症状隔层：claude 无视整份 settings（连 env 认证一起丢 → `Not logged in`）、crush 直接拒载配置——**生成物必须 json.tool 校验一步**（已固化进 workflow）；②新步骤插在依赖就绪之前（wrapper 守卫检查先于引擎安装，而 wrapper 设计即缺席响亮失败）——新增 step 先画依赖线再定位。
- **fail-fast 掩盖**：`cargo test` 默认首个失败二进制即停，本次 contract_adapters 之后的测试全部没跑，无法排除更多同类失败——CI test 步骤已改 `--no-fail-fast`；本地用干净 worktree 全量复跑补齐证据（21 二进制全绿）。

## mock 四坑（固化于 script/mock_llm.py，改前必读）

1. 工具参数必须含 agent schema 全部必填字段（缺 `description` → fantasy 静默拒绝）。
2. 工具名从请求 tools 列表自适应选取（Windows 下 claude `-p` 可能提供 `PowerShell` 无 `Bash`）。
3. OpenAI 协议 `stream=true` 必须回 SSE 分块，回 JSON 得 unexpected EOF。
4. Anthropic SSE `content_block_start` 的 tool_use `input` 必须是对象（zcode AI SDK 严格校验，回 `""` 整回合失败；claude/crush 宽容）。

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
