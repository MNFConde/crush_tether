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
| **headless 加载 hooks** | **❌ `-p` 注册表恒 0**（2.1.195/2.1.263 一致） | **❌ `crush run` 不执行**（同文件 providers 生效证配置被读） | n/a（无 headless 形态） |
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

### 1. headless/非交互模式下 hooks 静默失效（两 agent 一致）

`claude -p`（含 `--settings`/项目级/用户级全配置路径、`CLAUDE_CODE_ENABLE_FUNCTION_HOOKS=1` 开关开启前后、`--include-hook-events` 观察确认零 hook 事件）与 `crush run`（项目级/全局级、matcher 有无、探针/inline 命令全组合）下 hooks 均不加载/不执行，且**无任何告警**——agent 正常对话并执行工具。已排除：`--bare`（本机所有 `-p` 实验均未加 `--bare`）、环境变量污染（会话内无 CLAUDE/ANTHROPIC 变量）、配置格式（同一配置在交互会话全部生效）。

- **与官方文档的矛盾（待跟进）**：官方 `--bare` 文案（"skip hooks"）与社区资料均指向「普通 `-p` 应加载 hooks」；官方 hooks 文档亦承诺 settings hooks 在未信任目录的 `-p` 运行中执行。本机实测（2.1.195/2.1.263）与此矛盾——疑似 2.1.x hooks 重构引入的门控或回归（如 rollout flag 未对本机灰度；`tengu_plugin_hooks_modules` 默认 false，GrowthBook 冷缓存无 payload）。精确条件需官方侧信息（候选动作：提 GitHub issue 附本矩阵复现步骤）。
- **失效层辨析**：社区将 headless 失效归因于「权限完全放行（`--dangerously-skip-permissions` / `allowedTools:["*"]`）时跳过权限检查管道，hook 阻断依赖该管道故失灵」——该解释描述的是「**hook 已运行但回包被忽略**」层。本仓库实测证据指向更上游：headless 下 **hook 进程根本未被拉起**（探针 dump 零记录、claude debug 日志 `Found 0 total hooks in registry`、crush 同配置文件 providers 生效而 hook 零痕迹），与权限模式参数无关。两层可能并存（bypassPermissions 交互形态未实测，见待补测）。物理阻断类 workaround（hook 内做 chmod 等副作用代替 exit 2）只对「hook 已运行但回包被忽略」层有效，对「hook 不被拉起」层**无效**；且其前提（hook 可执行任意物理副作用）本身扩大信任面，本门不采用。测试方法论上的「可验证副作用」思路已在使用：探针 dump.jsonl / hook-fired 文件即 hook 触发的物理证据。
- **部署含义**：CI、脚本、自动化、任何非 TTY 场景中本权限门**当前静默缺席**（以实测为准，无论文档承诺）——部署验收的「确认 hook 确实触发」必须在**交互会话**形态下进行。
- **对 CI 矩阵的影响**：agent 的 hooks 行为无法在 CI（无 TTY）自动化实测——本矩阵 hooks 行格的数据来源 = 人工交互实测；CI 保留「headless hooks 有效性探测」（无 hook 冒烟 + 探针注册探测，该哨兵对「文档承诺」与「实测失效」两种解释都稳健：若未来 agent 使 headless hooks 生效，dump 出现即自动发现并报警扩展 CI 矩阵）。

### 2. 三档裁决语义两侧一致且与 design.md 契约节吻合

allow 直通 / confirm 走原生确认（claude 经 `ask` 信封，crush 经无意见）/ deny 阻断（两侧均为 exit 2 + stderr 路径）。契约节实测坐实。

### 3. `updated_input` 在两侧真实生效

hook 回包可改写命令（模型无感知，实际执行改写后命令）。权限门视角：我方引擎的 `updated_input` 输出（若有）在两侧真实生效；同时这也是**风险面**——任意 hook 都能改写命令，部署时 hook 脚本本身在信任边界内。

### 4. crush 的模型层预拦截与其两级阻断

