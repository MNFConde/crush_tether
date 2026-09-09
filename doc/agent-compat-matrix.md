# agent 兼容性矩阵（M7.3）

> **定位**：只记确定性事实——兼容性矩阵、版本测试结果（带更新时间）、测试方法、agent 差异与规避。排查过程、错误结论与更正史一律在 cairn/（LOG、ROADMAP、[agent-hook-testing](../cairn/agent-hook-testing.md)），不进本文档。
> **覆盖口径**：交互全语义（三档弹窗 / `updated_input` / halt / fail-open / 超时）= Windows 手工会话（锚点 0 人工批测）；headless hook 冒烟（正向断言）= Linux CI runner（pinned 每次 push/PR、latest 每周 cron）+ Windows 本机预演。除 §4 另注明外，结论均在 Windows 10 x64 实测。

## 1. 兼容性矩阵

### 1.1 agent × 版本（当前测试结论）

| claude-code | crush | zcode |
|---|---|---|
| 2.1.263（pinned）：通过 | 0.92.0（pinned）：通过 | 本机 CLI：部分通过（三档 ✅；headless n/a；`updated_input` 未测） |

注：本表只维护**当前状态**，格子只记通过/未通过；版本由不过转为通过时仅更新格子，变更流水记 §2。新版本（cron/dispatch）实测后**补一行**——与 pinned 同版且结果无差异则不变更；仅一个 agent 有新版时，新行其它 agent 格留空；latest 未实测不记录。pinned 不通过 = 「我方破坏」，latest 不通过 = 「上游信号」。

### 1.2 当前 pinned 能力快照（锚点 0 实测）

| 能力 | claude-code 2.1.263 | crush 0.92.0 | zcode |
|---|---|---|---|
| 交互会话加载 hooks | ✅（`/hooks` 显示注册数） | ✅（TUI 显示 `Hook hook-probe → OK`） | ✅（M7 前置） |
| headless 加载 hooks | ✅ 可钉死（`DISABLE_GROWTHBOOK=1`，内置默认=开） | ✅ 无条件（配置即生效） | n/a（无 headless 形态） |
| allow 直通 | ✅ `permissionDecision:"allow"` exit 0 | ✅ `{"decision":"allow"}` exit 0 | ✅ 三值 JSON |
| confirm 弹确认 | ✅ `permissionDecision:"ask"` → 原生确认 → 批准后执行 | ✅ 无意见（exit 0 无输出）→ 原生权限提示 | ✅ ask 转确认流程 |
| deny 阻断 | ✅ exit 2 + stderr（工具调用不执行） | ✅ exit 2 + stderr，或 JSON deny | ✅ |
| `updated_input` 改写采纳 | ✅ 全替换语义（echo 被改写执行） | ✅ 浅合并（配置序最后者赢；TUI 标记 `Rewrote Output`） | 待补测 |
| fail-open（hook 非 2 退出） | ✅ UI 明示 `non-blocking status code`，放行 | ✅ 其他退出码 = 非阻断放行 | ✅ exit 3 放行（M5.3） |
| hook 超时语义 | ✅ 挂 45s > timeout 30s → ~32s 放行 | ✅ headless 实测 33s 非阻断放行 | 未测 |
| halt 整个回合 | ❌ 无此概念 | ✅ exit 49（**引擎不使用**，保持单命令阻断统一） | ❌ |
| `PermissionRequest` 事件 | ❌ 无同语义事件 | ❌ | ✅ 存在但 JSON 回包不被采纳（M5.3） |
| 用户选择回传 | ❌（PostToolUse 仅执行结果） | ❌（无 post 类事件） | ❌（仅执行结果） |
| PostToolUse 事件 | ✅ 载荷含完整 `tool_response`（权限学习信号源） | ❌ 仅有 PreToolUse | ✅ |
| 模型层命令预拦截 | ❌ 未观测到 | ✅ banned commands 内置（curl/sudo 被劝退，更保守非缺口） | 未观测到 |
| 项目级 hooks 启用门槛 | ❌ 无 | ❌ 无（配置即生效） | ✅ 工作区审核门 |

## 2. 版本测试结果记录

| 日期 | agent | 版本 | 触发器 | 结果 | 变更点/备注 |
|---|---|---|---|---|---|
| 2026-09-08 | claude-code | 2.1.263 | 人工交互（锚点 0 全轴） | ✅ | 三档 / `updated_input` / fail-open / 超时 全语义基线 |
| 2026-09-08 | crush | 0.92.0 | 人工交互（锚点 0 全轴） | ✅ | 三档 / `updated_input` / halt 全语义基线 |
| 2026-09-08 | claude-code | 2.1.195 | headless 对照 | ⚠️ 已废 | 「headless 不加载」系灰度窗口假象（更正史见 cairn）；版本已退出锚点 |
| 2026-09-09 | claude-code + crush | pinned | 人工 headless | ✅ | headless 正向配方定稿；crush 超时 33s 放行实测 |
| 2026-09-09 | claude-code | 2.1.263 | CI 首跑（ubuntu，pinned） | ✅ | Linux 首证：headless 正向断言通过 |
| 2026-09-09 | crush | 0.92.0 | CI 首跑（ubuntu，pinned） | ✅ | 同上（tar 安装修复后全绿） |

