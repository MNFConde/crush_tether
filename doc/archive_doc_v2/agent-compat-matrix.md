# agent 兼容性矩阵（M7.3）

> **定位**：只记确定性事实——兼容性矩阵、版本测试结果（带更新时间）、测试方法、agent 差异与规避；维护口径见 §6。排查过程、错误结论与更正史一律在 cairn/（LOG、ROADMAP、[agent-hook-testing](../cairn/agent-hook-testing.md)），不进本文档。
> **覆盖口径**：交互全语义（三档弹窗 / `updated_input` / halt / fail-open / 超时）= Windows 手工会话（锚点 0 人工批测）；headless hook 冒烟（正向断言）= CI runner（ubuntu 双 job：claude/crush；windows 三 job：claude/crush/zcode，pinned 每次 push/PR、latest 每周 cron）+ Windows 本机预演。除 §4 另注明外，结论均在 Windows 10 x64 实测。

## 1. 兼容性矩阵

### 1.1 agent × 版本（当前测试结论）

| claude-code | crush | zcode |
|---|---|---|
| 2.1.263（pinned）：通过 | 0.92.0（pinned）：通过 | 3.11.2（Desktop App）：通过 |

注：
1. 本表只维护**当前状态**，格子只记通过/未通过
2. 版本由不过转为通过时仅更新格子，变更流水记 §2。新版本（cron/dispatch）实测后**补一行**——与 pinned 同版且结果无差异则不变更
3. 仅一个 agent 有新版时，新行其它 agent 格留空
4. latest 未实测不记录
5. pinned 不通过 = 「我方破坏」，latest 不通过 = 「上游信号」。
6. zcode 已入 CI（仅 windows）：App 内嵌 CLI 为 win32-x64 bundle，版本从 scoop extras bucket manifest 解析（latest）+ CDN 直链下载，7z 两步解包取 `zcode.cjs`（不装 App）；hook 走**插件轨**无人值守装配（`known_marketplaces`/`installed_plugins`/`cache` 拷贝 + `enabledPlugins`，本机 spike 验证无校验障碍）；hook 装配的前提项（工作区信任门）不适用插件轨。Linux zcode 内测中，公测后补 ubuntu job

### 1.2 当前 pinned 能力快照（锚点 0 实测）

> 列头 = §1.1 当前通过版本；换版本的更新动作（差异开新列/无差异并区间）、需要性标注口径、复核分层见 **§6 维护规则**。未复核格以 §2 最新流水为准。

