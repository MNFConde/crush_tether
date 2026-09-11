# agent 兼容性矩阵（M7.3）

> **定位**：只记确定性测试**结果**——兼容性矩阵、版本测试结果（带更新时间）、维护口径（§3）。测试方法、CI 设计、agent 差异与规避、挂账规划一律在 [test-and-ci.md](test-and-ci.md)；排查过程、错误结论与更正史在 cairn/（LOG、ROADMAP、[agent-hook-testing](../cairn/agent-hook-testing.md)），不进本文档。
> **覆盖口径**：交互全语义（三档弹窗 / `updated_input` / halt / fail-open / 超时）= Windows 手工会话（锚点 0 人工批测）；headless hook 冒烟（正向断言）= CI runner（ubuntu 双 job：claude/crush；windows 三 job：claude/crush/zcode，pinned 每次 push/PR、latest 每周 cron）+ Windows 本机预演。除 [test-and-ci.md §2](test-and-ci.md#2-agent-差异与规避) 另注明外，结论均在 Windows 10 x64 实测。

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

> 列头 = §1.1 当前通过版本；换版本的更新动作（差异开新列/无差异并区间）、需要性标注口径、复核分层见 **§3 维护规则**。未复核格以 §2 最新流水为准。

| 能力 | 需要性 | claude-code 2.1.263 | crush 0.92.0 | zcode 3.11.2（Desktop App） |
|---|---|---|---|---|
| 交互会话加载 hooks | 需要 | ✅（`/hooks` 显示注册数） | ✅（TUI 显示 `Hook hook-probe → OK`） | ✅（M7 前置） |
| headless 加载 hooks | 需要 | ✅ 可钉死（`DISABLE_GROWTHBOOK=1`，内置默认=开） | ✅ 无条件（配置即生效） | ✅ `-p` 非交互（内嵌 CLI 0.16.5，mock 驱动全回合；hook 走插件轨可拉起，config 轨信任门 headless 不可首授，见 [test-and-ci.md §2](test-and-ci.md#2-agent-差异与规避)） |
| allow 直通 | 需要 | ✅ `permissionDecision:"allow"` exit 0 | ✅ `{"decision":"allow"}` exit 0 | ✅ 三值 JSON |
| confirm 弹确认 | 需要 | ✅ `permissionDecision:"ask"` → 原生确认 → 批准后执行 | ✅ 无意见（exit 0 无输出）→ 原生权限提示 | ✅ ask 转确认流程 |
| deny 阻断 | 需要 | ✅ exit 2 + stderr（工具调用不执行） | ✅ exit 2 + stderr，或 JSON deny | ✅ |
| `updated_input` 改写采纳 | 备用 | ✅ 全替换语义（echo 被改写执行） | ✅ 浅合并（配置序最后者赢；TUI 标记 `Rewrote Output`） | ✅ 采纳——Claude 式 `updatedInput` 全替换（整条命令被替换执行）；crush 式顶层 `updated_input` 信封不采纳 |
| fail-open（hook 非 2 退出） | 需要 | ✅ 交互放行（UI 明示 non-blocking）；**无头拒绝**（`permission_denials`，2026-09-11，[test-and-ci.md §2](test-and-ci.md#2-agent-差异与规避) 差异 13） | ✅ 无头一致：其他退出码 = 非阻断放行 | ✅ 无头一致：exit 3 放行（M5.3） |
| hook 超时语义 | 需要 | ✅ 挂 45s > timeout 30s → ~32s 放行 | ✅ headless 实测 33s 非阻断放行 | 未测 |
| halt 整个回合 | 认知 | ❌ 无此概念 | ✅ exit 49（**引擎不使用**，保持单命令阻断统一） | ❌ |
| `PermissionRequest` 事件 | 认知 | ❌ 无同语义事件 | ❌ | ✅ 存在但 JSON 回包不被采纳（M5.3，挂点定 PreToolUse 的依据） |
| 用户选择回传 | 备用 | ❌（PostToolUse 仅执行结果） | ❌（无 post 类事件） | ❌（仅执行结果） |
| PostToolUse 事件 | 备用 | ✅ 载荷含完整 `tool_response`（权限学习信号源） | ❌ 仅有 PreToolUse | ✅ |
| 模型层命令预拦截 | 认知 | ❌ 未观测到 | ✅ banned commands 内置（curl/sudo 被劝退，更保守非缺口） | 未观测到 |
| 项目级 hooks 启用门槛 | 需要 | ❌ 无 | ❌ 无（配置即生效） | ✅ 工作区信任门（headless 不可首授，见 [test-and-ci.md §2](test-and-ci.md#2-agent-差异与规避)） |
| 原生模式 × hook | 需要 | 未测（§1.3） | 未测（§1.3） | ✅（见 §1.3） |

### 1.3 原生模式 × hook 交叉（按 agent）

各 agent 模式集不同，逐 agent 记录；§1.2 只留汇总行引用本节。

**zcode**（3.11.2 Desktop App 插件链路，2026-09-10 人工实测）

- 确认模式 × hook：allow **跳过原生弹窗**（变更类写操作实证）；ask 弹窗（人工批准）；deny 不弹直接阻断
- 反证实验：弹窗上点拒绝 → agent 侧收 Denied，证实弹窗人工性
- 计划模式 × hook：hook 照常评估（裁决日志增量可证）；计划模式只读分类器**短路 ask**（不弹窗直接拦）；agent 层系统硬约束禁写先于 hook（「hook allow 写操作」不可达）

**claude-code**：未测（permission_mode 交叉挂 [test-and-ci.md §5](test-and-ci.md#5-测试规划与挂账)）
**crush**：未测（yolo 语义有源码级核对，模式交叉挂 [test-and-ci.md §5](test-and-ci.md#5-测试规划与挂账)）

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
| 2026-09-10 | zcode | 0.16.5 CLI（App 3.11.2 内嵌） | headless `-p`（mock 驱动） | ✅ | headless 形态证实（**更正**「无 headless 形态」旧结论）；插件轨 hook 拉起 + 引擎裁决落盘实证；config 轨事件声明在 `hooks.events.*`，工作区信任门 headless 不可首授（见 [test-and-ci.md §2](test-and-ci.md#2-agent-差异与规避)） |
| 2026-09-10 | claude-code | 2.1.263 | CI 首跑（windows，pinned） | ✅ | windows runner 首证：matcher `Bash\|PowerShell` + mock 自适应覆盖 Windows 工具面 |
| 2026-09-10 | crush | 0.92.0 | CI 首跑（windows，pinned） | ✅ | zip 资产带版本嵌套目录（crush.exe 需归位 PATH 根，首跑 127 修 68d1055） |
| 2026-09-10 | zcode | 3.11.2(内嵌 CLI 0.16.5) | CI 首跑(windows,pinned) | ✅ | **zcode 首次入 CI**:CDN 直链 + 7z 解包取内嵌 CLI + 插件无人值守装配 + mock 驱动 headless,decisions.jsonl allow 断言通过 |
| 2026-09-11 | claude + crush + zcode | pinned(本机) | 无头场景组首测(deny/fail-open/rewrite) | ✅ | deny/rewrite 三家与交互定性一致;fail-open 分化:claude 无头拒绝(`permission_denials`)/crush、zcode 放行(见 [test-and-ci.md §1](test-and-ci.md#1-测试方法) 场景组);附带定性:zcode 无头对 confirm=拒绝、zcode spawn hook 精简 env(`uv` 不可用,差异 12) |

## 3. 维护规则

本节是本文档的维护口径，改动 §1/§2 前先读。

### 3.1 §1.1（版本结论表）

- 只维护当前状态，格子只记通过/未通过，细则见 §1.1 注 1–5（新版本实测后补行、单 agent 新版其它格留空、latest 未实测不记录、pinned 红 = 我方破坏 / latest 红 = 上游信号）

### 3.2 §1.2（能力快照）——版本演进

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