## 3. 测试如何进行

### 原理

mock LLM 后端驱动 agent 完成一轮固定 tool_use（`echo mock-hook-test`），注册的探针 hook 对该调用落盘 dump——**dump 出现 = hook 被拉起且裁决流转**（正向硬断言）。hook 链路是 agent 本地行为、与 LLM 无关，故 mock 驱动零凭证零费用。探针四角色与控制文件切实验见 design.md「hook 探针方法（定稿）」。

### 三层触发（[.github/workflows/agent-matrix.yml](../.github/workflows/agent-matrix.yml)）

- **push/PR**：pinned smoke + `paths` 过滤 agent 耦合面（`src/channel/`、`plugin/`、探针/mock 脚本）——红灯归因「我方破坏」
- **weekly cron**：latest smoke——上游破坏性变更哨兵（红灯 = 上游信号）
- **workflow_dispatch**：手动指定版本回溯/排查
- **明确不做**：交互 TUI 自动化（脆弱，维护成本远超每版本 5 分钟人工）；agent SDK headless API（不走同一 hooks 路径）
- **后置**：协议回放（dump 样本驱动引擎）、bot commit 矩阵机器层（测完自动改表提交，`[skip ci]`）

### 本地复现

`uv run --directory script mock_llm.py [--port 8787] [--log 请求日志.jsonl]` 起后端；hook 注册 = `script/hook_probe.py` 四角色（crush 走项目 `crush.json` 的 providers+hooks；claude 走 `--settings` 文件 env+hooks）；判定 = dump 行数与内容。三坑与判定准则见 §4。

### 人工批 1 清单（~5 分钟，发版触发）

预设注册与控制文件 → 交互会话照单跑：`echo hi`（allow 直通）→ `curl --version`（confirm 弹窗批准后执行）→ `sudo --version`（deny 阻断）→ 控制文件切 exit 3 后任一命令（fail-open 放行）→ 读 dump 断言回填 §2。

## 4. agent 差异与规避

| # | 差异 | 规避 |
|---|---|---|
| 1 | crush：工具调用缺 schema 必填字段（如 `description`）→ fantasy 校验**静默拒绝**（error tool result、无日志，伪装成对话正常） | mock 工具参数含全部必填字段 |
| 2 | claude（Windows）：shell 工具按运行环境选（`PowerShell`/`Bash`），工具列表无 `Bash` 时盲发被拒（`No such tool available`） | mock 从请求 tools 列表自适应选名 + matcher `"Bash\|PowerShell"` |
| 3 | claude：hook 加载随 GrowthBook 灰度翻动（冷窗口静默缺席） | `DISABLE_GROWTHBOOK=1` 钉死（官方 env；hooks 内置默认=开，实测） |
| 4 | crush OpenAI-compat：`stream=true` 必须回 SSE 分块，回 JSON 得 unexpected EOF | mock 完整实现分块（role → tool_calls → finish） |
| 5 | claude：settings env 三级优先级（shell < 用户级 `env` 块 < `--settings`），shell 注入会被覆盖 | 实验端点注入走 `--settings` |
| 6 | crush：banned commands 模型层预拦截（curl/sudo 在工具调用前被劝退，不触达 hook 层） | 测试用黑名单外命令 |
| 7 | **判定准则（通用）**：mock 驱动下「有 tool result」≠「工具执行过」（校验拒绝/工具缺席都会回 error result）；hook 执行以**探针 dump 物理副作用**判定；日志计数（`Registered 0 hooks`）与回执均不可尽信 | 探针 dump 为唯一判据 |

## 5. 待补测

- claude-code **交互 + 全放行形态**（`--dangerously-skip-permissions` / `allowedTools:["*"]`）下 hook 是否仍被评估——社区「权限管道跳过」假说（zcode 侧已有同构结论：hook 评估先于原生权限并可覆盖 yolo）
- exit 2 与 JSON 回包并发时的覆盖规则——**上游聚合语义引用（halt > deny > allow），非我方行为面**，仅可选抽查以验证 design.md 契约节引用的准确性
- claude-code 非默认 permission_mode（plan/bypassPermissions 交互）× hook 交叉；zcode 计划模式交叉
- zcode `updated_input` 采纳
