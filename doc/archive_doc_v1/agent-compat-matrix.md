# agent 兼容性矩阵（M7.3）

> **性质**：实测数据文档（活数据）。机器层（本文件的数据行）由 CI bot 与实测流程更新；结论层（各节分析）人工维护。design.md 各 agent 契约节与失效模式表只回填结论并指回本文件。
> **方法**：`script/hook_probe.py` 探针（四角色 + 控制文件切实验）注册进 agent 交互会话，dump.jsonl 留痕逐项核对；详见 design.md「hook 探针方法（定稿）」。

## 实测环境（锚点 0，2026-09-08）

| agent | 版本 | 会话形态 | 模型/供应商 | 项目 |
|---|---|---|---|---|
| ClaudeCode | 2.1.263 | 交互 TTY（`claude`） | deepseek-v4 系（第三方中转） | TestProject（项目级 `.claude/settings.json` 注册） |
| Crush | 0.92.0（最新 release） | 交互 TUI（`crush`） | deepseek-v4-flash-vision-exp via commandcode | TestProject（项目级 `crush.json` 注册） |
| zcode | （本机 CLI） | 交互会话 | 用户登录 | crush_tether 本仓库 + TestProject（M7 前置/M7.2 实测） |

## 兼容性矩阵（能力 × agent，锚点 0 实测）

| 能力 | ClaudeCode 2.1.263 | Crush 0.92.0 | zcode |
|---|---|---|---|
| 交互会话加载 hooks | ✅（`/hooks` 显示 2 hooks） | ✅（TUI 显示 `Hook hook-probe → OK`） | ✅（M7 前置） |
| **headless 加载 hooks** | **✅ 可钉死为确定**（灰度使能后 `-p` 全语义正常；`DISABLE_GROWTHBOOK=1` 可关灰度、内置默认=开，实测过——见 headless 节更正与深夜追加） | **✅ 正常执行**（~~run 路径未接线~~系 mock 工具形态缺陷造成的误判，2026-09-09 深夜插桩排查推翻：修正 mock 后 `crush run` 全链触发——dump 落盘 + `Hook completed` INFO + 插桩 `hookedTool.Run` 实跑；详见 headless 节二次更正） | n/a（无 headless 形态） |
| allow 直通 | ✅ `permissionDecision:"allow"` exit 0 | ✅ `{"decision":"allow"}` exit 0 | ✅ 三值 JSON |
| confirm 弹确认 | ✅ `permissionDecision:"ask"` → 原生确认 → 批准后执行 | ✅ 无意见（exit 0 无输出）→ 原生权限提示 | ✅ ask 转确认流程 |
| deny 阻断 | ✅ exit 2 + stderr（工具调用不执行） | ✅ exit 2 + stderr（`git push blocked`） | ✅ |
| `updated_input` 改写采纳 | ✅ `updatedInput` 生效（echo 被改写执行） | ✅ 浅合并生效（TUI 标记 `Rewrote Output`） | 待补测 |
| fail-open（hook 非 2 退出） | ✅ UI 明示 `non-blocking status code`，放行 | ✅（文档语义：其他退出码 = 非阻断放行） | ✅ exit 3 放行（M5.3） |
| halt 整个回合 | ❌ 无此概念 | ✅ exit 49「Turn halted by hook」（**引擎不使用**，保持三 agent 统一为单命令阻断） | ❌ |
| `PermissionRequest` 事件 | ❌ 无同语义事件（`PermissionDenied` 是 auto-mode 分类器拒绝，不同物） | ❌ | ✅ 存在但 JSON 回包不被采纳（M5.3） |
| 用户选择回传 | ❌（PostToolUse 仅执行结果） | ❌（无 post 类事件） | ❌（仅执行结果） |
| PostToolUse 事件 | ✅ 载荷含完整 `tool_response`（权限学习信号源可用） | ❌ 仅有 PreToolUse | ✅ |
| 模型层命令预拦截 | ❌ 未观测到 | ✅ system prompt 内置 banned commands（curl/sudo 在工具调用前被模型劝退——更保守，非安全缺口） | 未观测到 |
| hook 超时语义 | ✅ 挂 45s > timeout 30s → ~32s 后放行执行、改写回包未送达（非阻断） | 文档：cancel → 视为非阻断错误 → 放行（未单独实测） | 未测 |
| 项目级 hooks 启用门槛 | ❌ 无（未信任目录 + `-p` 亦声明加载；实测交互即加载） | ❌ 无（配置即生效） | ✅ 工作区审核门（更正登记 21） |