| 能力 | 需要性 | claude-code 2.1.263 | crush 0.92.0 | zcode 3.11.2（Desktop App） |
|---|---|---|---|---|
| 交互会话加载 hooks | 需要 | ✅（`/hooks` 显示注册数） | ✅（TUI 显示 `Hook hook-probe → OK`） | ✅（M7 前置） |
| headless 加载 hooks | 需要 | ✅ 可钉死（`DISABLE_GROWTHBOOK=1`，内置默认=开） | ✅ 无条件（配置即生效） | ✅ `-p` 非交互（内嵌 CLI 0.16.5，mock 驱动全回合；hook 走插件轨可拉起，config 轨信任门 headless 不可首授，见 §4） |
| allow 直通 | 需要 | ✅ `permissionDecision:"allow"` exit 0 | ✅ `{"decision":"allow"}` exit 0 | ✅ 三值 JSON |
| confirm 弹确认 | 需要 | ✅ `permissionDecision:"ask"` → 原生确认 → 批准后执行 | ✅ 无意见（exit 0 无输出）→ 原生权限提示 | ✅ ask 转确认流程 |
| deny 阻断 | 需要 | ✅ exit 2 + stderr（工具调用不执行） | ✅ exit 2 + stderr，或 JSON deny | ✅ |
| `updated_input` 改写采纳 | 备用 | ✅ 全替换语义（echo 被改写执行） | ✅ 浅合并（配置序最后者赢；TUI 标记 `Rewrote Output`） | ✅ 采纳——Claude 式 `updatedInput` 全替换（整条命令被替换执行）；crush 式顶层 `updated_input` 信封不采纳 |
| fail-open（hook 非 2 退出） | 需要 | ✅ UI 明示 `non-blocking status code`，放行 | ✅ 其他退出码 = 非阻断放行 | ✅ exit 3 放行（M5.3） |
| hook 超时语义 | 需要 | ✅ 挂 45s > timeout 30s → ~32s 放行 | ✅ headless 实测 33s 非阻断放行 | 未测 |
| halt 整个回合 | 认知 | ❌ 无此概念 | ✅ exit 49（**引擎不使用**，保持单命令阻断统一） | ❌ |
| `PermissionRequest` 事件 | 认知 | ❌ 无同语义事件 | ❌ | ✅ 存在但 JSON 回包不被采纳（M5.3，挂点定 PreToolUse 的依据） |
| 用户选择回传 | 备用 | ❌（PostToolUse 仅执行结果） | ❌（无 post 类事件） | ❌（仅执行结果） |
| PostToolUse 事件 | 备用 | ✅ 载荷含完整 `tool_response`（权限学习信号源） | ❌ 仅有 PreToolUse | ✅ |
| 模型层命令预拦截 | 认知 | ❌ 未观测到 | ✅ banned commands 内置（curl/sudo 被劝退，更保守非缺口） | 未观测到 |
| 项目级 hooks 启用门槛 | 需要 | ❌ 无 | ❌ 无（配置即生效） | ✅ 工作区信任门（headless 不可首授，见 §4） |
| 原生模式 × hook | 需要 | 未测（§1.3） | 未测（§1.3） | ✅（见 §1.3） |

### 1.3 原生模式 × hook 交叉（按 agent）

各 agent 模式集不同，逐 agent 记录；§1.2 只留汇总行引用本节。

**zcode**（3.11.2 Desktop App 插件链路，2026-09-10 人工实测）

- 确认模式 × hook：allow **跳过原生弹窗**（变更类写操作实证）；ask 弹窗（人工批准）；deny 不弹直接阻断
- 反证实验：弹窗上点拒绝 → agent 侧收 Denied，证实弹窗人工性
- 计划模式 × hook：hook 照常评估（裁决日志增量可证）；计划模式只读分类器**短路 ask**（不弹窗直接拦）；agent 层系统硬约束禁写先于 hook（「hook allow 写操作」不可达）

**claude-code**：未测（permission_mode 交叉挂 §5）
**crush**：未测（yolo 语义有源码级核对，模式交叉挂 §5）

## 2. 版本测试结果记录