crush 的 system prompt 内置 banned commands 规则（curl/sudo 等），confirm/deny 类命令常在工具调用前被模型自行劝退（方向更保守，非安全缺口，但实测时需换黑名单外命令才能触达 hook 层）。阻断两级：exit 2 = 阻断单命令回合继续（**引擎采用，与 claude/zcode 行为统一**）；exit 49 = halt 整回合（agent 特有能力，引擎不使用）。

### 5. 权限学习可行性（顺带评估）

跨 agent 一致信号源 = PostToolUse：claude-code 载荷含完整 `tool_response` 与 `tool_input`；zcode 仅执行结果；crush 无 post 事件。suggest 候选在 claude-code 信号最全，crush 侧不可行（无信号），维持「不支持的 agent 上默认不生效」的降级原则。

## 测试方法与自动化策略（headless 受限下的设计）

前提事实：hooks 行为仅在交互（TTY）会话可实测（见「headless 失效」节）。agent 兼容性测试按可自动化性拆三层：

### CI 常态层（全自动，cron 驱动，零凭证）

1. **协议回放**：用实测攒下的 dump.jsonl 载荷样本（带 agent 版本标注）直接驱动探针+引擎，断言回包契约（信封格式 / 三值 / exit code）——防我方回归，不测 agent；样本漂移靠版本标注显性化。
2. **agent 活性冒烟**：mock LLM 后端驱动最新版 agent 完成一轮对话——确认能装、能起、协议未崩。
3. **headless hooks 负向探测**：注册探针 + headless 跑一发，断言 dump 不出现；dump 一旦出现 = agent 行为变化，自动报警并扩展 CI 矩阵（对「文档承诺」与「实测失效」两种解释都稳健的哨兵）。
4. **版本与变更追踪**：cron 拉 npm / GitHub Releases 最新版本号 + release notes 抓 hook 相关关键词 → 有信号才触发下一层。

### 发版触发层（人工 ~5 分钟，清单化）

新版本或 hook 相关变更信号出现时执行：预设注册与控制文件 → 一次交互会话照清单跑命令 → 读 dump 断言入矩阵。清单（以 claude-code 为例，crush 同构）：`echo hi`（allow 直通）→ `curl --version`（confirm 弹窗，批准后执行）→ `sudo --version`（deny 阻断）→ 控制文件切 exit 3 后任一命令（fail-open 放行）。

### 大版本层（人工 ~15 分钟，低频）

大版本或疑似破坏时跑全轴：`updated_input` 改写、超时挂起、halt（crush exit 49）、双探针聚合、非默认 permission_mode 交叉。

### 明确不做

交互 TUI 自动化（ConPTY/winpty 驱动）——脆弱、维护成本远超每版本 5 分钟人工；agent SDK 的 headless API 走的不是同一 hooks 路径——自动化测错形态比人工测对形态更糟。

### bot commit 边界

矩阵机器层（版本号、headless 探测结果、协议回放结果）可由 bot commit（scoop 模式：`[skip ci]` + 限定路径 + 最小权限）；hooks 行为行标注「人工交互实测 + 版本/日期」，永远人工维护。

## 版本记录

| 版本 | 结论 | 日期 | 方式 |
|---|---|---|---|
| claude-code 2.1.263 / crush 0.92.0 | 本矩阵全部实测行 | 2026-09-08 | 人工交互实测（探针） |
| claude-code 2.1.195 | headless hooks 不加载（与 .263 一致） | 2026-09-08 | headless 对照 |
| 历史版本回溯 | 未测（按需二分） | — | CI/人工 |

## 待补测

- claude-code **交互 + 全放行形态**（`--dangerously-skip-permissions` / `allowedTools:["*"]`）下 hook 是否仍被评估——社区「权限管道跳过」假说的直接验证（zcode 侧已有同构结论：hook 评估先于原生权限并可覆盖 yolo）
- exit 2 与 JSON 回包并发时的覆盖规则（契约：exit 2 覆盖 JSON；未单独实测）
- crush hook 超时的实机验证（文档语义：cancel → 非阻断放行）
- claude-code 非默认 permission_mode（plan/bypassPermissions 交互）× hook 交叉；zcode 计划模式交叉（原 M7.3 待补两项）
- zcode `updated_input` 采纳