## 关键发现

### 1. headless/非交互模式下的 hooks 加载（**2026-09-09 晚重大更正**：条件性加载，非恒定关闭）

> **⚠️ 更正声明**：本节初版（69f6741）「headless 下 hooks 整体不加载」的绝对结论**已被受控重测推翻**。初版的判定方法有缺陷——误信了 `Registered 0 hooks` / `Found 0 total hooks in registry` 日志计数，而受控重测证明**该计数与实际执行管线脱节**（hook 实际执行时它仍显示 0）。hook 执行的判定必须以**物理副作用**（探针 dump 落盘）+ **`[INFO] Slow PreToolUse hooks` 日志行**为准。以下保留初版排查过程作为历史记录，结论以本更正为准。

**受控重测（2026-09-09，三发 `claude -p`，perm 探针 + delay=45s 控制文件）**：

| 条件 | 结果 |
|---|---|
| 无 `--settings`、无特殊 env | ✅ hook 执行（dump 落盘 + `Slow PreToolUse hooks: 30100ms (1 hooks)` + 30s timeout 杀后放行） |
| `--settings`（env 指向 mock） | 无效样本（mock 未起，API 连接失败） |
| `CLAUDE_CODE_ENABLE_FUNCTION_HOOKS=1` | ✅ hook 执行（同上，30100ms） |

加上前此一发（00:39，无 --settings）共 **3/3 执行**，行为完全正确（拉起 → 读载荷 → 挂起 → 30s timeout 杀 → 放行）。

**修正后的结论**：

1. **`-p` 下 hooks 能且确实正常执行**——全部语义（拉起、载荷、timeout、放行）与交互会话一致，与官方文档及 `--bare` 文案（仅 bare 跳过）**不再矛盾**。
2. **存在随时间变化的使能开关（claude-code）**：同机同配置，2026-09-08 下午 16:4x 的全部 headless 实验 hooks 未执行（日志明示 `cold GrowthBook cache, no payload yet`），当晚 00:39 起全部执行（cold 行消失 = payload 已拉到/缓存生效）。时间线与 GrowthBook 灰度状态（`tengu_plugin_hooks_modules` 等默认 false 的 flag）冷→热完全吻合。**灰度窗口内 hooks 静默缺席**——这才是初版误判的根源。
   - **crush 侧定性二次更正（2026-09-09 深夜，源码插桩排查）**：~~`crush run` 固定不执行~~ **错误，`crush run` 一直正常执行 hooks，两 agent headless 定性统一为「均可用」**。根因链：① 此前全部「run 不触发」实验（三种注册途径、跨日复测）均由 `tmp/mock_llm.py`（现固化 `script/mock_llm.py`）驱动，其 OpenAI-compat 路径返回的工具参数**缺必填的 `description`**；② fantasy（v0.42.0）在工具分发前按 schema 校验参数，缺参调用**静默拒绝**（产出 `missing required parameter: description` 的 error tool result，工具不执行、hook 不运行、无任何日志告警）；③ mock 的「见到 tool result 即回 `spike done`」逻辑把失败伪装成成功（exit 0、对话正常）；④ 触发过 hooks 的 TUI 实验实为**真实供应商模型**驱动（deepseek 的工具调用参数自然完整）——「TUI 触发 / run 不触发」从头被「真实模型 vs mock」混杂。实锤：修正 mock 一行后 `crush run -m mockspike` 全链触发（探针 dump 落盘 + `Hook completed event=PreToolUse` INFO + 插桩 `hookedTool.Run` 实跑）；插桩另证 run 模式下 hook 接线全程健康（buildTools `pre_hooks=1` → wrap 26/26 → SetTools → 回合快照 26/26 全为 hookedTool）。编译版与 scoop 版同 commit（559ec80）同行为，二进制无罪。**方法论教训追加：mock 驱动的 agent 测试中「有 tool result」≠「工具执行过」——参数校验失败会静默产出 error tool result；判定以物理副作用为准的准则再次制胜。**