| 日期 | agent | 版本 | 触发器 | 结果 | 变更点/备注 |
|---|---|---|---|---|---|
| 2026-09-08 | claude-code | 2.1.263 | 人工交互（锚点 0 全轴） | ✅ | 三档 / `updated_input` / fail-open / 超时 全语义基线 |
| 2026-09-08 | crush | 0.92.0 | 人工交互（锚点 0 全轴） | ✅ | 三档 / `updated_input` / halt 全语义基线 |
| 2026-09-08 | claude-code | 2.1.195 | headless 对照 | ⚠️ 已废 | 「headless 不加载」系灰度窗口假象（更正史见 cairn）；版本已退出锚点 |
| 2026-09-09 | claude-code + crush | pinned | 人工 headless | ✅ | headless 正向配方定稿；crush 超时 33s 放行实测 |
| 2026-09-09 | claude-code | 2.1.263 | CI 首跑（ubuntu，pinned） | ✅ | Linux 首证：headless 正向断言通过 |
| 2026-09-09 | crush | 0.92.0 | CI 首跑（ubuntu，pinned） | ✅ | 同上（tar 安装修复后全绿） |
| 2026-09-09 | zcode | 本机 CLI | 人工（config 轨探针四轮） | ✅ | `updated_input` 采纳定论：Claude 式 `updatedInput` 全替换（整条复合命令被替换执行）；crush 式顶层 `updated_input` 信封不采纳。测试后 config 轨已退役 |
| 2026-09-10 | zcode | 3.11.2 | 人工（插件链路，确认模式） | ✅ | 确认模式 × hook 三值：allow 跳过原生弹窗（touch 变更类实证）/ ask 弹窗批准 / deny 直接阻断；反证实验（弹窗点拒绝 → agent 收 Denied）证实弹窗人工性 |
| 2026-09-10 | zcode | 3.11.2 | 人工（插件链路，计划模式） | ✅ | 计划模式 × hook：照常评估；allow 只读放行；只读分类器短路 ask（不弹窗）；agent 层硬约束先于 hook |
| 2026-09-10 | zcode | 0.16.5 CLI（App 3.11.2 内嵌） | headless `-p`（mock 驱动） | ✅ | headless 形态证实（**更正**「无 headless 形态」旧结论）；插件轨 hook 拉起 + 引擎裁决落盘实证；config 轨事件声明在 `hooks.events.*`，工作区信任门 headless 不可首授（见 §4） |
| 2026-09-10 | claude-code | 2.1.263 | CI 首跑（windows，pinned） | ✅ | windows runner 首证：matcher `Bash\|PowerShell` + mock 自适应覆盖 Windows 工具面 |
| 2026-09-10 | crush | 0.92.0 | CI 首跑（windows，pinned） | ✅ | zip 资产带版本嵌套目录（crush.exe 需归位 PATH 根，首跑 127 修 68d1055） |
| 2026-09-10 | zcode | 3.11.2（内嵌 CLI 0.16.5） | CI 首跑（windows，pinned） | ✅ | **zcode 首次入 CI**：CDN 直链 + 7z 解包取内嵌 CLI + 插件无人值守装配 + mock 驱动 headless，decisions.jsonl allow 断言通过 |

## 3. 测试如何进行

### 原理

mock LLM 后端驱动 agent 完成一轮固定 tool_use（`echo mock-hook-test`），注册的探针 hook 对该调用落盘 dump——**dump 出现 = hook 被拉起且裁决流转**（正向硬断言）。hook 链路是 agent 本地行为、与 LLM 无关，故 mock 驱动零凭证零费用。探针四角色与控制文件切实验见 design.md「hook 探针方法（定稿）」。

### 三层触发（[.github/workflows/agent-matrix.yml](../.github/workflows/agent-matrix.yml)）

- **push/PR**：pinned smoke（ubuntu 双 job + windows 三 job）+ `paths` 过滤 agent 耦合面（`src/channel/`、`plugin/`、探针/mock 脚本）——红灯归因「我方破坏」
- **weekly cron**：latest smoke——上游破坏性变更哨兵（红灯 = 上游信号）
- **workflow_dispatch**：手动指定版本回溯/排查
- **zcode windows job 配方**：extras bucket manifest 解析版本（pinned = `ZCODE_VERSION_PINNED`，latest = cron/dispatch）→ CDN 直链下载 NSIS 安装器 → 7z 两步解包取 `resources/glm/zcode.cjs`（只需 node，不装 App）→ `cargo install --path .` 供插件轨 hook 命令 → 插件无人值守装配四件套 + `enabledPlugins` 置真 → mock 驱动 `-p` → 断言 `decisions.jsonl` 落 allow 裁决
- **明确不做**：交互 TUI 自动化（脆弱，维护成本远超每版本 5 分钟人工）；agent SDK headless API（不走同一 hooks 路径）；zcode config 轨 CI 化（信任门 headless 不可首授）
- **后置**：协议回放（dump 样本驱动引擎）、bot commit 矩阵机器层（测完自动改表提交，`[skip ci]`）、zcode ubuntu job（等 Linux 公测）

### 本地复现

`uv run --directory script mock_llm.py [--port 8787] [--log 请求日志.jsonl]` 起后端；hook 注册 = `script/hook_probe.py` 四角色（crush 走项目 `crush.json` 的 providers+hooks；claude 走 `--settings` 文件 env+hooks）；判定 = dump 行数与内容。三坑与判定准则见 §4。

### 人工批 1 清单（~5 分钟，发版触发）

