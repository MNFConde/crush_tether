# design.md — crush_tether 设计文档

`crush_tether` 是 Crush 的**命令级 bash 权限门**：通过 PreToolUse hook 对每条 bash 命令做三档分类（`allow` / `confirm` / `deny`），返回给 Crush 决定是否放行。本文件是 crush-guard 抽取/重写的设计单一事实源；历史结论与教训见 `cairn/crush-guard-bash-gate.md` 和 `cairn/crush-guard-retrofit-options.md`。

## 目标

把 `mdor` 仓库内的 `crush-guard/`（Python + bashlex）独立化并（可选）用 Rust 重写，作为可复用、可配置的 bash 权限门，供本仓库和/或其他仓库（及多 agent）共用。除三档分类外，另含**可配置规则引擎**（TOML 声明层 + Rhai/Lua 脚本层）与**多 Agent 适配层**（Crush / ClaudeCode 首发）。**二进制为纯引擎、零内置策略**：一切规则（含默认规则）均来自外部配置文件与脚本，见 [规则引擎与配置（定稿）](#规则引擎与配置定稿)、[零内置策略与默认配置生成（定稿）](#零内置策略与默认配置生成定稿)。

## 背景与关键约束

- 本仓库 `.crushrc` 用 PreToolUse hook 挂 `crush-guard/guard.py`（bashlex AST）对 bash 命令做三档分类。
- **瓶颈在 Python 冷启动**（`import bashlex` ~99ms，总时延 ~98-147ms），与启动形态无关；`uv tool install` 的 console script 并不更快。
- **命令级粒度只能靠自定义 hook**：Crush 内置 `permissions allow/deny` 只匹配完整工具名，`safeCommands`/`bannedCommands` 写死且为前缀匹配，存在三类绕过（`echo hi && rm -rf`、`find -delete`、`git show --output=.git/config`）。
- **hook 与语言无关**（子进程、读 stdin/env、输出 allow/deny/none），Go 不会让集成更简单；Rust 可行（`tree-sitter-bash`，启动 ~2ms、单 `.exe`）。

## 三档分类语义

| 档位 | 含义 | 返回 |
|---|---|---|
| **allow** | 只读 / 仓库内安全写 | exit 0，输出 `{"decision":"allow"}` |
| **confirm** | 有风险、可逆、需人工确认 | exit 0，不输出 JSON（走正常权限提示） |
| **deny** | 不可逆 / 破坏性 | exit 2（硬阻断） |

分类对象是 AST 拉平后的每条简单命令（`gather_commands`），因此 `echo hi && rm -rf /` 这类复合命令会被拆开逐条分类，不会因前缀匹配 `echo` 而漏掉 `rm`。

> 核心洞见：**用 AST/语义而非正则前缀匹配**。专堵两个已知洞——`echo hi && rm -rf /` 与 `git show --output=.git/config HEAD`。

## 判定表（纯语义，可 1:1 平移）

> 落点变更（2026-09-04 定稿）：本节判定表从「编译进 engine.rs 的代码常量」改为**默认规则数据**（默认 `rules.toml` + `rules.rhai`，由二进制生成到项目侧）。语义本身不变；1:1 平移的验收载体是回归测试 + 生成出的默认配置。

- `DESTRUCTIVE`（sudo/rm/mkfs/...）→ deny；`GIT_DESTRUCTIVE`（reset/clean/rebase/push/pull/...）→ deny。
- `GIT_READONLY`（status/log/diff/show/...）→ allow，但若带 write flag（`--output`/`--pretty`/`-c` 等）转 confirm。
- `GIT_SAFE_WRITE`（add/commit/checkout/switch/mv/rm）→ allow（仅当路径不逃逸仓库）。
- `READONLY` 目录（ls/cat/grep/rg/find/...）→ allow，但若带 write flag 或**写文件重定向**（`>`/`>>` 到真实路径）转 confirm；丢弃式重定向（`2>/dev/null`/`2>&1`/`>/dev/null`）不转。
- 构建/工具链（cargo/go/gofmt/black/ruff/make/just/mkdir/touch）→ allow（路径逃逸时 confirm）。
- 默认（npm install/publish、pip、python -c、curl/wget 等）→ confirm；`curl|sh` → deny。

### 关键边界（实测确认）

- `has_writing_redirect`：只对**真实写文件**的重定向返回 True（`>/>>/>|/<>` 且目标非 `/dev/null`/`NUL`/`null`/`-`）；输入型 `<`/`<<`/`<<<`、fd dup `2>&1`、fd close `2>&-` 均无写副作用，保持 allow。
- `find_mutates`：`find` 带 `-delete/-exec/-execdir/-ok` 时转 confirm；纯读 `find . -type f` 仍 allow。
- git 两态子命令（branch/remote/tag/config）：只有写形态才 confirm，纯读保持 allow。
- heredoc 无终止符时 bashlex 抛 `ParsingError` → `decide` 走 `unparseable` 分支返回 confirm（安全，不误放行）。

## 结构设计

### 目标结构（单 crate，lib/bin 双入口）

```
crush_tether/
├── Cargo.toml            # 单 crate：同时提供 lib 与 bin 两种入口
├── src/
│   ├── lib.rs            # 库入口：核心逻辑（model/engine/config/cmd_parse/channel），可被复用/单测
│   ├── main.rs           # bin：装配壳（check 模式已落地；serve 随 P4）
│   ├── channel.rs        # agent 适配层（Crush / ClaudeCode 契约；stdin JSON/env → 裁决输出）
│   ├── engine.rs         # 规则管线 + 安全原语/特征 + pipe sink + 组合裁决（零内置策略）
│   ├── config.rs         # 三层加载/merge + 默认配置生成（rules.toml 反序列化；随 P2）
│   ├── model.rs          # Decision / Verdict（combine 组合语义）/ unparseable 兜底
│   └── cmd_parse.rs      # tree-sitter-bash 解析 + flatten + 写重定向/路径逃逸检测
├── tests/
│   └── guard_regression.rs  # test_guard.py 89 用例 1:1 平移 + 解析器边界单测
├── .cairn/               # Cairn 配置
├── cairn/                # Cairn 知识层
└── doc/design.md         # 本文档
```

> 说明：本工具为单一二进制 hook，核心逻辑（分类/配置/规则/适配）**无第二消费方**，故不拆 workspace（省去 resolver/共享依赖钉版/跨 crate 开销）。但为保留核心逻辑的可复用性与干净单测，采用**单 crate + `src/lib.rs` + `src/main.rs` 双入口**——库入口装 `model`/`engine`/`cmd_parse` 等分类逻辑，`main.rs` 仅做装配。逻辑模块间靠契约边界划分（`module-boundary-contract-design.md` 思想），不在目录上拆 crate。

### 运行架构图（进程拓扑与模块分层）

进程拓扑（正常路径）:

```text
Agent（Crush / ClaudeCode）
  │ PreToolUse：每条 bash 命令 spawn 一次
  ▼
crush-tether hook（二进制本体 · 短命进程 · client 角色）
  ① channel 解析（全系统唯一知道 agent 契约的入口）
  ② connect 端点（µs）─ 成功 ─▶ 一行协议收发 ─▶ 按契约 emit 裁决 ─▶ exit
  ③ 失败 ─▶ detached spawn serve + 有界等就绪 + 重试 connect（~200ms 预算）
  ④ 仍失败 ─▶ 降级本进程全量管线（check 路径），绝不无裁决放行
  │ 命名端点 crush-tether-<hash(项目根, engine)>（每项目一实例 · ACL 限当前用户）
  ▼
crush-tether serve（二进制本体 · detached 常驻 · server 角色）
  · 启动第一动作 = 独占创建端点：成功 = 唯一服务；失败 = 已存在 → 静默退出
  · 串行 accept：请求 → 匹配 Arc<RuleSet> 快照 → 应答
  · last_activity 归零 + 空闲超 grace → exit（崩溃自愈：下一条命令由 hook 重拉）
  · 三层配置 → merge → 编译不可变快照；notify + debounce → 整段重编译换指针
  · DSL（Rhai/Lua）沙箱在本进程内执行，永不以 OS 进程形态存在

check（兜底/冒烟）：同一二进制单发，不碰端点，本进程全量 Parse → … → Verdict
```

模块分层（依赖方向单向，编译期可见性强制）:

```text
装配层  main.rs + cli/        子命令分发（hook / serve / check），仅此层知道三种角色
适配层  channel/  service/    agent 契约适配；端点监听与 connect-or-spawn 客户端、热重载、idle 退出
核心层  engine/  cmd_parse/   规则管线与裁决组合；tree-sitter-bash 解析与特征提取
        config/               三层加载 merge、TOML/DSL 编译进 Arc<RuleSet>
类型层  model/                Cmd / Verdict / Decision

依赖方向：model ← cmd_parse/engine/config ← channel/service ← cli
```

### 分类器输入/输出契约（hook 协议）

- 输入：stdin（命令正文）+ 环境变量 `CRUSH_PROJECT_DIR` / `CRUSH_TOOL_INPUT_COMMAND`。
- 输出：如上三档分类语义表。

## 规则引擎与配置（定稿）

> 本节描述 crush-tether 的**可配置规则引擎**、**配置分层**、**脚本 DSL** 与 **Agent 适配层**。本节为这些主题的单一事实源，其余文档只回指、不复述。

### 配置分层与优先级（定稿）

- 解析优先级：**项目 > 用户 > 全局**（高层覆盖低层，不粘性）。`deny` 不被全局粘性锁定，均可被高层覆盖，属门卫而非沙箱。**三层同时存在时，效力顺序仍是 项目 > 用户 > 全局**；「三层皆缺」才触发项目侧默认配置生成（见 [零内置策略与默认配置生成（定稿）](#零内置策略与默认配置生成定稿)）。
- 显式覆盖：`--config <path>` 或环境变量 `CRUSH_TETHER_CONFIG`，优先级高于所有层。
- 项目配置最可靠来源为 `CRUSH_PROJECT_DIR`（Crush 对 hook 注入，且为路径逃逸检查基准）；缺失时从 cwd 逐级上溯最近 `.git` 或 `.crush-tether/`。
- merge 语义：标量覆盖；命令集合用并集（只增不减，`exclude` 表可显式剔除）；`[[rules]]` 在高层**前插**（first-match-wins）。
  - **【已替换】（2026-09-05 草案更正）**：merge 语义与规则链已被[配置格式与脚本边界（草案 v1）](#配置格式与脚本边界草案-v1)替换——命令集合并集 → token 级高层胜出低层补缺；`[[rules]]` 规则链 → `[local]`/`[global]` 双表三桶查表。待 P2 实现验收后升格定稿。

### 配置拆分（定稿）

```text
.crush-tether/                 # 项目级配置目录
├── rules.toml                 # 声明层：数据/默认值/命令集合/简单 when→decision 规则/security
└── rules.rhai 或 rules.lua    # 脚本层：跨命令逻辑/自定义谓词/fn 规则（按 --engine 选后缀）
```

用户级 `~/.config/crush-tether/`、全局（系统路径；其默认文件由命令生成，后期设计）与项目同构。脚本层**同文件按优先级**：项目脚本最后执行，可作最终裁决。

### 零内置策略与默认配置生成（定稿）

> 定稿（2026-09-04）：**软件本身不提供任何规则**——二进制只实现引擎能力（解析/flatten/特征提取/安全原语/管线/组合裁决），不含一行策略数据。默认策略也由**外部配置文件 + 脚本**提供，以项目侧生成的形态落地。

- **默认策略 = 外部数据**：默认 `rules.toml`（能声明表达的：命令集合、flag 前缀、位置参数数、特征布尔等）+ 默认 `rules.rhai`（声明层表达不了的跨参数逻辑：`find` 突变检测、`git config` ≥2 位置参数写判定等）以**模板内嵌于二进制**——模板只是生成源数据，不参与判定，不构成内置策略。
  - **【已替换】（2026-09-05 草案更正）**：默认 `rules.toml` 的内容界定已由[配置格式与脚本边界（草案 v1）](#配置格式与脚本边界草案-v1)收窄——无条件的纯查表进 TOML（不再有「flag 前缀」「位置参数数」这类声明层字段），一切条件判断下沉脚本层；默认包的具体结构以草案 v1 为准，待 P2/P3 验收后升格。
- **生成触发**：按层寻找配置（全局 → 用户 → 项目，含 `--config`/`CRUSH_TETHER_CONFIG` 显式指定）后**三层合并仍得不到任何有效配置**时，才在项目 `.crush-tether/` 写出默认 `rules.toml` + `rules.rhai`；**任一层存在有效配置即尊重现状，不生成**（避免将来全局/用户自定义被项目层默认值遮蔽）。
- **损坏 ≠ 缺失**：配置文件存在但解析失败时，先把原文件改名留档（`rules.toml.bak-<时间戳>`）再生成默认，并 stderr 告警——满足「损坏也重生成默认」，但绝不覆盖销毁用户手改内容。
- **全局/用户层生成延后**：v1 只做项目层生成；全局/用户层默认文件由命令提供（如 `crush-tether init --global`，后期设计）。
- **引导豁免**：生成动作本身是管线引导步骤，不经规则链判定；只写 `.crush-tether/` 下固定文件名，不触碰其他路径。
- **幂等与原子**：模板内容恒定 → 重复生成结果一致（幂等）；落盘 temp + rename 原子替换，多 hook 并发发现缺失时天然收敛到同一结果。
- **fail-safe 衔接**：生成完成前 / 生成失败时按既有 fail-safe 处理（unparseable → confirm），绝不放行。
- **测试落点**：89 条回归用例改为「引擎 + 默认规则 fixture」驱动——默认配置文件本身成为验收对象（生成出的默认包必须完整复现原判定表语义）。

### 运行模式与配置热重载（定稿）

#### 三种运行模式

二进制本体承担三个子命令角色，agent 配置只写 `hook`：

```text
crush-tether hook [--agent crush --engine rhai]      # agent 配置入口：connect-or-spawn，失联降级单发（默认）
crush-tether serve [--agent crush --engine rhai]     # 常驻：命名端点监听，由 hook 进程自动拉起，无手动场景
crush-tether check [--agent crush --engine rhai]     # 单发：stdin JSON → stdout/exit code（现 hook 契约，兜底 + 冒烟测试）
```

| 模式 | 触发方 | 进程 | 配置解析次数 | 适用 |
|---|---|---|---|---|
| hook（默认） | agent 的 PreToolUse 配置直配二进制 | 每命令一次（短命 client） | serve 已加载则 0 次 | 正常使用 |
| serve（常驻） | hook 进程 connect 失败时自动 detached 拉起 | 长驻后台 | 文件变化时重载 | 正常使用 |
| check（单发） | hook 降级路径 / 手动 | 每命令一次 | 每次全量 | 兜底路径 / 冒烟测试 / 模式验收 |

- **check 先行**：启动即以 check 模式直接挂 hook（最小正确路径，不引入后台进程风险），serve 稳定后切 hook 模式。
- 三模式共用同一管线与裁决逻辑，可并启对照（`--benchmark` 同输入双跑，diff 即协议/管线 bug）。

#### 生命周期：使用驱动（非 agent 进程耦合）【当前】

> 「随 agent 启动/关闭」精确耦合不可行：Crush 无会话事件挂钩（仅 PreToolUse）；ClaudeCode SessionEnd 在 crash/kill 时不触发（孤儿进程），SessionStart 同步 spawn 拖慢会话启动；父子进程信号（PDEATHSIG/pidfd/Job Object）需按平台 API 探测 agent pid（OpenProcess + 启动时间校验防 PID 复用），复杂度远超收益。且「随某一 agent 关闭而关闭」在多会话共用 serve 时是错误语义。故生命周期绑定**使用**而非进程（sccache 模式）：

- **connect-or-spawn**：hook 进程每次先连本机命名端点（µs 级）；连不上 → detached spawn serve + 有界等待（~200ms 预算）就绪重试；仍失败 → 本进程降级 check，绝不无裁决放行。首条命令即「随 agent 启动」。
- **退出**：serve 的在途请求归零且空闲超 grace（`--idle-exit`，默认 30s）自动退出 ≈「随 agent 关闭」（延迟 ≤ grace）；hook 进程崩溃 = 内核关闭其全部句柄，serve 读循环即刻感知 EOF，无 pidfile、无陈旧状态清理逻辑。
- **【备选】ClaudeCode SessionEnd 主动回收**：加速回收，但仅覆盖 ClaudeCode 且不覆盖 crash；serve 稳定后按需加，不做正确性依赖。
- **【已否决】客户端壳 + bash 进程替换持 fd（初稿方案）**：Crush（Go 实现）子进程仅传 std 三件套（fd 全 CLOEXEC），shell 持有的 fd 传不进 hook；且每个 PreToolUse hook 都是全新 bash，跨调用共享 fd 前提不成立。由命名端点方案替换。

#### serve 模式协议（命名端点，一项目一实例）

- **传输**：**本机命名端点**（Windows named pipe / Unix domain socket），协议不绑定实现语言；不走 localhost TCP（连接膨胀、端口管理、安全面大）。非 Windows 平台优先 abstract namespace socket（进程死即消失，无残留文件），退选文件系统 socket（需处理崩溃残留 unlink + rebind 有界重试）。
- **端点名**：`crush-tether-<hash(canonical(project_dir), engine标签)>`（engine 取自 CLI 参数）。**一项目一 serve**：配置/热重载/裁决域天然按项目隔离，进程内无需多项目缓存与逐出；同项目**所有 agent/会话**共用同一 serve（裁决与 agent 无关，Channel 适配留在一次性 hook 进程）。
- **单实例**：serve 启动第一动作 = **独占创建端点**（bind / 第一管道实例创建），同一 syscall 同步裁定唯一性与角色：成功 = 本项目唯一服务；失败 = 已存在 → 本进程静默退出（输者转 connect 重试，非报错退出）。同项目多会话并发冷启动的惊群由此消解，无锁无 pidfile。崩溃残留：Windows 管道与 abstract socket 活在内核命名空间，进程死即消失，天然免疫；文件系统 socket 需「bind 失败但 connect ECONNREFUSED → unlink + rebind」有界重试。
- **协议**：复用 hook 的 JSON envelope 作行单元：请求 `{id, op:"check", command}` / `{id, op:"ping"}`；响应 `{id, verdict:{decision, reason}}`。`id` 客户端生成单调递增，严格逐请求应答，无乱序。连接生命周期 = 一次请求（短命 hook 进程），无长连接池、无会话态。
- 依赖钉版：tree-sitter 0.25 / tree-sitter-bash 0.25 / serde 1 / serde_json 1 / toml 0.9；`rhai`/`mlua`/`notify` 待 P3/P4 引入，避免未用依赖拖累编译。
- **连接感知**：全靠内核事件，建立 = `accept()` 返回 / `ConnectNamedPipe` 完成，断开 = read 得 EOF（`0`）/ `ERROR_BROKEN_PIPE`；本机端点不存在 TCP 式半开连接（同机进程死 = 内核关 fd = 对端立即 EOF），无需心跳。
- **Windows 忙实例**：第二客户端 `CreateFile` 得 `ERROR_PIPE_BUSY` → `WaitNamedPipe` 重试后重连（客户端标准模式）。
- **安全**：端点 ACL 限当前用户（Windows 管道默认 DACL / unix socket 0600）。同用户其他进程可伪造请求，但裁决只输出 allow/confirm/deny 且 deny/confirm 均为安全侧，伪造最多把危险命令转人工确认，无可放大面。
- **v1 串行 accept**：`accept → 读 → 判 → 写` 单循环，「连接计数」退化为 `last_activity` 时间戳（poll timeout = 距退出 deadline 的剩余时间，到点醒一次退出，其余零唤醒）；并发 hook 请求排队，每请求 <1ms 可忽略；慢请求 per-request deadline（~5s）兜底。【备选】epoll/IOCP + atomic 计数的并发版，升级只换连接处理、协议不变（开闭原则落点）。

#### 软件与项目内脚本分工（定稿）

> 项目内**不存在任何可执行脚本**：agent 配置写的命令就是二进制本体；connect / 独占 bind / 沙箱执行全部在二进制内；`rules.rhai|lua` 是**数据文件**，在二进制内的 DSL 沙箱执行，永不以 OS 进程形态存在。跨仓库分发 = 分发一个 `.exe` + 可选规则数据。

| 资产 | 形态 | 所在位置 | 职责 | 不做什么 |
|---|---|---|---|---|
| `crush-tether`（二进制本体） | Rust 静态单 `.exe` | cargo 安装路径（`~/.cargo/bin/` 等） | 三角色运行时：hook client（channel 适配 + connect-or-spawn + 降级）/ serve（独占 bind + 端点监听 + 热重载 + idle 退出）/ check（单发全量管线）；独占 bind 的 syscall 在此 | 不存项目状态；不感知 agent 契约以外的环境 |
| agent 配置条目 | 配置文本 | `.crushrc` / ClaudeCode settings | 一行命令直指二进制（`crush-tether hook --agent crush`） | 无包装脚本、无内联 shell 逻辑 |
| `.crush-tether/rules.toml` | 声明层数据 | 项目内 | 数据/默认值/命令集合/简单 when→decision 规则 | 不是代码，不被执行 |
| `.crush-tether/rules.rhai|.lua` | 脚本层数据 | 项目内 | 跨命令逻辑/自定义谓词/fn 规则 | 不独立执行，仅在二进制内沙箱运行（max_operations 限流） |
| 用户/全局配置 | 声明+脚本数据 | `~/.config/crush-tether/` 等系统路径 | 与项目同构的三层配置 | 同上 |

#### 配置加载与热重载

- **冷启动全量、热更新整段重编译**：启动读 全局 → 用户 → 项目 三层（三层皆无有效配置则先执行项目侧默认生成，见 [零内置策略与默认配置生成（定稿）](#零内置策略与默认配置生成定稿)），按上文优先级 merge 后编译成不可变快照 `Arc<RuleSet>`；任一文件变化则**整段重建**再原子换指针（O(1)），在途请求继续用旧快照，新请求用新快照，无锁争用。
- **声明层**（`rules.toml`）：`serde` 反序列化 → 规则序表（μs 级，可随时整表重建）。
- **脚本层**（`rules.rhai|lua`）：`Engine` 全局只建一次（编译 AST 缓存）；文件变化才重新编译；编译失败**保留旧快照** + stderr 告警，绝不半更新。
- **监听**：`notify`（inotify / ReadDirectoryChangesW）+ **600ms debounce**（编辑器写临时文件再 rename 会连发事件）；规则文件 KB 级，整文件重读远比增量 patch 简单可靠。
- **容错**：监听失败/事件丢失降级为**每请求 stat mtime**（一次 syscall 级开销），正确性不受影响；mtime + size + hash 三重校验防误判。

#### 资源与低配友好性

- **内存**：Rhai `Engine` ~1-2MB、脚本 AST 几十 KB、规则表 + `Cmd` 缓冲池化复用，整体常驻 <5MB。
- **零 busy-loop**：文件监听由 OS 事件驱动，空闲零 CPU。
- **每次命令成本**：常驻 = 一次端点收发 + 已编译规则匹配（tree-sitter 解析 + 判定 <1ms）；兜底 = 冷启动 ~2ms（Rust 静态二进制）+ 全量加载。
- **budget**：P95 < 5ms（serve）/ < 10ms（check），内存 < 10MB；CI 加 `--benchmark` 门槛防退化。

#### 扩展点（开闭原则落点）

- 新 agent = 新 `Channel` 实现（不动 core）；新分类规则 = 新 `Rule` 或一条 `[[rules]]`（不动管线）；新 DSL 引擎 = 新 `RuleEngine` 实现（不动调用方）。
- **依赖方向**：`model ← cmd_parse/engine/config ← channel/service`，`model`/`engine` 不反向依赖 channel/service（编译期由模块可见性保证）。
- **服务化不侵入核心**：`engine`/`config` 不感知「谁在调用、调用几次」，热重载只是换 `Arc<RuleSet>` 指针。

### DSL 引擎（定稿）

| 引擎 | 用途 | 说明 |
|---|---|---|
| **Rhai**（默认） | 新语法 | `Engine::new().eval()` 一行嵌入；动态类型、专为配置/规则脚本 |
| **Lua（mlua）** | 兼容旧习惯 | 经典语法，`--engine lua` 切换 |

- 默认 `--engine rhai`；`--engine lua` 可选。两者实现同一 `RuleEngine` trait，由 Rust 提供**不可绕过的安全原语**（`writes_file`/`path_escapes`/`deny`），DSL 只能组合判定、不能绕过。
- 安全防护：脚本须设 `max_operations`/`max_call_levels`/`max_expr_depth` 限流，防死循环/OOM。

### 命令建模与规则（定稿）

每个 AST 拉平后的简单命令抽为特征对象 `Cmd`：

```rust
struct Cmd {
    bin: String, subcommand: Option<String>, args: Vec<String>,
    flags: Vec<String>, positionals: Vec<String>,
    // 语义特征（由 Rust 安全原语提供）：
    write_flag: bool, writes_file: bool, path_escapes: bool,
    pipe_to_shell: bool, mutates: bool, in_repo: bool,
    positional_count: usize,
}
```

规则 = 匹配器(`when`) → 决策(`then`)，有序链、first-match-wins。同一命令的多态（如 `git config` 读写两态）用多条规则按序评估。复合命令（`echo hi && rm -rf /`）经 flatten 逐节点分类后再组合裁决（任一 deny→deny；全 allow→allow；否则 confirm）。

> **【已替换】（2026-09-05 草案更正）**：`when→then` 规则链与 first-match-wins 已被[配置格式与脚本边界（草案 v1）](#配置格式与脚本边界草案-v1)的三桶查表替换——声明层为纯查表（`[[rules]]`/`DataRule` 不再从配置实例化，改为查表结构），条件判断（两态子命令、参数检查）改由脚本层承载；组合裁决语义（任一 deny→deny 等）不变。`Cmd` 特征对象与 `Rule` trait 的管线位置不变，具体形态随 P2/P3 实现修订。

### 筛查管线与编译期组装（定稿）

```
Parse → Flatten → Extract(每节点造 Cmd/Features) → Match(逐规则) → Verdict(组合节点裁决)
```

```rust
pub trait Rule {
    fn extract(&self, node: &Cmd, feat: &mut Features) {}
    fn match_(&self, node: &Cmd, feat: &Features) -> Option<Decision> { None }
    fn finalize(&self, node: &Cmd, v: &mut Verdict) {}
}
```

规则链完全由配置实例化（`[[rules]]` → `DataRule`，脚本层 → DSL 规则），**无编译期内置策略**；二进制编译期组装的只有管线与安全原语，无反射/无运行时注入。

### Agent 适配层（定稿）

角色拆两层（**crate 内逻辑模块**，非物理 crate）：**core 模块** 负责配置 + 规则管线 + 分类，与 agent 无关；**channel 模块** 只做「拿命令 / 输出裁决」，每种 agent 一个 adapter。

```rust
pub trait Channel {
    fn read_command(&self) -> Option<String>;
    fn project_dir(&self) -> PathBuf;
    fn emit(&self, v: Verdict) -> ExitCode;
}
```

**首发 adapter：Crush（一）→ ClaudeCode（二）**。其余 agent 留空壳，不首发。
- **OpenCode**：延后至版本稳定（存在 V1 `permission`/`bash` 与 V2 `permissions`/`shell` 分叉、插件 in-process、命令改写 bug），**不先适配**。
- 其他候选（Cursor / Continue.dev / Gemini CLI / Cline / Roo）仅列空壳，后续按需扩展。

#### Crush 契约（核实）

- 输入：stdin JSON `{event, session_id, cwd, tool_name, tool_input:{command}}`；env `CRUSH_PROJECT_DIR`/`CRUSH_TOOL_INPUT_COMMAND`/`CRUSH_CWD`/`CRUSH_EVENT`/`CRUSH_TOOL_NAME`/`CRUSH_SESSION_ID`。
- 输出（exit 0 + JSON envelope）：`{version:1, decision:"allow"|"deny", halt, reason, context, updated_input}`。
  - allow → `{"decision":"allow"}` exit 0
  - confirm → 不输出 JSON、exit 0（走正常权限流程）
  - deny → exit 2（stderr 作 reason）**或** JSON `{"decision":"deny"}` exit 0
- 聚合：`deny > allow > 无意见`；`decision:"allow"` 需 exit 0。

#### ClaudeCode 契约（核实）

- 输入：stdin JSON `{tool_name:"Bash", tool_input:{command,description,timeout,run_in_background}, cwd, session_id, prompt_id, permission_mode, transcript_path, hook_event_name, tool_use_id}`；env `CLAUDE_PROJECT_DIR`（session 起点绝对根；**无 `CLAUDE_MODEL`**）。
- 权限来源基准：`cwd`（stdin）优先，回退 `CLAUDE_PROJECT_DIR`。
- 输出（**`decision`/`reason` 已废弃**，走 `hookSpecificOutput`）：
  - allow → `{"hookSpecificOutput":{"hookEventName":"PreToolUse","permissionDecision":"allow"}}` exit 0
  - confirm → 「吐出该 JSON 且 `permissionDecision:"ask"`」 exit 0
  - deny → exit 2 + stderr，**或** JSON `permissionDecision:"deny"` exit 0
- 规则：`deny > defer > ask > allow`；exit 2 会覆盖 JSON。
- **Crush 兼容**：Crush 接受 Claude 的 `hookSpecificOutput` 信封，仅 `updated_input` 语义不同（Crush 浅合并 vs Claude 全替换）。ClaudeCode adapter 可复用 Crush 大部分输出逻辑，仅改 env/输入键名与 `updated_input` 语义。

## 配置格式与脚本边界（草案 v1）

> [!IMPORTANT]
> **状态：草案，非定稿（2026-09-05）**。本节在 P2 实现前先纸面钉一版格式，作为 P2/P3 的实现基准；实现期发现表达力不足时就地修订本节并留更正记录，89 条回归用例以「引擎 + 默认配置 fixture」全绿后升格定稿。与既有定稿的冲突见本节末尾[更正登记](#更正登记对既有定稿)。

### 设计原则（草案定调）

- **声明层零条件判断**：`rules.toml` 只做「命令归属查表」（无一处 if/参数检查）；一切带条件判断的分类（两态子命令、参数内容检查、跨命令检查）全部下沉脚本层。这是对定稿「能声明表达的进 TOML」的收窄，见[更正登记](#更正登记对既有定稿)。
- **命令中心**：配置按「命令 → 子命令/flag 分桶」组织，不设命名命令集合与规则链（替代定稿的 `[[rules]]` + first-match-wins）。
- **作用域即结构**：文件顶层分 `[local]` / `[global]` 两表——`[local]` = 效果不出项目（其内 allow 一律带路径逃逸检查）；`[global]` = 允许影响项目外（其内 allow 豁免逃逸检查，如团队统一放行的 docker）。

### `rules.toml` 结构（草案）

```toml
# 裸键区（所有表头之前）
default    = "confirm"                      # 未命中任何配置的命令
precedence = ["deny", "confirm", "allow"]   # 桶间优先级,可调;跨层 merge 时整表覆盖

# ═══ [local]:效果不出项目 —— 此表内所有 allow 默认带路径逃逸检查 ═══
[local]
allow   = ["ls", "cat", "grep", "rg", "find", "head", "tail", "wc", "pwd", "echo",
  "printf", "which", "file", "stat", "sort", "uniq", "comm", "diff", "md5sum",
  "sha1sum", "sha256sum", "sha512sum", "date", "env", "du", "nl", "less", "more",
  "tree", "ls-files", "rev-parse",
  "cargo", "go", "gofmt", "black", "ruff", "dprint", "make", "just", "pytest",
  "touch", "mkdir"]
confirm = ["rm", "pip", "pip3", "npx", "curl", "wget"]

[local.git]
allow.sub   = ["status", "log", "diff", "show", "branch", "--version", "remote",
  "ls-files", "config", "rev-parse", "blame", "shortlog", "tag", "help", "describe",
  "check-ignore", "show-ref", "for-each-ref", "cat-file", "ls-tree", "diff-tree",
  "name-rev", "merge-base", "rev-list", "diff-index", "diff-files", "check-ref-format",
  "add", "commit", "checkout", "switch", "mv"]
confirm.sub = ["rm", "restore", "reset"]
deny.sub    = ["push", "pull", "reset", "clean", "rebase", "revert", "cherry-pick",
  "fetch", "gc", "prune", "filter-branch", "reflog"]
deny.flag   = ["--output", "-o", "--pretty", "--format", "--config", "-c",
  "--force", "-f", "--hard", "--in-place", "-w", "-h"]

[local.npm]
confirm.sub = ["install", "i", "ci", "add", "uninstall", "remove", "update", "publish"]
default     = "allow"     # 节内 default 覆盖顶层 default(npm run 等其余子命令放行)

[local.pnpm]
confirm.sub = ["install", "add", "remove", "upgrade", "update", "publish"]
default     = "allow"

# ═══ [global]:允许影响项目外 —— 此表内 allow 豁免逃逸检查 ═══
[global]
allow = []    # 默认配置为空;项目特例/团队统一放行(如 docker)写这里,随项目提交
```

- **三桶**：每命令一节（或头部裸列表），`allow` / `confirm` / `deny` 三桶 + `sub`（子命令）/ `flag`（flag）两个子键；flag 长写与简写**并排显式枚举**，不做前缀推断。
- **deny/confirm 单份**：deny/confirm 是安全侧，与作用域无关（`git push` 在项目内同样该拒），全部写在 `[local]` 下；`[global]` 表实际只承载 allow。
- **查表语义**：命令先定位表（命中 `[global].allow` 的命令整命令豁免，同命令两表皆现时 `[global]` 优先——全局放行是更强的承诺）；同表内按 `precedence` 顺序查桶，先命中先裁决。
- **兜底**：未命中任何配置 → 顶层 `default`；各节可用节内 `default` 覆盖。

### 脚本层职责边界（草案）

一切条件判断进脚本（`rules.rhai`），分四类：

1. **两态子命令**：`git branch/remote/tag/config` 的 action-token 命中 → confirm（纯读保持 TOML allow）；`git config` ≥2 位置参数 = 写 → confirm。
2. **参数内容检查**：`find` 带 `-delete/-exec/-execdir/-ok/-okdir` → confirm；`curl/wget` 参数含 `|` → deny（纯拉取维持 confirm）。
3. **跨命令检查**：管道 sink（管道下一段为 bash/sh/zsh/python/python3/perl/php/ruby）→ 整体 deny。
4. **原语组合升级**：readonly/allow 命中但带写 flag 或写重定向 → confirm；`[local]` allow 命中但路径逃逸 → confirm。

- **引擎保留原语**（脚本可组合、不可绕过）：写重定向检测（丢弃式 `/dev/null` 与 fd dup/close 豁免）、写 flag 检测（读 TOML flag 桶）、路径逃逸检查（`[local]`/`[global]` 表的豁免语义在引擎实现）、unparseable → confirm 兜底、复合命令组合裁决（任一 deny→deny / 全 allow→allow / 否则 confirm）。
- **脚本 allow 契约（新增待定稿）**：允许脚本对**显式枚举的命令**返回 allow（支撑项目侧 global 放行特例）；**禁止无条件 allow 兜底**（防一行脚本架空门禁）。

### 日志（格式先行，开关位置 P4 定）

JSONL 一行一条裁决，字段覆盖：命令原文、结果、触发层级、触发条件：

```json
{"ts":"2026-09-05T14:03:22+08:00","mode":"serve","agent":"crush",
 "command":"git push --force origin main",
 "decision":"deny","reason":"git.deny.sub: push",
 "source":{"layer":"project","file":".crush-tether/rules.toml",
           "entry":"git.deny.sub","match":"push"},
 "script":{"file":null,"rule":null}}
```

- `source.layer` ∈ global/user/project/explicit/script/default；脚本裁决填 `script.file`/`script.rule`。
- serve 模式由 serve 单点写（复用行协议已有字段），hook 降级路径自写一行；人读视图由后续 `crush-tether log` 子命令渲染 JSONL（人看视图、程序读原文）。默认开/关在 P4 落地 serve 时定。

### merge 语义（新定义，token 级）

- 优先级不变：项目 > 用户 > 全局（见 [配置分层与优先级（定稿）](#配置分层与优先级定稿)）。
- 同 `bin` 下**逐 token 合并**：每个 sub/flag token 以**最高层所在桶为准**，高层未提及的 token 由低层补齐；`default`/`precedence` 与头部裸列表为标量，整表高层覆盖。例：用户层 allow `git log`、项目层 deny `git log --output` 可同时成立。

## 更正登记（对既有定稿）

> 以下为对本文档已定稿措辞的更正（草案阶段调整，非推翻方向），原定稿表述处已加更正指针，不静默覆盖：

1. 「能声明表达的进 `rules.toml`」→ 收窄为「**无条件的纯查表**进 `rules.toml`，一切条件判断进脚本」。
2. 「`[[rules]]` 规则链 + first-match-wins」→ **删除**，替换为「`[local]`/`[global]` 双表 + 每命令三桶查表 + 可调桶间优先级 `precedence`」。
3. 「命令集合并集（只增不减，`exclude` 表剔除）」→ 替换为 **token 级高层胜出、低层补缺**；无需 `exclude` 表（高层把 token 挪桶即等效剔除）。
4. guard.py `WRITE_FLAG_PREFIXES` 前缀匹配（`--output=` 等）→ 放弃，flag 长写/简写显式并排枚举。
5. guard.py `WRITE_FLAGS` 中的 `-h` 疑似笔误（`git -h` 可改写行为的判定依据待查），草案 1:1 保留，实现期确认后剔除并在此登记。
6. 作用域由「每命令 scope 属性」方案（曾讨论）收敛为 `[local]`/`[global]` 顶层双表。

## 待定决策（见 cairn/ROADMAP.md）

> 原有三项（抽取方式 / 是否重写 / fail-safe 时机）已于 2026-09-04 定稿，见 [CAIRN-MOVED]；当前无未定项。P2+ 实现里程碑见 `cairn/ROADMAP.md` 推进计划。

- **抽取方式**：【当前】纯全局工具（本仓库为唯一实现，mdor 不再保留 crush-guard；Python 版随回归用例平移后退役）。
- **是否重写**：【当前】Rust 重写已落地（P0+P1 完成，`src/{model,cmd_parse,engine,channel}` + check 模式 + 回归测试 9/9 绿）。
- **fail-safe**：guard 任何环节崩了都保守降级为确认（输出 none）而非放行；解析失败（含 heredoc 无终止符）走 `unparseable` → confirm。
- **规则来源**：【当前】零内置策略（2026-09-04 定稿）——二进制为纯引擎，默认策略由项目侧生成的外部 `rules.toml` + `rules.rhai` 提供；三层皆缺才生成，损坏留档（`.bak-<时间戳>`）后重新生成；全局/用户层生成由命令提供（后期设计）。见 [零内置策略与默认配置生成（定稿）](#零内置策略与默认配置生成定稿)。

## 实现时的安全目标（fail-safe）

- guard 任何环节崩了都该**保守降级为确认**（输出 none），而不是放行。
- hook 失败 = fail-open 是安全上的反模式，必须用「输出 none」而非「依赖失败」来兜底。