3. **`CLAUDE_CODE_ENABLE_FUNCTION_HOOKS` 与加载与否无关**（③ 带此 env 照常执行；其真实作用是控制插件 hooks 模块加载，二进制考古所得）。
4. **`--settings` 不屏蔽 hooks（补验证实）**：flag 使能后带 `--settings`（含 hooks 键）重跑——`Slow PreToolUse hooks (2 hooks)`，`--settings` 的 hooks 与项目级 hooks **聚合并存执行**（同 command 各自计一个，串行各 30s timeout 后放行）。下午不跑的唯一解释即灰度窗口。
5. **方法论教训（已并入探针方法实践）**：hook 是否执行的判定 = dump 物理副作用优先，日志计数仅作参考；`Registered/Found 0 hooks` 在 hooks 实际执行时仍打印 0，是本次误判的直接原因。

- **部署含义（修正）**：headless（CI/脚本/自动化）权限门**可用但使能状态随灰度翻动**——新装环境/冷缓存窗口内静默缺席。部署验收仍须实测 hook 确实触发（且注意验收时点与灰度状态相关）。
- **对 CI 的含义（修正）**：headless hooks 探测哨兵从「负向探测」升级为**双向探测**——dump 出现与否都记录，跟踪灰度状态翻动；协议回放与冒烟层不变。**二次更正后**：claude 侧哨兵维持双向；crush 侧恢复**正向**（`crush run` + mock 断言 dump 必须出现）；**mock 工具参数必须完整**（含所有 schema 必填字段）——否则校验静默拒绝、哨兵误报「hook 不执行」。
- **→ 2026-09-09 深夜追加（官方文档 + 实测，claude 灰度不稳定源可消除）**：官方 env-vars 文档（`code.claude.com/docs/en/env-vars`）明载 **`DISABLE_GROWTHBOOK=1`** = 禁用灰度拉取、所有 flag 落二进制内置默认值（`CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC`/`DISABLE_TELEMETRY`/`DO_NOT_TRACK` 同效）；实测带此 env 跑 `claude -p` + mock **hooks 照常全链触发**（内置默认=开）——**claude CI 哨兵升级为正向硬断言（dump 必须出现），与 crush 对齐，灰度翻动不再是不稳定源**。附带新坑与修法：**Windows 下 claude `-p` 的 shell 工具按运行环境选择**（本机 zcode 环境提供 `PowerShell` 无 `Bash`；用户终端同机提供 `Bash`）——mock 盲发 `Bash` 会被静默拒（`No such tool available: Bash`，error tool result、mock 见结果即回包、无告警，与 crush 侧 fantasy 校验拒绝同款伪装）；修法 = **mock 从请求的 tools 列表自适应选工具名**（已实现）+ **hook matcher 放宽 `"Bash|PowerShell"`**。官方仓库 `anthropics/claude-code` 有 `examples/hooks` 官方示例目录（无完整源码）。
- **失效层辨析**（保留，仍然成立）：「权限管道跳过导致阻断被忽略」层与「hook 未被拉起」层是两回事；本仓库实测到的 headless 行为为后者（灰度窗口内）——物理阻断 workaround 对灰度缺席同样无效。
- ~~claude-code 侧开关线索~~（保留历史）：`CLAUDE_CODE_ENABLE_FUNCTION_HOOKS` 控制插件 hooks 模块加载（`tengu_plugin_hooks_modules` 默认 false，GrowthBook 远程覆盖）——受控重测证明它不影响 settings hooks 的执行。

### 2. 三档裁决语义两侧一致且与 design.md 契约节吻合