预设注册与控制文件 → 交互会话照单跑：`echo hi`（allow 直通）→ `curl --version`（confirm 弹窗批准后执行）→ `sudo --version`（deny 阻断）→ 控制文件切 exit 3 后任一命令（fail-open 放行）→ 读 dump 断言回填 §2。

### zcode 人工测试流程（无 CI 自动化，每版本照此走）

1. **前置**：`crush-tether` 在 PATH（`cargo install --path .`，更新引擎后重装）；插件安装 = Plugin Management → Discover → `+` → 本地目录选仓库 `plugin/` → 安装 crush-tether → **重启会话**（hook 自动武装，无需信任门）
2. **headless 冒烟**（可自动化，每版本建议加做）：`node <App安装目录>/resources/glm/zcode.cjs -p "<一句驱动 Bash 的指令>"`；模型后端写 `~/.zcode/cli/config.json` 的 `provider` + `model` 键（`model.main` 只接受 `"provider/model"` 字符串，端点与密钥在 `provider.<id>.options` 下）；hook 验证走**插件轨**（读 `.crush-tether/decisions.jsonl` 增量断言）；config 轨信任门 headless 不可首授，仅交互会话可用（见 §4）
3. **武装判定**：跑任意命令后查 `.crush-tether/decisions.jsonl` 增量——每 hook 触发记一条裁决；`type:"load"` 行为配置加载留痕
4. **三档**：`echo hi`（allow）→ `curl --version`（ask）→ `sudo --version`（deny），读日志断言
5. **`updated_input`**：插件停用 + 探针 config 轨（`.zcode/config.json` 写 `hooks.enabled: true` + `events.PreToolUse` perm 角色，指向 `script/hook_probe.py`）→ 交互会话批准信任门武装 → 控制文件 `perm-out.txt` 切信封（Claude 式 `updatedInput` / crush 式对照）→ 看执行输出是否被改写 → **测后退役 config 轨**（防与插件双轨叠跑）
6. **模式交叉**（确认模式/计划模式，人在场看弹窗）：
   - 确认模式：跑 allow/ask/deny 三类，观察弹窗——allow 应跳过弹窗、ask 应弹、deny 不弹直接挡；**区分「人工批准 vs 自动放行」用反证实验**：弹窗上点拒绝，agent 侧收到 Denied 即弹窗为真
   - 计划模式：hook 照常评估（日志增量可证）；计划模式只读分类器会**短路 ask**（不弹窗直接拦）；agent 层被系统硬约束禁写，「hook allow 写操作」不可达
7. **版本记录**：ZCode Desktop App 版本随测随记入 §1.1/§2（当前 3.11.2，内嵌 CLI 版本轨道独立以 `zcode.cjs version` 为准，当前 0.16.5）

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
| 8 | zcode 探针 config 轨：hook 在命令**执行前**读控制文件（天然差一拍），且 `updatedInput` 全替换会把同调用内的写文件操作一并废掉 | 改控制文件用**非 Bash 工具**（hook matcher 只匹配 Bash）；每轮只发纯探测命令 |
| 9 | zcode：非交互入口不在 PATH——CLI 是 App 内嵌 bundle（`resources/glm/zcode.cjs`），版本轨道与 App 版本号独立（App 3.11.2 / CLI 0.16.5） | `node <app>/resources/glm/zcode.cjs -p "<指令>"`；双版本号分别记录 |
| 10 | zcode config 轨：事件声明在 `hooks.events.<Event>` 下（非插件 envelope 的 `hooks.<Event>`，写错仅日志 `config.file.invalid` 提示）；项目 hooks 需工作区信任，信任由 **capable host**（Desktop App UI）授予、按 工作区+声明 digest 持久化于 `~/.zcode/security/workspace-hook-trust-v1.json`——headless **不可首授** | headless/CI 一律走**插件轨**（免信任，实测 hook 可拉起）；config 轨仅交互会话用 |
| 11 | zcode：Anthropic SSE `content_block_start` 的 `input` 字段严格校验（须为对象），mock 回空串即整回合失败（`AI_TypeValidationError`） | mock 固化版已修正为 `"input": {}`（claude/crush 对空串宽容） |

