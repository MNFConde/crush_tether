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
| **headless 加载 hooks** | **✅ 条件性**（灰度使能后 `-p` 全语义正常执行；cold 窗口内缺席，见 headless 节更正） | ❌ `crush run` 不执行（**源码+日志实锤**：交互 TUI 5 条 `Hook completed` INFO vs run 零 hook 日志——`runner.Run` 未被调用；三种注册途径含 crushrc `hook add` builtin 全无效；0.92.0 run 路径未接线） | n/a（无 headless 形态） |
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
   - **crush 侧定性不同（跨日复测）**：`crush run` 在 claude 侧灰度已使能的次日复测**仍不执行 hooks**（dump 零记录、mock 对话正常）——非灰度窗口，0.92.0 run 模式的固有行为（hooks 大概率仅在交互 TUI 会话装配）。两 agent 的 headless 定性因此分开：claude-code = 条件性（灰度使能后可用）；crush = run 模式固定不执行。
3. **`CLAUDE_CODE_ENABLE_FUNCTION_HOOKS` 与加载与否无关**（③ 带此 env 照常执行；其真实作用是控制插件 hooks 模块加载，二进制考古所得）。
4. **`--settings` 不屏蔽 hooks（补验证实）**：flag 使能后带 `--settings`（含 hooks 键）重跑——`Slow PreToolUse hooks (2 hooks)`，`--settings` 的 hooks 与项目级 hooks **聚合并存执行**（同 command 各自计一个，串行各 30s timeout 后放行）。下午不跑的唯一解释即灰度窗口。
5. **方法论教训（已并入探针方法实践）**：hook 是否执行的判定 = dump 物理副作用优先，日志计数仅作参考；`Registered/Found 0 hooks` 在 hooks 实际执行时仍打印 0，是本次误判的直接原因。

- **部署含义（修正）**：headless（CI/脚本/自动化）权限门**可用但使能状态随灰度翻动**——新装环境/冷缓存窗口内静默缺席。部署验收仍须实测 hook 确实触发（且注意验收时点与灰度状态相关）。
- **对 CI 的含义（修正）**：headless hooks 探测哨兵从「负向探测」升级为**双向探测**——dump 出现与否都记录，跟踪灰度状态翻动；协议回放与冒烟层不变。
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
- crush hook 超时的实机验证（文档语义：cancel → 非阻断放行）
- claude-code 非默认 permission_mode（plan/bypassPermissions 交互）× hook 交叉；zcode 计划模式交叉（原 M7.3 待补两项）
- zcode `updated_input` 采纳