allow 直通 / confirm 走原生确认（claude 经 `ask` 信封，crush 经无意见）/ deny 阻断（两侧均为 exit 2 + stderr 路径）。契约节实测坐实。

### 3. `updated_input` 在两侧真实生效

hook 回包可改写命令（模型无感知，实际执行改写后命令）。权限门视角：我方引擎的 `updated_input` 输出（若有）在两侧真实生效；同时这也是**风险面**——任意 hook 都能改写命令，部署时 hook 脚本本身在信任边界内。

### 4. crush 的模型层预拦截与其两级阻断

crush 的 system prompt 内置 banned commands 规则（curl/sudo 等），confirm/deny 类命令常在工具调用前被模型自行劝退（方向更保守，非安全缺口，但实测时需换黑名单外命令才能触达 hook 层）。阻断两级：exit 2 = 阻断单命令回合继续（**引擎采用，与 claude/zcode 行为统一**）；exit 49 = halt 整回合（agent 特有能力，引擎不使用）。

### 5. 权限学习可行性（顺带评估）

跨 agent 一致信号源 = PostToolUse：claude-code 载荷含完整 `tool_response` 与 `tool_input`；zcode 仅执行结果；crush 无 post 事件。suggest 候选在 claude-code 信号最全，crush 侧不可行（无信号），维持「不支持的 agent 上默认不生效」的降级原则。

## 测试方法与自动化策略（headless 受限下的设计）

前提事实（2026-09-09 深夜三次更正后）：headless（`claude -p` / `crush run`）hooks 均可执行且**可钉死为确定行为**——claude 经 `DISABLE_GROWTHBOOK=1` 关灰度（内置默认=开，实测过）、crush 无条件；CI 一律**正向硬断言**。mock 驱动时：工具名自适应 agent 实际提供的列表（Windows claude 可能提供 `PowerShell` 非 `Bash`）、工具参数含全部 schema 必填字段（缺参会被 fantasy 静默拒绝）。agent 兼容性测试按可自动化性拆三层（**已落地 `.github/workflows/agent-matrix.yml`：push/PR pinned smoke + weekly cron latest 哨兵 + dispatch 指定版本回溯，2026-09-09；首跑待 push 验证**）：

### CI 常态层（全自动，cron 驱动，零凭证）

1. **协议回放**：用实测攒下的 dump.jsonl 载荷样本（带 agent 版本标注）直接驱动探针+引擎，断言回包契约（信封格式 / 三值 / exit code）——防我方回归，不测 agent；样本漂移靠版本标注显性化。
2. **agent 活性冒烟**：mock LLM 后端驱动最新版 agent 完成一轮对话——确认能装、能起、协议未崩。
3. **headless hooks 正向探测（两侧一致，2026-09-09 深夜起）**：注册探针 + headless 跑一发，断言 dump **必须**出现——缺席 = 接线破坏，报警。claude 侧配方 = `DISABLE_GROWTHBOOK=1`（钉死灰度态，官方 env）+ mock 自适应工具名 + matcher `"Bash|PowerShell"`；crush 侧 = mock（参数完整）+ `crush run`。原「负向探测」与「claude 双向哨兵」设计基于误判与灰度翻动顾虑，均作废（灰度已可官方 env 钉死）。
4. **版本与变更追踪**：cron 拉 npm / GitHub Releases 最新版本号 + release notes 抓 hook 相关关键词 → 有信号才触发下一层。

### 发版触发层（人工 ~5 分钟，清单化）

新版本或 hook 相关变更信号出现时执行：预设注册与控制文件 → 一次交互会话照清单跑命令 → 读 dump 断言入矩阵。清单（以 claude-code 为例，crush 同构）：`echo hi`（allow 直通）→ `curl --version`（confirm 弹窗，批准后执行）→ `sudo --version`（deny 阻断）→ 控制文件切 exit 3 后任一命令（fail-open 放行）。

### 大版本层（人工 ~15 分钟，低频）

大版本或疑似破坏时跑全轴：`updated_input` 改写、超时挂起、halt（crush exit 49）、双探针聚合、非默认 permission_mode 交叉。

### 明确不做