## 5. 待补测

- claude-code **交互 + 全放行形态**（`--dangerously-skip-permissions` / `allowedTools:["*"]`）下 hook 是否仍被评估——社区「权限管道跳过」假说（zcode 侧已有同构结论：hook 评估先于原生权限并可覆盖 yolo）
- exit 2 与 JSON 回包并发时的覆盖规则——**上游聚合语义引用（halt > deny > allow），非我方行为面**，仅可选抽查以验证 design.md 契约节引用的准确性
- claude-code 非默认 permission_mode（plan/bypassPermissions 交互）× hook 交叉
- crush 原生确认/计划模式 × hook 交叉（yolo 语义已有源码级核对，模式交叉未实测）
- zcode headless 全轴：三档语义（现 mock 只发固定 `echo`，deny/confirm 需扩展 mock 或换规则）、`updated_input`、模式交叉在 headless 形态下的表现
- zcode ubuntu job：Linux 版内测中，公测后补（发行渠道落地即可平移 windows job 配方）
- zcode cron latest 哨兵的首个自动触发尚待观察（每周一 UTC）

已收口的项不再留痕于 §5（测完即删，过程史在 cairn）。

## 6. 维护规则

本节是本文档的维护口径，改动 §1/§2 前先读。

### 6.1 §1.1（版本结论表）

- 只维护当前状态，格子只记通过/未通过，细则见 §1.1 注 1–5（新版本实测后补行、单 agent 新版其它格留空、latest 未实测不记录、pinned 红 = 我方破坏 / latest 红 = 上游信号）

### 6.2 §1.2（能力快照）——版本演进

- **列头 = §1.1 当前通过版本**。agent 出现实测通过的新版本时：
  - 能力面**有差异** → 该 agent **开新列**，逐格填新版本结论
  - 能力面**无差异** → 不开新列，当前列头并入版本区间（如 `2.1.263–2.1.264`）
  - 两种情况 §2 都必须记一行流水（日期/版本/触发器/结果/变更点）——**§2 是全文档唯一的版本时间维度**
- **换版本复核分层**（不是每格都重测）：
  - CI 自动兜底：headless 加载、allow 直通（pinned bump 后每次 push 跑，cron latest 常态盯）
  - 人工批 1 清单（~5 分钟，发版触发）：confirm 弹窗、deny 阻断、fail-open
  - 专项实验行（`updated_input`/超时/halt/`PermissionRequest`/回传/PostToolUse/预拦截/模式交叉）：**不随版本主动重测**，格子语义 = 「最后一次实测的结论随列头推进」；cron 哨兵红灯或上游大版本才触发专项复测，出差异即改格 + §2 流水
- **需要性标注**（「需要性」列，对当前引擎版本而言）：
  - `需要` = 交付/部署直接依赖；`备用` = 契约面或未来特性信号源，当前未用；`认知` = 验证过的行为差异或不采用的决策记录
  - 本项目引擎自身出现版本分化、能力需要性随引擎版本不同时，**另维护一张「能力 × 引入版本」表**（哪个引擎版本引入/移除对该能力的需要），不在本表混记
- **新增能力行默认填「未测」**；追溯测试随时可做（`workflow_dispatch` 指定老版本跑 CI + 人工批测交互语义），但上游发行资产可回装无永久承诺，追溯宜早

### 6.3 CI 覆盖边界（谁测什么）

- **CI 机测面（push/PR/cron 全部 job）**：headless 下 hook 被拉起 + allow 裁决流转——mock 单发固定 `echo`、引擎默认规则 allow，断言探针 dump（claude/crush）或 `decisions.jsonl`（zcode）
- **不在 CI 内**：confirm 弹窗（人工性本质不可机测）、fail-open、`updated_input`、超时、模式交叉、zcode config 轨（信任门 headless 不可首授）——由人工批 1 / 专项实验维护
- deny 路径**可以**机测（规则侧把 `echo` 设 deny 即可无 UI 断言），未立项，待拍板