交互 TUI 自动化（ConPTY/winpty 驱动）——脆弱、维护成本远超每版本 5 分钟人工；agent SDK 的 headless API 走的不是同一 hooks 路径——自动化测错形态比人工测对形态更糟。

### bot commit 边界

矩阵机器层（版本号、headless 探测结果、协议回放结果）可由 bot commit（scoop 模式：`[skip ci]` + 限定路径 + 最小权限）；hooks 行为行标注「人工交互实测 + 版本/日期」，永远人工维护。

### 实验操作细节（复现本矩阵实验的实操知识）

- **claude-code**：settings env 优先级 = shell 环境变量 < 用户级 `~/.claude/settings.json` 的 `env` 块（**会覆盖 shell 环境变量**——实验端点注入必须走下一级）< `--settings` 传入（可再覆盖前者）；hook 观察 = `--debug hooks --debug-file <file>`（看装配，但 `Registered/Found 0` 计数不可信）+ `--include-hook-events`（事件流带 hook 生命周期）+ 交互 `/hooks` 菜单（只读）；模型弃用警告不影响请求；`-p` 下 workspace trust dialog 被跳过。
- **crush**：项目级 `crush.json` 可同时承载 `providers`（`openai-compat` + `base_url` 指向本地 mock 即零费用实验）与 `hooks`；全局 `~/.config/crush/crushrc` 是 provider add 命令脚本（非 JSON）且另有 `hook add <event> --command CMD [--name] [--matcher] [--timeout]` builtin；`-y --yolo` 是 root flag（`run` 子命令无 yolo）；模型选择 `-m provider/model`；hook 执行的日志判据 = `Hook completed` INFO 行（runner.go），`crush logs` 查看。

## 版本记录

| 版本 | 结论 | 日期 | 方式 |
|---|---|---|---|
| claude-code 2.1.263 / crush 0.92.0 | 本矩阵全部实测行 | 2026-09-08 | 人工交互实测（探针） |
| claude-code 2.1.195 | headless hooks 不加载（与 .263 一致） | 2026-09-08 | headless 对照 |
| 历史版本回溯 | 未测（按需二分） | — | CI/人工 |

## 待补测

- claude-code **交互 + 全放行形态**（`--dangerously-skip-permissions` / `allowedTools:["*"]`）下 hook 是否仍被评估——社区「权限管道跳过」假说的直接验证（zcode 侧已有同构结论：hook 评估先于原生权限并可覆盖 yolo）
- exit 2 与 JSON 回包并发时的覆盖规则（契约：exit 2 覆盖 JSON；未单独实测）
- claude-code 非默认 permission_mode（plan/bypassPermissions 交互）× hook 交叉；zcode 计划模式交叉（原 M7.3 待补两项）
- zcode `updated_input` 采纳

已收口：~~crush hook 超时的实机验证~~（2026-09-09 深夜 headless 实测：探针 delay 45s > timeout 30 → 33s 后非阻断放行，与 claude ~32s 行为一致）。

## 附录：crush run 悬案插桩排查记录（已结案，2026-09-09 深夜）

> 本附录由曾存于 `tmp/m73-crush-src-investigation-plan.md` 的排查计划与其执行结果合并存档
> （原 tmp/ 载体已删；自约束起文档不得以 git 未追踪文件为内容载体）。结论正文见 headless 节二次更正。

### 背景与假设

crush v0.92.0 交互 TUI hooks 全语义正常（`crush logs` 5 条 `Hook completed` INFO），`crush run` 三种注册途径（全局/项目 crush.json、crushrc `hook add` builtin）跨日均不执行（探针 dump 零记录、`runner.Run` 未被调用）。源码静态分析预测 run 应执行——动态与静态矛盾未闭合。三假设分支判据（插桩一次 run 即分胜负）：

| 日志现象 | 结论 | 下一步 |
|---|---|---|
| `len(preToolHooks)==0` | config 加载层丢 hooks | 追 `internal/config/load.go` 合并逻辑 |
| len>0 且已包装，但 `hookedTool.Run` 不打印 | run 执行路径绕过 hookedTool | 追 currentAgent 工具链 / run 专用 agent 构建 |
| `hookedTool.Run` 打印但探针 dump 无 | spawn/shell 层失败 | 追内嵌 POSIX shell（mvdan.cc/sh）执行与 PATH |

### 插桩点（最终五处，各 3-5 行 slog）

1. `internal/agent/coordinator.go` buildTools 的 hookRunner 构造处：打 `len(preToolHooks)` 与 `isSubAgent`
2. 同文件 `wrapToolsWithHooks` 调用后：打包装后工具数与 runner 是否 nil
3. `internal/agent/hooked_tool.go` `wrapToolsWithHooks` 入口：打入参工具数/runner_nil/is_sub
4. 同文件 `hookedTool.Run` 入口：打工具名
5. `internal/agent/agent.go` `SetTools` 与回合快照（`Run` 内 `a.tools.Copy()` 处）：各打 total/hooked 计数

### 排查过程（证据链）

1. **编译基线**：clone v0.92.0（commit `559ec80`，与 scoop 发布二进制 `go version -m` 所载 vcs.revision 一致、vcs.modified=false、旗标同为 CGO_ENABLED=0 + GOEXPERIMENT=greenteagc）→ 编译版 run 反而**触发** hooks（dump 4→42 行）——二进制假说出局
2. **2×2 对照**（二进制 × `-m`）：不触发的两格均为 mockspike（mock）驱动、触发格走真实供应商 → 真判别变量 = **mock vs 真实模型**
3. **五处插桩**：run 模式下接线全程健康（`pre_hooks=1` → wrap 26/26 → SetTools → 回合快照 26/26 全为 hookedTool），但 `hookedTool.Run` 零调用——工具从未执行
4. **crush.db 会话记录定案**：mock 的 OpenAI-compat 路径返回的工具调用缺必填 `description`，fantasy 分发前按 schema 校验、缺参**静默拒绝**（error tool result「missing required parameter: description」，无日志）；mock「见 tool result 即回 spike done」伪装成功
5. **修正 mock 一行**（补 description）后 `crush run -m mockspike` 全链触发（dump 落盘 + `Hook completed` INFO + `hookedTool.Run` 实跑）

### 证据代码位置（upstream v0.92.0 / fantasy v0.42.0，替代原 tmp/*.go 样本）

- `internal/cmd/root.go`：`useClientServer()` 只认 `CRUSH_CLIENT_SERVER` env——默认走本地路径（`run.go:157`）
- `internal/cmd/run.go`：本地路径 = `App().RunNonInteractive`；client/server 路径 = `connectToServer` + `runNonInteractive`
- `internal/app/app.go`：`RunNonInteractive` 先 `InitCoderAgentNonInteractive` 再（有 `-m` 时）`overrideModelsForNonInteractive`（:521 override INFO、:568 unknown-provider WARN）→ `UpdateModels`
- `internal/agent/coordinator.go`：buildTools 的 hookRunner 构造（读 `c.cfg.Config().Hooks[EventPreToolUse]`）与 `wrapToolsWithHooks(filteredTools, hookRunner, isSubAgent)`；`UpdateModels` = SetModels + buildTools + SetTools
- `internal/agent/agent.go`：回合开始 `a.tools.Copy()` 快照 → `fantasy.NewAgent(WithTools(...))`；`PrepareStep` 内 `prepared.Tools = a.tools.Copy()`
- fantasy `agent.go`：`validateAndRepairToolCall` 按 tool schema 校验（crush 未设 repair 函数，缺参即 invalid，**不执行不分发**）；`executeTools` 以 `toolMap[toolCall.ToolName].Run` 分发

### 存档说明

排查产物（crush-src clone、crush-dbg.exe、crush-instr.exe、tmp 版 mock）已清理；重建 = clone v0.92.0 + 按「插桩点」重打 + `CGO_ENABLED=0 GOEXPERIMENT=greenteagc go build`。现行 CI 复用同套排查面：`script/mock_llm.py`（固化版）+ `script/hook_probe.py`。
