# design.md — crush_tether 设计文档

`crush_tether` 是 Crush 的**命令级 bash 权限门**：通过 PreToolUse hook 对每条 bash 命令做三档分类（`allow` / `confirm` / `deny`），返回给 Crush 决定是否放行。本文件是 crush-guard 抽取/重写的设计单一事实源；crush-guard 原型的历史结论与教训沉淀于 mdor 仓库 `cairn/`（crush-guard-bash-gate 等），本项目侧可复用结论已并入本文件与 `cairn/` 主题笔记。

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
>
> 定位澄清（2026-09-06）：guard.py 及其 89 条回归用例是**语义参考、灵感来源与待替换对象，不是本项目的验收标准**——默认包与回归用例断言冲突时，以定稿草案为准更新用例并留变更记录（论证见 [D-05](decisions.md#d-05-guardpy-定位重置参考对象而非验收标准)）。

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
│   ├── lib.rs            # 库入口：核心逻辑装配，可被复用/单测
│   ├── main.rs           # bin：装配壳（check/hook/serve/benchmark 四模式参数分发）
│   ├── model.rs          # Decision / Verdict（combine 组合语义）/ unparseable 兜底
│   ├── engine.rs         # 管线原语：管道 sink 拓扑 + 组合裁决的注入式顶层入口（零内置策略）
│   ├── cmd_parse.rs      # tree-sitter-bash 解析 + flatten + 写重定向/路径逃逸检测
│   ├── channel.rs        # agent 适配层（Crush / ClaudeCode / zcode 契约；stdin JSON/env → 裁决输出）
│   ├── config/           # 发现（含项目根解析单一实现）/ schema / 字段级继承合并 / 归一 / seed 模板
│   ├── lookup.rs         # rules.toml 查表（多命中合成 + 溯源）
│   ├── script/           # 脚本层沙箱（RuleEngine trait + ScriptChain 层链 + 定稿点；mod.rs=rhai、lua.rs=lua 引擎）
│   ├── knowledge.rs      # 命令知识库（bucket 框架；alias_of/same_flag 归一数据源）
│   ├── lint.rs           # 双层 lint（结构类 + 语义类；只告警不拒绝）
│   └── service.rs        # RuleSet 装配 + serve（端点/热重载/idle 退出）+ hook client + 裁决日志
├── tests/                # 集成测试（guard_regression 89 用例平移 + service/script/config 验收）
├── script/               # 本地工程脚本（check-links 等，uv 管理）
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
  │ 命名端点 crush-tether-<hash(项目根, engine, --config)>（每项目一实例 · ACL 限当前用户）
  ▼
crush-tether serve（二进制本体 · detached 常驻 · server 角色）
  · 启动第一动作 = 独占创建端点：成功 = 唯一服务；失败 = 已存在 → 静默退出
  · 串行 accept：请求 → 匹配规则快照 → 应答
  · last_activity 归零 + 空闲超 grace → exit（崩溃自愈：下一条命令由 hook 重拉）
  · 三层配置 → merge → 编译不可变快照；notify + debounce → 整段重编译后整体替换
  · DSL（Rhai/Lua）沙箱在本进程内执行，永不以 OS 进程形态存在

check（兜底/冒烟）：同一二进制单发，不碰端点，本进程全量 Parse → … → Verdict
```

模块分层（依赖方向单向，编译期可见性强制）:

```text
装配层  main.rs               子命令分发（hook / serve / check / benchmark），仅此层知道运行角色
适配层  channel.rs  service.rs  agent 契约适配；RuleSet 装配、端点监听与 connect-or-spawn 客户端、热重载、idle 退出、裁决日志
核心层  engine.rs  cmd_parse.rs  管线原语与裁决组合；tree-sitter-bash 解析与特征提取
        config/  lookup.rs  script/  knowledge.rs  lint.rs   发现/合并/查表/脚本沙箱/知识库/双层 lint
类型层  model.rs              Decision / Verdict / 组合语义

依赖方向：model ← cmd_parse/engine/config/knowledge ← lookup/script/lint ← channel/service ← main
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
  - **【已替换】（2026-09-05 草案更正；2026-09-06 再修订）**：merge 语义与规则链已被[配置格式与脚本边界（v1 定稿）](#配置格式与脚本边界v1-定稿)替换——命令集合并集 → 字段级继承合并（数组覆盖 / inline table 增删，见 [D-02](decisions.md#d-02-字段级继承合并模型)）；`[[rules]]` 规则链 → `[local]`/`[global]` 双表三桶查表。已于 2026-09-06 随本节升格为定稿。

### 配置拆分（定稿）

```text
.crush-tether/                 # 项目级配置目录
├── rules.toml                 # 声明层：数据/默认值/命令集合/简单 when→decision 规则/security
└── rules.rhai 或 rules.lua    # 脚本层：跨命令逻辑/自定义谓词/fn 规则（按 --engine 选后缀）
```

用户级 `~/.config/crush-tether/`、全局（系统路径；其默认文件由命令生成，后期设计）与项目同构。脚本层**同文件按优先级**：项目脚本最后执行，可作最终裁决。

### 零内置策略与默认配置生成（定稿）

> 定稿（2026-09-04）：**软件本身不提供任何规则**——二进制只实现引擎能力（解析/flatten/特征提取/安全原语/管线/组合裁决），不含一行策略数据。默认策略也由**外部配置文件 + 脚本**提供，以项目侧生成的形态落地。

- **默认策略 = 外部数据**：默认 `rules.toml`（能声明表达的：命令集合、flag 前缀、位置参数数、特征布尔等）+ 默认 `rules.rhai`（或 `--engine lua` 时的 `rules.lua`，M6.1；声明层表达不了的跨参数逻辑：`find` 突变检测、`git config` ≥2 位置参数写判定等）以**模板内嵌于二进制**——模板只是生成源数据，不参与判定，不构成内置策略。
  - **【已替换】（2026-09-05 草案更正）**：默认 `rules.toml` 的内容界定已由[配置格式与脚本边界（v1 定稿）](#配置格式与脚本边界v1-定稿)收窄——无条件的纯查表进 TOML（不再有「flag 前缀」「位置参数数」这类声明层字段），一切条件判断下沉脚本层；默认包的具体结构以本节（v1 定稿）为准。
- **生成触发**：按层寻找配置（全局 → 用户 → 项目，含 `--config`/`CRUSH_TETHER_CONFIG` 显式指定）后**三层合并仍得不到任何有效配置**时，才在项目 `.crush-tether/` 写出默认 `rules.toml` + 脚本模板（`rules.rhai`；`--engine lua` 时为 `rules.lua`）；**任一层存在有效配置即尊重现状，不生成**（避免将来全局/用户自定义被项目层默认值遮蔽）。
- **损坏 ≠ 缺失（2026-09-06 收窄）**：文件存在但解析失败 → stderr 告警 + 按 fail-safe confirm 兜底，**原文件不动、不留档不生成**；仅文件不存在（三层皆缺）才触发生成。原「留档 `.bak-<时间戳>` 后重生成默认」方案已收窄——留档接管会使 serve 冷启动遇用户手改中间态时裁决静默漂移且 `.bak` 堆积（论证见 [D-03](decisions.md#d-03-损坏重生成收窄)）。
- **全局/用户层生成延后**：v1 只做项目层生成；全局/用户层默认文件由命令提供（如 `crush-tether init --global`，后期设计）。
- **引导豁免**：生成动作本身是管线引导步骤，不经规则链判定；只写 `.crush-tether/` 下固定文件名，不触碰其他路径。
- **幂等与原子**：模板内容恒定 → 重复生成结果一致（幂等）；落盘 temp + rename 原子替换，多 hook 并发发现缺失时天然收敛到同一结果。
- **fail-safe 衔接**：生成完成前 / 生成失败时按既有 fail-safe 处理（unparseable → confirm），绝不放行。
- **测试落点**：89 条回归用例改为「引擎 + 默认规则 fixture」驱动——默认配置文件本身成为验收对象；默认包与用例断言冲突时以定稿草案为准更新用例并留变更记录（guard.py 为语义参考而非验收标准，见[判定表节定位澄清](#判定表纯语义可-11-平移)）。

### 运行模式与配置热重载（定稿）

#### 运行模式（hook / serve / check / benchmark）

二进制本体承担四个子命令角色，agent 配置只写 `hook`；**无子命令参数时默认 `check`**（嵌入/测试便利）：

```text
crush-tether hook [--agent crush --engine rhai --config <file>]   # agent 配置入口：connect-or-spawn，失联降级单发
crush-tether serve [--engine rhai --config <file> --project <dir> --idle-exit <secs>]  # 常驻：命名端点监听（hook 自动拉起，也可手动）
crush-tether check [--agent crush --engine rhai --config <file>]  # 单发：stdin JSON → stdout/exit code（兜底 + 冒烟测试）
crush-tether benchmark [--engine rhai --config <file>]            # 双跑对比：in-process vs serve 路径，裁决 diff 为空即 exit 0
```

| 模式 | 触发方 | 进程 | 配置解析次数 | 适用 |
|---|---|---|---|---|
| hook | agent 的 PreToolUse 配置直配二进制 | 每命令一次（短命 client） | serve 已加载则 0 次；降级时全量 | 正常使用 |
| serve（常驻） | hook 进程 connect 失败时自动 detached 拉起（或手动） | 长驻后台 | 文件变化时重载 | 正常使用 |
| check（默认） | 无子命令参数 / 手动 / 测试 | 每命令一次 | 每次全量 | 兜底路径 / 冒烟测试 / 模式验收 |
| benchmark | 手动 | 双跑一次 | 两次全量 | 验收 in-process 与 serve 路径 diff 为空 |

- 测试钩子环境变量：`CRUSH_TETHER_LOG=0|off|false`（关裁决日志，见「日志」节）、`CRUSH_TETHER_IDLE_EXIT=<secs>`（覆盖 spawn 的 serve 空闲退出秒数）、`CRUSH_TETHER_DISABLE_SERVE=1`（hook 跳过 connect-or-spawn，强制降级路径）。benchmark 双跑只对比 decision（不 diff reason——reason 文案允许两路径措辞差异）。

- **check 先行**是 P1 的历史推进策略：agent 入口自 P5 起为 `hook`（serve 路径已验收），check 保留为无子命令时的默认模式（兜底/嵌入/测试）。
- 四模式共用同一管线与裁决逻辑；`benchmark` 双跑 diff 为空是路径等价性的验收门。

#### 生命周期：使用驱动（非 agent 进程耦合）【当前】

> 「随 agent 启动/关闭」精确耦合不可行：Crush 无会话事件挂钩（仅 PreToolUse）；ClaudeCode SessionEnd 在 crash/kill 时不触发（孤儿进程），SessionStart 同步 spawn 拖慢会话启动；父子进程信号（PDEATHSIG/pidfd/Job Object）需按平台 API 探测 agent pid（OpenProcess + 启动时间校验防 PID 复用），复杂度远超收益。且「随某一 agent 关闭而关闭」在多会话共用 serve 时是错误语义。故生命周期绑定**使用**而非进程（sccache 模式）：

- **connect-or-spawn**：hook 进程每次先连本机命名端点（µs 级）；连不上 → detached spawn serve + 有界等待（~200ms 预算）就绪重试；仍失败 → 本进程降级 check，绝不无裁决放行。首条命令即「随 agent 启动」。
- **退出**：serve 的在途请求归零且空闲超 grace（`--idle-exit`，默认 30s）自动退出 ≈「随 agent 关闭」（延迟 ≤ grace）；hook 进程崩溃 = 内核关闭其全部句柄，serve 读循环即刻感知 EOF，无 pidfile、无陈旧状态清理逻辑。
- **【备选】ClaudeCode SessionEnd 主动回收**：加速回收，但仅覆盖 ClaudeCode 且不覆盖 crash；serve 稳定后按需加，不做正确性依赖。
- **【已否决】客户端壳 + bash 进程替换持 fd（初稿方案）**：Crush（Go 实现）子进程仅传 std 三件套（fd 全 CLOEXEC），shell 持有的 fd 传不进 hook；且每个 PreToolUse hook 都是全新 bash，跨调用共享 fd 前提不成立。由命名端点方案替换。

#### serve 模式协议（命名端点，一项目一实例）

- **传输**：**本机命名端点**（Windows named pipe / Unix domain socket），协议不绑定实现语言；不走 localhost TCP（连接膨胀、端口管理、安全面大）。非 Windows 平台优先 abstract namespace socket（进程死即消失，无残留文件），退选文件系统 socket（需处理崩溃残留 unlink + rebind 有界重试）。
- **端点名**：`crush-tether-<hash(canonical(project_dir), engine标签, --config 覆盖路径)>`（engine/`--config` 取自 CLI 参数；`--config` 缺省不进 hash）。**一项目一 serve**：配置/热重载/裁决域天然按项目隔离（显式 `--config` 覆盖视作独立裁决域），进程内无需多项目缓存与逐出；同项目**所有 agent/会话**共用同一 serve（裁决与 agent 无关，Channel 适配留在一次性 hook 进程）。
- **单实例**：serve 启动第一动作 = **独占创建端点**（bind / 第一管道实例创建），同一 syscall 同步裁定唯一性与角色：成功 = 本项目唯一服务；失败 = 已存在 → 本进程静默退出（输者转 connect 重试，非报错退出）。同项目多会话并发冷启动的惊群由此消解，无锁无 pidfile。崩溃残留：Windows 管道与 abstract socket 活在内核命名空间，进程死即消失，天然免疫；文件系统 socket 需「bind 失败但 connect ECONNREFUSED → unlink + rebind」有界重试。
- **协议**：复用 hook 的 JSON envelope 作行单元：请求 `{id, op:"check", command, agent}` / `{id, op:"ping"}`；响应 `{id, verdict:{decision, reason}, error}`（`error` = 畸形请求/未知 op 的带内报错；`agent` 供日志溯源）。`id` 客户端生成单调递增，严格逐请求应答，无乱序（v1 一连接一请求下恒为 1，字段为将来复用连接保留）。连接生命周期 = 一次请求（短命 hook 进程），无长连接池、无会话态。
- 依赖钉版（全景清单；版本约束的唯一事实源 = 根 `Cargo.toml`）：tree-sitter 0.25 / tree-sitter-bash 0.25 / serde 1 / serde_json 1 / toml 0.9 / rhai 1（P3 引入，`internals` feature——AST 字面量提取；传递依赖 `smartstring` unmaintained 见 [D-08](decisions.md#d-08-audit-警告口径接受-smartstring-unmaintained-并名册化)，接受并名册化）/ interprocess 2.4（P4 引入，命名端点）/ notify 8（P4 引入，热重载监听）/ mlua 0.12（M6.1 引入，`lua54` + `vendored` feature——Lua 引擎）。
- **连接感知**：全靠内核事件，建立 = `accept()` 返回 / `ConnectNamedPipe` 完成，断开 = read 得 EOF（`0`）/ `ERROR_BROKEN_PIPE`；本机端点不存在 TCP 式半开连接（同机进程死 = 内核关 fd = 对端立即 EOF），无需心跳。
- **Windows 忙实例**：第二客户端 `CreateFile` 得 `ERROR_PIPE_BUSY` → `WaitNamedPipe` 重试后重连（客户端标准模式）。
- **安全**：端点 ACL 限当前用户（Windows 管道默认 DACL / unix socket 0600）。同用户其他进程可伪造请求，但裁决只输出 allow/confirm/deny 且 deny/confirm 均为安全侧，伪造最多把危险命令转人工确认，无可放大面。
- **v1 串行 accept**：`accept → 读 → 判 → 写` 单循环，「连接计数」退化为 `last_activity` 时间戳；并发 hook 请求排队，每请求 <1ms 可忽略。慢请求兜底 = hook 客户端响应读 **5s deadline**（超时走本进程降级 check）+ serve watchdog 空闲 grace（接受但静默的连接被 grace 回收）。【备选】epoll/IOCP + atomic 计数的并发版，升级只换连接处理、协议不变（开闭原则落点）。

#### 软件与项目内脚本分工（定稿）

> 项目内**不存在任何可执行脚本**：agent 配置写的命令就是二进制本体；connect / 独占 bind / 沙箱执行全部在二进制内；`rules.rhai|lua` 是**数据文件**，在二进制内的 DSL 沙箱执行，永不以 OS 进程形态存在。跨仓库分发 = 分发一个 `.exe` + 可选规则数据。

| 资产 | 形态 | 所在位置 | 职责 | 不做什么 |
|---|---|---|---|---|
| `crush-tether`（二进制本体） | Rust 静态单 `.exe` | cargo 安装路径（`~/.cargo/bin/` 等） | 四模式运行时：hook client（channel 适配 + connect-or-spawn + 降级）/ serve（独占 bind + 端点监听 + 热重载 + idle 退出）/ check（单发全量管线）/ benchmark（双跑对比验收）；独占 bind 的 syscall 在此 | 不存项目状态；不感知 agent 契约以外的环境 |
| agent 配置条目 | 配置文本 | `.crushrc` / ClaudeCode settings | 一行命令直指二进制（`crush-tether hook --agent crush`） | 无包装脚本、无内联 shell 逻辑 |
| `.crush-tether/rules.toml` | 声明层数据 | 项目内 | 数据/默认值/命令集合/简单 when→decision 规则 | 不是代码，不被执行 |
| `.crush-tether/rules.rhai|.lua` | 脚本层数据 | 项目内 | 跨命令逻辑/自定义谓词/fn 规则 | 不独立执行，仅在二进制内沙箱运行（max_operations 限流） |
| 用户/全局配置 | 声明+脚本数据 | `~/.config/crush-tether/` 等系统路径 | 与项目同构的三层配置 | 同上 |

#### 配置加载与热重载

- **冷启动全量、热更新整段重编译**：启动读 全局 → 用户 → 项目 三层（**v1 全局层无发现路径**，为后期设计留位；三层皆无有效配置则先执行项目侧默认生成，见 [零内置策略与默认配置生成（定稿）](#零内置策略与默认配置生成定稿)），按上文优先级 merge 后编译成不可变快照；任一文件变化则**整段重建**后整体替换——v1 串行 accept 下快照为本地所有权 `RuleSet`（rhai `Engine` 非 Send，`Arc` 化不可行），在途请求继续用旧快照，新请求用新快照，无锁争用；**并发版升级点** = rhai `sync` feature + `Arc<RwLock<Arc<RuleSet>>>` 原子换指针。脚本层链同理随快照整体重建（用户层先、项目层最后，见 [D-02](decisions.md#d-02-字段级继承合并模型) 与「配置拆分」）。
- **stale 首请求边界**：热重载信号在 serve 主线程的**请求间隙**消费——debounce 窗口内到达的首个请求仍用旧快照裁决（最坏滞后一个请求，最终一致；裁决日志可按 load 事件行对齐时间线）。
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

- 新 agent = channel 层新增适配（`Agent` 变体 + 契约分派，不动 core）；新分类规则 = 配置词条或脚本层谓词（不动管线，[更正登记](#更正登记对既有定稿) 2/12 的替代形态）；新 DSL 引擎 = 新 `RuleEngine` 实现（不动调用方）。
- **依赖方向**：`model ← cmd_parse/engine/config ← channel/service`，`model`/`engine` 不反向依赖 channel/service（编译期由模块可见性保证）。
- **服务化不侵入核心**：`engine`/`config` 不感知「谁在调用、调用几次」，热重载只是整体替换规则快照。

### DSL 引擎（定稿）

| 引擎 | 用途 | 说明 |
|---|---|---|
| **Rhai**（默认） | 新语法 | `Engine::new().eval()` 一行嵌入；动态类型、专为配置/规则脚本 |
| **Lua（mlua）** | 兼容旧习惯 | 经典语法，`--engine lua` 切换 |

- 默认 `--engine rhai`；`--engine lua` 可选。两者实现同一 `RuleEngine` trait，由 Rust 提供**不可绕过的安全原语**（`path_escapes`/`inside_repo` + `kb_*` 知识库数据源 + `allow("bin")` 受控激活通道；写特征经 ctx 字段 `writes_redirect`/`pipe_to_shell` 暴露），DSL 只能组合判定、不能绕过。
- 安全防护：脚本须设 `max_operations`/`max_call_levels`/`max_expr_depth` 限流，防死循环/OOM。
- **Lua 引擎定型（2026-09-06 M6.1 落地）**：mlua 0.12（Lua 5.4 vendored）。沙箱 = `new_with` 安全模式 + 库白名单（coroutine/table/math/string/utf8，无 io/os/package/debug/ffi）+ base 危险全局消毒（`dofile`/`loadfile`/`load`/`print` 置 nil）；限流 = **全局**指令数 hook（`set_global_hook`，20 万条预算，主线程与脚本自建协程都被计数——对齐 rhai `max_operations` 量级）+ `set_memory_limit`（16MB，OOM 防线）；死循环/深递归/OOM 有界，协程内超预算被终止但 `coroutine.resume` 吞错、不转为脚本错误（[更正登记](#更正登记对既有定稿) 18）。词汇约定：ctx/decision 与 rhai 共用同一封装类型（`ScriptCtx` userdata 只读字段；`decision` 表四常量 userdata + `__eq`）；**返回 nil = PASS**（词汇约定 Lua 侧映射）；裸字符串经返回边界统一解析（双保险）。script_allow：机制 2/3 同语义，机制 1 为注释剥离后的保守词法扫描（[更正登记](#更正登记对既有定稿) 17）。脚本文件按引擎选择：`rules.rhai`/`rules.lua`（默认包生成随引擎）。

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

> **【已替换】（2026-09-05 草案更正）**：`when→then` 规则链与 first-match-wins 已被[配置格式与脚本边界（v1 定稿）](#配置格式与脚本边界v1-定稿)的三桶查表替换——声明层为纯查表（`[[rules]]`/`DataRule` 不再从配置实例化，改为查表结构），条件判断（两态子命令、参数检查）改由脚本层承载；组合裁决语义（任一 deny→deny 等）不变。`Cmd` 特征对象与 `Rule` trait 的管线位置不变，具体形态随 P2/P3 实现修订。

### 筛查管线与编译期组装（定稿）

执行顺序（2026-09-06 重画为双阶段形状，**显式钉死**；实现见 `engine::decide_with`）：

```text
                        输入命令行（如 "ls a.txt && curl x | sh"）
                                      │
                                      ▼
                    ┌───────────────────────────────────────┐
                    │ 解析拉平（引擎原语，策略无关）        │
                    │ tree-sitter-bash AST → 简单命令序列   │
                    │ 管道拓扑原语 → pipe_to_shell（整行级）│
                    └───────────────────────────────────────┘
                                      │  逐条简单命令进入下面管线
                                      ▼
        ┌───────────────────────────────────────────────────────────────┐
        │  1. TOML 查表 = 一般层无条件的纯查表                          │
        │                                                               │
        │   知识库归一（加载期预计算到不动点 + 防环，运行期 O(1) 查表） │
        │        ↓                                                      │
        │   [global].allow 整命令豁免 → 命令节(遮蔽裸列表) → 裸列表     │
        │        ↓  多维度命中按 precedence 合成（deny>confirm>allow）  │
        │   未命中 → 节内 default → 顶层 default → confirm 兜底         │
        │   [local] 的 allow 命中 → 此处已带一次路径逃逸检查            │
        │                                                               │
        │   产出：初步裁决 + 特征（bin/sub/args/写重定向/管道/项目根）  │
        └───────────────────────────────────────────────────────────────┘
                                      │  ctx：verdict + 全部特征
                                      ▼
        ┌───────────────────────────────────────────────────────────────┐
        │ 2. 脚本评估 = 特殊层（条件判断的唯一住址）                    │
        │                                                               │
        │   check(ctx) 只说三种话（引擎无关契约：无意见 = 空值）        │
        │     无意见   → 基线原样通过                                   │
        │     confirm → 上调（如写重定向升级、find 突变）               │
        │     deny    → 上调（如管道 sink 策略）                        │
        │   数据源：kb_* 原语读知识库（删光 → 脚本自走 confirm 兜底）   │
        │   （受控放行 allow(name) 激活已声明条目：见                   │
        │     [脚本条件放行](#脚本条件放行script_allow定稿)，实现 M4.0）│
        └───────────────────────────────────────────────────────────────┘
                                      │
                                      ▼
        ┌───────────────────────────────────────────────────────────────┐
        │ 3. 定稿 = 全引擎唯一的放行出口（安全性质的物理落点）          │
        │                                                               │
        │   deny 终审：查表落 deny 的命令，脚本任何返回都翻不动         │
        │   allow(name) 激活：按声明作用域元数据决定——                  │
        │     local 声明 → 此处执行路径逃逸检查（逃逸 → confirm）       │
        │     global 声明 → 豁免（两表皆声明时 global 胜，M2.3 同规）   │
        └───────────────────────────────────────────────────────────────┘
                                      │  每条简单命令的最终裁决
                                      ▼
        ┌───────────────────────────────────────────────┐
        │ 4. 组合裁决（引擎原语）：任一 deny → deny；   │ 
        │    全 allow → allow；否则 confirm             │
        └───────────────────────────────────────────────┘
```

三个安全性质随管线形状显式钉死：**定稿点唯一**（放行出口只有一个，单点施加检查）；**逃逸检查挂定稿点**（不在脚本里——脚本不可信，不能依赖作者自觉；不在脚本之前——激活尚未发生；local 语义的激活在 ③ 处强制复查，脚本自查通过也照样再查）；**deny 终审**（不可逆操作不给任何机制留放行通道，与组合裁决「任一 deny → deny」同一原则）。

原 `pub trait Rule`（extract/match_/finalize 规则链接口）已被替代：单命令分类 = 查表（`lookup::RuleLookup`）+ 脚本（`script::RuleEngine`）经规则注入式入口 `engine::decide_with` 组装，见[更正登记](#更正登记对既有定稿) 12。**无编译期内置策略**的性质不变：二进制编译期组装的只有管线与安全原语，策略全部来自外部配置文件。

### Agent 适配层（定稿）

角色拆两层（**crate 内逻辑模块**，非物理 crate）：**core 模块** 负责配置 + 规则管线 + 分类，与 agent 无关；**channel 模块** 只做「拿命令 / 输出裁决」，每种 agent 一个 adapter。

```rust
pub trait Channel {
    fn read_command(&self) -> Option<String>;
    fn project_dir(&self) -> PathBuf;
    fn emit(&self, v: Verdict) -> ExitCode;
}
```

**首发 adapter：Crush（一）→ ClaudeCode（二）→ zcode（三，2026-09-06 并入 P5/M5.3）**。其余 agent 留空壳，不首发。
- **zcode**：hook 协议与 ClaudeCode 高度同构——模板变量 `${CLAUDE_PROJECT_DIR}`/`${ZCODE_PROJECT_DIR}` 双别名、`PreToolUse` 可返回 `allow`/`ask`/`deny` 三值决策（与三档一一映射）、exit 0/2 语义一致；支持 `type:"process"` 参数向量执行（Windows 免 shell 转义）。两点 zcode 文档未写死，实现期探针实测后再定型（不预设）：① stdin 输入载荷键名（是否同 ClaudeCode 信封）；② `PermissionRequest` 能否返回三值决策（能则改挂它，语义上更正的拦截点；否则用已验证的 `PreToolUse`）。交付形态取插件分发（`hooks/hooks.json` 自动启用 hook runner；配置文件里的 hooks 默认禁用）。
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

#### Hook 接入失效模式与保障边界（定稿）

hook 接入是权限门的「最后一米」——管线内部的 fail-safe 再完备，hook 没被拉起来就全盘失效。本节把各 agent 接入形态的失效模式与兜底责任固定成表，作为**后续新 agent 接入的验收对照表**：每接入一个新 agent（OpenCode / Codex 等），逐行核对「该失效模式在其形态下是否存在、由哪层兜住、怎么验证」，不重新推导。

兜底责任分三层：**A. agent 侧机制**（agent 提供的保障，如插件自动启用）；**B. 我方管线内部**（fail-safe confirm / connect-or-spawn 降级）；**C. 部署验收实测**（无机制可堵时的唯一覆盖手段）。每条失效模式至少一层覆盖，下表 #2 是唯一待补的洞。

| # | 失效模式 | 兜底层 | 说明与验证 |
|---|---|---|---|
| 1 | 配置文件 hook 默认禁用被忘开（zcode 配置文件形态特有，须显式 `hooks.enabled: true` 才跑） | A | 插件贡献的 hook 自动启用 hook runner（zcode 配置指南核实）。验证：M5.3 探针「插件分发实际触发」（含启用路径）。 |
| 2 | hook 二进制路径失效 / 被卸载（hook 进程根本起不来） | **待补** | 缺口如实登记：此时 agent 侧行为（放行 or 阻断）未验证——我方 fail-safe 只覆盖「进程起来后」的失效。探针待办：实测 zcode 对 hook 进程启动失败的语义；若放行，则「二进制已安装 + 路径可达」成为部署清单必查项。 |
| 3 | hook 进程已拉起，但内部崩溃 / 超时 / serve 端点不可达 | B | fail-safe confirm（裁决前任何异常落 confirm）+ connect-or-spawn 降级（serve 不可达 → 本进程跑全量管线，绝不无裁决放行）。既有单测与契约测试覆盖（M4.1、P1 起）。 |
| 4 | Crush 类「配置即生效、无启用门槛」形态的静默失效（配错路径 / 拼写错，无任何机制提醒） | C | 无机制可堵，部署验收实测触发是唯一覆盖：M5.2 契约用例集（实现层）+ 部署后实测 hook 确实触发。 |
| 5 | 用户手动禁用插件 / 删除配置 | — | 信任边界，不设防，如实声明。 |

ClaudeCode / Crush 实机部署形态是否存在类似 zcode 的启用门槛：**未核实**，实机验证时一并确认后回填本表。

## 配置格式与脚本边界（v1 定稿）

> [!IMPORTANT]
> **状态：v1 定稿（2026-09-06 升格）**。P2 六项里程碑（解析模型 / 字段级继承合并 / 双表三桶查表 / 知识库归一 / 双层 lint / 默认包生成）+ 样例仓库端到端验收全绿后升格；解析器与文档示例逐行一致（默认包模板 = 本节示例块，测试钉死）。89 条回归用例的「引擎 + 默认配置 fixture」迁移属引擎侧收尾，在 P3 末（M3.3）完成；默认 `rules.rhai` 四类谓词随 M3.2 落地。实现期修订仍就地更新并留更正记录；与既有定稿的冲突见本节末尾[更正登记](#更正登记对既有定稿)。

### 设计原则

- **声明层零条件判断**：`rules.toml` 只做「命令归属查表」（无一处 if/参数检查）；一切带条件判断的分类（两态子命令、参数内容检查、跨命令检查）全部下沉脚本层。这是对定稿「能声明表达的进 TOML」的收窄，见[更正登记](#更正登记对既有定稿)。
- **命令中心**：配置按「命令 → 子命令/flag 分桶」组织，不设命名命令集合与规则链（替代定稿的 `[[rules]]` + first-match-wins）。
- **作用域即结构**：文件顶层分 `[local]` / `[global]` 两表——`[local]` = 效果不出项目（其内 allow 一律带路径逃逸检查）；`[global]` = 允许影响项目外（其内 allow 豁免逃逸检查，如团队统一放行的 docker）。

### `rules.toml` 结构（定稿）

```toml
# 裸键区（所有表头之前）
version    = 1                              # 配置文件格式（schema）版本，非用户内容版本
default    = "confirm"                      # 未命中任何配置的命令
precedence = ["deny", "confirm", "allow"]   # 桶间优先级（可调），见「查表顺序」条

# ═══ [local]:效果不出项目 —— 此表内所有 allow 默认带路径逃逸检查 ═══
[local]
allow   = ["ls", "cat", "grep", "rg", "find", "head", "tail", "wc", "pwd", "echo",
  "printf", "which", "file", "stat", "sort", "uniq", "comm", "diff", "md5sum",
  "sha1sum", "sha256sum", "sha512sum", "date", "env", "du", "nl", "less", "more",
  "tree", "ls-files", "rev-parse",
  "gofmt", "black", "ruff", "dprint", "make", "just", "pytest",
  "touch", "mkdir"]
confirm = ["rm", "pip", "pip3", "npx", "curl", "wget"]
deny    = ["sudo", "dd", "shutdown", "mkfs", "mkfs.ext2", "mkfs.ext3",
  "mkfs.ext4", "mkfs.vfat", "mkfs.fat", "mkfs.xfs", "mkfs.btrfs", "mkfs.ntfs"]
  # deny：系统级破坏/提权硬阻断（guard.py DESTRUCTIVE 收窄为四族；声明层无前缀
  # 匹配，mkfs.* 逐字枚举；rm 有日常可逆用途保留 confirm 档，reboot/halt/parted
  # 等项目可按需自行补 deny 桶）

[local.git]
allow.sub    = ["status", "log", "diff", "show", "branch", "--version", "remote",
  "ls-files", "config", "rev-parse", "blame", "shortlog", "tag", "help", "describe",
  "check-ignore", "show-ref", "for-each-ref", "cat-file", "ls-tree", "diff-tree",
  "name-rev", "merge-base", "rev-list", "diff-index", "diff-files", "check-ref-format",
  "add", "commit", "checkout", "switch", "mv"]
confirm.sub  = ["rm", "restore", "reset"]   # reset 软/mixed 走确认；--hard 在 deny.flag
deny.sub     = ["push", "pull", "clean", "rebase", "revert", "cherry-pick",
  "fetch", "gc", "prune", "filter-branch", "reflog"]
confirm.flag = ["--output", "-o", "--pretty", "--format", "--config", "-c",
  "--force", "-f", "--in-place", "-w", "--write", "-h"]   # -h 笔误登记保留（更正登记 5）
deny.flag    = ["--hard"]

[local.npm]
confirm.sub = ["install", "i", "ci", "add", "uninstall", "remove", "update", "publish"]
default     = "allow"   # 节内 default 覆盖顶层 default（npm run 等其余子命令放行；
                        # npm exec/x 经知识库 alias 归一到 npx 查表，不走本节 default）

[local.pnpm]
confirm.sub = ["install", "add", "remove", "upgrade", "update", "publish"]
default     = "allow"   # pnpm dlx 默认放行（默认包自定策略；需收紧可把 dlx 补进 confirm.sub）

[local.yarn]
confirm.sub = ["install", "add", "remove", "upgrade", "update", "publish"]

[local.bun]
confirm.sub = ["install", "add", "remove", "upgrade", "update", "publish"]

[local.cargo]
allow.sub = ["build", "test", "fmt", "check", "clippy"]  # install 等其余子命令落顶层 confirm

[local.go]
allow.sub = ["build", "test", "vet", "fmt", "mod"]       # go run 执行任意代码，不入（落 confirm）

# ═══ [global]:允许影响项目外 —— 此表内 allow 豁免逃逸检查 ═══
[global]
allow = []    # 默认配置为空;项目特例/团队统一放行(如 docker)写这里,随项目提交
```

- **三桶**：每命令一节（或头部裸列表），`allow` / `confirm` / `deny` 三桶 + `sub`（子命令）/ `flag`（flag）两个子键；flag 配置写**任一形态**即可，等价形态由知识库 `same_flag` 闭包覆盖（原「长写+简写显式双配」原则收窄，见[更正登记](#更正登记对既有定稿)第 8 条）。
- **deny/confirm 单份**：deny/confirm 是安全侧，与作用域无关（`git push` 在项目内同样该拒），全部写在 `[local]` 下；`[global]` 表实际只承载 allow。
- **查表顺序（2026-09-06 钉死）**：命令**节优先**——裸列表词条只是「该命令整命令入桶」的语法糖，同层内被命令节遮蔽；命中 `[global].allow` 的命令整命令豁免（同命令两表皆现时 `[global]` 优先——全局放行是更强的承诺）；同表内按 `precedence` 顺序查桶，先命中先裁决。
- **precedence 与 lint 的分工**：precedence 不可去除——运行时存在 lint 覆盖不了的**多命中合成**（不同维度 token 各自合法命中不同桶：`git show --output=x` 中 `show` 命中 allow.sub、`--output` 命中 confirm.flag，须有序合成；复合命令组合裁决同此）。lint 上线后其角色从「同 token 双桶的静默兜底」收窄为「合法多命中的合成规则」（同 token 双桶由 lint 告警暴露，见 [D-04](decisions.md#d-04-precedence-与-lint-的分工)）。
- **兜底**：未命中任何配置 → 顶层 `default`；各节可用节内 `default` 覆盖（跨层按字段级继承：高层节未写 `default` 则继承低层节，低层也无才落顶层）。
- **version**：裸键 `version` 标识配置文件格式（schema）代次，非用户内容版本（策略增删由 git 追踪，不改号）；某次升级改变键名/结构后，加载器按 `version` 识别旧文件并明确报「格式过旧」或迁移，不静默按新语法误解析。知识库文件同套语义。

### 命令知识库（bucket 框架，定稿）

> 与用户策略文件分离的**命令元数据**：只记录关于命令世界的通用**事实**（读写属性、别名、联系），不记录任何裁决偏好。设计论证与被否决方案见 [D-01](decisions.md#d-01-命令知识库框架)。

**定位与分发**：

- 知识库独立于 `rules.toml`（策略），随软件分发 **main**（默认知识包，随默认配置生成机制一并落盘为外部数据文件）；参照 scoop bucket 模式：可添加、可删除、可换源（含官方 main），社区/团队可维护自己的 bucket。
- **删光 = 不做语义检查**：知识库全部移除后，lint 自动降级为纯结构检查，判定完全不受影响（策略文件自足承担裁决正确性）。已知后果：别名/flag 等价随删除失效，等价命令按各自字面查表，可能出现「npx 被 confirm 而 npm exec 走另一桶」的差异——属删除的已知后果而非故障；裁决日志 `kb` 字段对此自证（见[日志](#日志格式先行开关位置-p4-定)）。
- 与零内置策略的边界辨析：**知识库不产生裁决**——「curl 可能写」是事实，「curl 该 confirm」才是策略；知识库再大也推不出任何一条裁决，裁决只来自策略文件。

**条目文法**：一命令一表头 `[bin]`；`sub` / `flag` 是仅有的两个**保留结构键**（点号打开子命令/flag 条目空间，值用单行 inline table）；其余键为直接书写的属性槽位。一个命令的知识只在一处：

```toml
# knowledge.toml（main，随默认配置生成机制落盘）
version = 1

[npx]
may_write = true                 # 属性：下载并执行任意包

[npm]
sub.exec = { alias_of = "npx" }  # 联系：等价命令（运行时归一）
sub.x    = { alias_of = "npx" }

[pip3]
alias_of = "pip"

[curl]
may_write   = true
write_flags = ["-o", "--output"] # 属性：带这些 flag 会写文件

[git]
sub.branch     = { write_tokens = ["-d", "-D", "-m", "-M", "--delete", "--move", "--create"] }
sub.remote     = { write_tokens = ["add", "set-url", "remove", "rename", "set-head", "set-branches"] }
sub.tag        = { write_tokens = ["-d", "--delete", "-a", "-s", "-m", "-f", "-u", "--annotate"] }
sub.config     = { write_arg_count = 2 }   # 位置参数 ≥2 即写形态
flag."--force" = { same_flag = "-f" }      # 联系：flag 等价
flag."--hard"  = { irreversible = true }   # 属性：破坏性参数

[make]
delegates = "Makefile"            # 联系：委托执行项目内文件中的任意命令

[sudo]
wraps = "*"                       # 联系：包装壳（v1 仅登记）
```

**槽位封闭集（v1 共 10 个）**——按消费机制分组，完整建模依据见[单命令建模](#单命令建模定稿)：

| 组 | 槽位 | 适用层级 | 记录的事实 | 消费机制 |
|---|---|---|---|---|
| 运行时归一 | `alias_of` | 命令/子命令 | 本条目是别处的等价别名（npm exec ≡ npx） | 查表前归一改写 |
| 运行时归一 | `same_flag` | flag | 与另一 flag 等价（--force ≡ -f） | flag 归一到等价类规范形 |
| 运行时归一 | `takes_value` | flag | 该 flag 后跟一个值 | 引擎分解 `--output=x`/`-o x`/`-oX` 值边界，归一不丢值 |
| lint+脚本数据源 | `may_write` | 命令/子命令 | 有写的可能（npx 执行任意包） | lint 建议；不改裁决 |
| lint+脚本数据源 | `write_flags` | 命令/子命令 | 带这些 flag 才会写（curl -o） | 同上 |
| lint+脚本数据源 | `write_tokens` | 子命令 | 这些 token 出现即写形态（branch -d） | 默认 rules.rhai 两态判定数据源 |
| lint+脚本数据源 | `write_arg_count` | 子命令 | 位置参数 ≥N 即写形态（git config ≥2） | 同上 |
| lint+脚本数据源 | `irreversible` | flag | 破坏性/不可逆参数（--hard） | lint 建议；脚本数据源 |
| lint 提示 | `delegates` | 命令 | 实际执行项目内文件定义的命令（make→Makefile） | lint 提示「allow 它 = 允许执行被委托物」 |
| 登记后置 | `wraps` | 命令 | 包装壳（sudo/env/nice/xargs，危险性由被包裹命令决定） | v1 不消费；剥壳归一后置（sudo 已在 deny 桶兜住） |

两条设计原则：

- **条目开放、谓词封闭**：数据条目人人可写（社区/团队 bucket），但槽位种类由引擎版本定义——一类槽位绑定一个明确的消费机制；不提供任意谓词表达式，防止知识库演化为绕过零内置策略的隐性规则系统。新联系类型 = 引擎加新槽位并登记消费机制。
- **运行时只消费等价类，不消费属性**：引擎判定路径只使用 `alias_of`/`same_flag`/`takes_value`（归一）；属性类槽位只进 lint 与脚本层（默认 rules.rhai 读 `write_tokens`/`write_arg_count` 做两态判定——知识库删光时脚本查不到数据 → confirm 兜底）。

**归一机制**：命令进入管线后、三桶查表前，按知识库改写到规范形——`npm exec foo` → `npx foo`（bin+子命令 → 目标 bin，参数原样保留），链式改写到不动点（a→b→c），加载期防环校验（a→b→a 报配置错误）；日志记录归一链。归一只做名字改写，**绝不做语义变换**（不动参数值、顺序、结构）。

**lint 规则集**（双层，随版本扩充）：

- 结构类（引擎内置，无需知识库）：同文件同 token 多桶（按 precedence 取生效桶 + 告警，不拒绝加载）；同 bin 裸列表与命令节并存；被 precedence 压死的死词条。
- 语义类（读知识库）：allow 一个 `may_write` 命令 → 建议；等价冗余（allow `pip` 又 allow `pip3`，归一后后者永不命中，属死词条）；`same_flag` 等价类跨桶冲突（confirm `-f` 但 allow `--force`）；未知子命令拼写提示（`git stauts` → `status`）。

**分期**：v1 单文件 main（随默认配置生成机制落盘）；多 bucket 管理（增删/多源合并/优先级）与配置编写时提示登记为 P4 后专项。

### 单命令建模（定稿）

知识库槽位对单条命令的覆盖完备性由「**槽位跟着消费机制走**」标准裁定：一个维度只有存在明确消费者（归一 / lint / 脚本数据源）才配拥有槽位，没有消费者的维度等真实策略需求出现再加。单命令各部分的责任分工（建模论证见 [D-06](decisions.md#d-06-单命令建模完备性标准)）：

| 命令的部分/维度 | 例子 | 归谁负责 |
|---|---|---|
| bin 裸名 | `rm`、`git` | 知识库 `[bin]` 表头 |
| bin 路径形态 | `/usr/bin/rm`、`./x.sh` | 引擎归一：绝对路径取 basename 再查表；项目内脚本无法预先入库 → 未识别走 confirm 兜底 |
| 环境变量前缀 | `FOO=bar cmd` | 引擎解析剥除（bash 语法，非命令属性） |
| 子命令 / flag / 位置参数 | `git branch -d`、`git config a b` | 知识库槽位（`sub`/`flag` 结构键 + 10 槽位） |
| 参数内容模式 | curl 参数含 `\|`、sed 脚本体 | 脚本层（这是谓词不是事实，按「谓词封闭」原则永不进知识库） |
| 写重定向 / 复合拼接 | `> f`、`a && b` | 引擎原语（shell 语法：写重定向检测 / flatten 拆分） |
| 包装/放大壳 | `sudo`/`env`/`xargs` | `wraps`（v1 登记后置） |
| 委托执行 | `make`、`npm run` | `delegates`（lint 提示） |
| 网络访问/平台差异等 | curl 联网、GNU vs BSD | 不进：无消费机制（记了没人读的死数据），按需扩展 |
| 完全未知的命令 | agent 自造的 `ll`（非交互 shell 不展开用户 alias） | fail-safe → confirm |

### 脚本层职责边界（定稿）

一切条件判断进脚本（`rules.rhai`），分四类：

1. **两态子命令**：`git branch/remote/tag/config` 的 action-token 命中 → confirm（纯读保持 TOML allow）；`git config` ≥2 位置参数 = 写 → confirm。写形态 token 与位置参数阈值**读知识库** `write_tokens`/`write_arg_count` 槽位（数据与策略分离；知识库缺失时脚本查不到数据 → confirm 兜底）。
2. **参数内容检查**：`find` 带 `-delete/-exec/-execdir/-ok/-okdir` → confirm；`curl/wget` 参数含 `|` → deny（纯拉取维持 confirm）。
3. **跨命令检查**：管道 sink（管道下一段为 bash/sh/zsh/python/python3/perl/php/ruby）→ 整体 deny。
4. **原语组合升级**：readonly/allow 命中但带写 flag 或写重定向 → confirm；`[local]` allow 命中但路径逃逸 → confirm。

- **引擎保留原语**（脚本可组合、不可绕过）：**别名归一**（查表前按知识库 `alias_of`/`same_flag`/`takes_value` 改写规范形）、写重定向检测（丢弃式 `/dev/null` 与 fd dup/close 豁免）、写 flag 检测（读 TOML flag 桶）、路径逃逸检查（`[local]`/`[global]` 表的豁免语义在引擎实现）、unparseable → confirm 兜底、复合命令组合裁决（任一 deny→deny / 全 allow→allow / 否则 confirm）。
- **脚本 allow 契约（v1 定稿，2026-09-06）**：**脚本 v1 无放行权**——`check()` 返回 `allow` 即契约违约，引擎拒绝并 fail-safe confirm；脚本升级权仅到 confirm/deny，放行语义完全由 `rules.toml` 承载（`[global]` 整命令放行特例已覆盖）。原方向「允许对显式枚举命令返回 allow」搁置：图灵完备脚本上「禁无条件兜底」无法机械校验（静态检查可被 `if true { "allow" }` 平凡绕过），结构性禁止才给出可保证的安全性质；带条件的脚本 allow 登记为后续扩展，须先设计机械校验（见[更正登记](#更正登记对既有定稿) 10）。→ **后续扩展设计已定稿（2026-09-06）**：见[脚本条件放行（script_allow，定稿）](#脚本条件放行script_allow定稿)——由「绝对禁止」演进为「注册式 + 声明对账」的受控开口，实现登记 M4.0（更正登记 11）。
- **脚本词汇约定（2026-09-06 定稿，代码落地随 M4.0 前置小改）**：
  - **决策值 = 类型化四变体 + 只读模块常量 `decision::`**（2026-09-06 M6.1 枚举化落地）：`ALLOW` / `CONFIRM` / `DENY` / `PASS` 四值——无意见也是一种决定，`PASS` 语义为「不表态、交还查表基线」（Rhai 映射同类型常量，Lua 引擎映射 nil）。常量为封闭枚举值，脚本无法拼出第四种决策值，非法值在边界（解析/返回）即报错；限定名只读，变量遮蔽污染不了词汇表。引擎同时接受等价裸字符串，在返回边界统一解析（双保险）；裸 allow 值（常量或字符串）一律违约。
  - **ctx 可选字段缺省 = 空字符串**：`ctx.sub` 无子命令时为 `""`，谓词写 `ctx.sub != ""`——不向脚本暴露解释器内部的 unit/nil 语义；「无意见」的契约表述随之引擎无关化。
  - **ctx 彻底封装（2026-09-06 M6.1 落地）**：传给脚本的是自定义类型（不暴露裸 map/unit），字段经只读 getter 暴露——脚本侧语法保持 `ctx.bin` 属性形式（rhai 的属性访问本质即方法调用糖；Lua 侧 userdata 同形），默认模板与既有脚本零改动。

### 脚本条件放行（script_allow，定稿）

> 状态：**设计定稿（2026-09-06），实现登记 M4.0**（独立里程碑，P4 前后皆可插入，启动需用户授权）。本节是 v1 allow 契约（脚本无放行权）的**受控开口**：机制 = **注册式 + 声明对账**——脚本不能凭空说出 allow，只能激活用户在 `rules.toml` 中亲笔声明的放行条目。图灵完备脚本上「禁无条件兜底」无法机械校验，因此可校验面从「脚本全文」缩到「结构化的放行条目」：校验的是声明对账，不是脚本行为推断。

**声明文法（双形态，同一声明集）**：

```toml
[local]
script_allow = ["ls", "docker"]   # 顶级列表：整命令粒度批量声明

[local.ls]
script_allow = true               # 命令节键：与该命令其他配置同址的显式形态
```

- 两形态汇入同一**声明集**（按 bin 名索引，附声明所在表的元数据）；`script_allow` 是列表字段，跨层合并适用 D-02（数组 = 覆盖 / inline table `add`·`remove` = 增删），项目层可整体替换或增量调整用户层声明。
- **声明不是规则、不进查表路径**：与 allow/confirm/deny 三桶是不同命名空间，无遮蔽关系；它只是脚本 allow 激活的对账白名单——放行面 = 用户声明集 ∩ 脚本条件命中，脚本永远无法扩大它。

**引擎机制五件套**（机械校验，无一步依赖对脚本行为的推断；前两步在加载期，失败 = **拒载整个脚本** + fail-safe confirm）：

1. **加载期字面量提取**：AST 静态提取脚本中全部 `allow("…")` 调用的参数；实参为变量 / 循环变量等运行时确定的形态 → 拒载。常量拼接（`allow("cu"+"rl")`）被 rhai 优化器折叠为字面量，按折叠值提取并对账——静态提取集与运行实参恒一致，安全审计面等价（[更正登记](#更正登记对既有定稿) 15）。
2. **声明集对账**：提取集 − 声明集 ≠ ∅ → 拒载（脚本作者无法替用户决定放行谁）。
3. **运行时双保险**：`allow(name)` 执行时再校验 name ∈ 声明集。
4. **定稿点作用域化逃逸检查**：激活后按声明元数据——local 声明 → 对原始命令参数执行 `path_escapes`，逃逸则激活失败降 confirm；global 声明 → 豁免；两表皆声明 → global 胜（M2.3「更强的承诺」同规）。检查在引擎、单点、脚本不可绕过也不可代劳（脚本自查通过照样再查，脚本没查引擎兜底）。
5. **deny 终审**：查表落 deny 的命令，allow 激活一律无效——不可逆操作不给任何机制留放行通道。

**脚本侧形态与权力边界**：`allow("bin名")` 只能以字面量名调用并作为 `check()` 返回值；裸 `"allow"` 字符串仍是契约违约（放行必须走带名通道，引擎才有的对账）。作用域概念**不进脚本词汇**——脚本写 `allow("a")`，语义由引擎按声明位置解析，脚本既不区分也不选择作用域。

| 初步裁决 | 脚本能做什么 |
|---|---|
| deny | 不可翻（终审） |
| confirm | 升 deny；或对**已声明** bin 经条件激活改判 allow |
| allow | 升 confirm/deny；对已声明 bin 的激活为幂等 no-op |

**示例**（可载入；对应用例「写重定向到仓库内放行、逃逸确认」——该条件 TOML 表达不了，正是本机制的正当用例）：

```rhai
fn check(ctx) {
    if ctx.bin == "ls" && ctx.writes_redirect && all_args_in_repo(ctx) {
        return allow("ls");      // "ls" 已声明 → 合法激活；逃逸由定稿点复查
    }
    if ctx.writes_redirect {
        return "confirm";
    }
    ""
}
```

拒载形态（死在机制 1/2）：`allow("curl")` 而 curl 未声明；`let b = "cu"+"rl"; allow(b)` 动态名；循环变量分发。

**lint 新增三条**（随 M4.0）：**死声明**（声明无任何脚本引用 → 告警）；**may_write 建议**（声明 bin 属知识库 `may_write` → 与 allow-may-write 同级建议）；**冲突提示**（声明 bin 在声明层存在 deny 桶条目 → 提示「脚本放行受 deny 终审限制」，校准放行面预期）。

**配套约定**：声明集是「脚本放行面」的可审计清单；编辑器补全索引可由同一解析路径生成（见 [ROADMAP](../cairn/ROADMAP.md) 编辑器支持候选）。

### 日志（格式先行，开关位置 P4 定）

JSONL 一行一条裁决，字段覆盖：命令原文、结果、触发层级、触发条件：

```json
{"ts":"2026-09-06T14:03:22Z","mode":"serve","agent":"crush",
 "command":"git push --force origin main",
 "decision":"deny","reason":"git.deny.sub: push",
 "source":{"layer":"project","file":".crush-tether/rules.toml",
           "entry":"git.deny.sub","match":"push"},
 "kb":["main"],"normalized":null,
 "script":{"file":null,"rule":null}}
```

- `source.layer` ∈ global/user/project/explicit/script/default；脚本激活/改判时 layer=script 并填 `script.file`（区分项目层与用户层脚本文件，文件名随 `--engine` 取 `rules.rhai`/`rules.lua`；`script.rule` v1 恒 null——脚本无命名规则概念，字段为后续扩展保留）。v1 可达值：user/project/explicit/script/default；`global` 待全局层发现落地（v1 不做）。
- `kb`：本次裁决加载的知识库 bucket 列表；`[]` = 知识库已删光——**当前配置未经任何内置规则校验、别名/flag 归一未生效**，日志自证可见。`normalized` 记录归一链（如 `"npm exec → npx"`），未归一为 `null`。
- serve 加载/**热重载成功**时另记一条非裁决事件行（`type:"load"`，含 kb 状态与 lint 告警），冷热路径都留痕；热重载失败不留痕（快照未换，stderr 告警）。
- serve 模式由 serve 单点写（复用行协议已有字段），hook 降级路径与 check/benchmark 自写一行（mode 字段区分 hook 降级与独立 check）；人读视图由后续 `crush-tether log` 子命令渲染 JSONL（人看视图、程序读原文）。默认开/关在 P4 落地 serve 时定。
- **实现注记（2026-09-06，M4.3 落地，[D-07](decisions.md#d-07-裁决日志默认开与落盘形态)）**：默认开（`CRUSH_TETHER_LOG=0|off|false` 关）；落盘 `<project>/.crush-tether/decisions.jsonl` 追加写、写入失败静默；`ts` 为 UTC RFC3339（零依赖，本地时区由人读视图渲染）；热重载信号在 serve 主线程请求间隙消费（改规则后的第一个请求触发重载并留 load 事件）。

### 层间合并（字段级继承，定稿）

- 优先级不变：项目 > 用户 > 全局（见 [配置分层与优先级（定稿）](#配置分层与优先级定稿)）。
- **字段级继承**（低层 = 父类，高层 = 子类）：子层**未定义**的键整体继承父层；**定义了**即覆盖（遮蔽）。标量（`default`/`precedence`/`version`）写值即覆盖。
- **列表值双形态**：数组 = **覆盖定义**（本份就是全部）；inline table `{ add = [...], remove = [...] }` = **继承低层并增删**——无保留字、无前缀歧义，flag 桶同样支持剔除：

```toml
# 用户层（父类）
[local]
allow = ["ls", "cat", "grep", "curl"]
```

```toml
# 项目层（子类）
[local]
allow   = { add = ["jq"], remove = ["curl"] }   # 继承全部、剔除 curl、追加 jq
confirm = ["rm", "pip"]                          # 数组 = 覆盖：本份就是全部

[local.git]
deny.sub   = ["push", "filter-branch"]           # 覆盖
allow.flag = { remove = ["-h"] }                 # 继承并移除（flag 也能删）
```

- 纯继承不改任何东西的场景无需写该键（未定义即继承），语法无冗余；「照单继承 / 微调 / 完全换一套」三种意图一套模型表达。论证与被否决方案（token 级合并、列表内 `super`/`-token` 保留字、挪桶表删除）见 [D-02](decisions.md#d-02-字段级继承合并模型)。

## 更正登记（对既有定稿）

> 以下为对本文档已定稿措辞的更正（草案阶段调整，非推翻方向），原定稿表述处已加更正指针，不静默覆盖：

9. 「配置格式与脚本边界（草案 v1）」于 **2026-09-06 升格定稿**：P2 六项里程碑（M2.1 解析模型 / M2.2 字段级继承合并 / M2.3 双表三桶查表 / M2.4 知识库归一 / M2.5 双层 lint / M2.6 默认包生成）+ M2.7 样例仓库端到端验收全绿；默认包模板与本文档示例块逐行一致（测试钉死）。第 1–8 条更正随升格固化入正文。遗留：89 用例「引擎 + 默认配置 fixture」迁移在 M3.3；默认 `rules.rhai` 四类谓词在 M3.2 并入生成包。
10. 「脚本 allow 契约：允许显式枚举 allow」（原「新增待定稿」方向）→ **2026-09-06 定稿为「脚本 v1 无放行权」**：返回 `allow` 一律契约违约（fail-safe confirm）。理由：图灵完备脚本上「禁无条件兜底」无法机械校验，结构性禁止给出可保证的安全性质；`[global]` 整命令放行特例由 TOML 承载。带条件的脚本 allow 登记为后续扩展，须先设计机械校验（见[脚本层职责边界](#脚本层职责边界定稿)）。
11. 「脚本 v1 无放行权」（第 10 条）的后续扩展**设计已定稿（2026-09-06）**：[脚本条件放行（script_allow，定稿）](#脚本条件放行script_allow定稿)——注册式 + 声明对账的受控开口，allow 契约由「绝对禁止」演进为「放行面 = 用户声明集 ∩ 脚本条件命中」，deny 终审与裸 `"allow"` 违约不变；实现登记 **M4.0**（独立里程碑，启动需用户授权），脚本词汇约定修订（decision 常量四值 / ctx 空串约定）随其前置小改落地。
12. 「筛查管线 `Parse → Flatten → Extract → Match(逐规则) → Verdict` 与 `pub trait Rule` 规则链接口」→ **2026-09-06 重画为双阶段管线**（① TOML 查表一般层 → ② 脚本评估特殊层 → ③ 定稿唯一放行出口 → ④ 组合裁决），`Rule` trait 由规则注入式 `engine::decide_with` + `script::RuleEngine` 替代；三个安全性质（定稿点唯一 / 逃逸检查挂定稿点 / deny 终审）随形状显式钉死。见[筛查管线与编译期组装（定稿）](#筛查管线与编译期组装定稿)。
13. 「默认包缺口（M3.3 变更记录登记的宽松断言）」→ **2026-09-06 补齐**（用户拍板推荐值）：①知识库 `[git]` 补 `sub.remote`/`sub.tag` 的 `write_tokens`（忠实平移 guard.py `GIT_ACTION` 数据，裸创建 `git tag <名>` 原工具同样不视为写）；②`[local]` 补 `deny` 裸列表（sudo/dd/shutdown/mkfs 族——guard.py `DESTRUCTIVE` 收窄为四族，`rm` 保留 confirm 档、reboot/halt/parted 等留项目自补）。M3.3 变更记录中 remote/tag 与 mkfs/dd/shutdown/sudo 两行宽松断言随之消解。
14. 「日志默认开关在 P4 定」→ **2026-09-06 定稿落地**（M4.3，[D-07](decisions.md#d-07-裁决日志默认开与落盘形态)）：默认开、落盘 `.crush-tether/decisions.jsonl`、`ts` 用 UTC（本地时区渲染挂 `log` 子命令）、热重载信号在请求间隙消费（重编译留主线程——rhai Engine 非 Send）。`source.layer` 经合并层 Provenance 实现全层溯源，字段与示例一致。
15. 「机制 1 字面量提取：字符串拼接 → 拒载」措辞 → **2026-09-06 收窄**：变量/循环变量等运行时确定的实参 → 拒载；常量拼接（`allow("cu"+"rl")`）被 rhai 优化器折叠为字面量，按折叠值提取并对账——静态提取集与运行实参恒一致，安全审计面等价。
16. 「默认包 pnpm dlx 经 alias 归一到 npx」注释 → **2026-09-06 如实化**：默认知识库无 `[pnpm]` 条目，归一不发生，`pnpm dlx` 落 `[local.pnpm]` default 放行。定性（延续 [D-05](decisions.md#d-05-guardpy-定位重置参考对象而非验收标准)）：默认包为本项目自定策略，guard.py 的规则只是参考对照、不作验收标准；需收紧可把 `dlx` 补进 `confirm.sub`。
17. 「script_allow 机制 1 = AST 静态提取」→ **2026-09-06 M6.1 补充**：Lua 侧无公开 AST，机制 1 退化为**注释剥离后的保守词法扫描**（识别 `allow("…")`/`allow('…')`；非引号实参拒载；行/块注释先剥离防误拒；字符串内含 `--` 的极端形态漏收不误拒）——机制 2/3（声明集对账拒载、运行时双保险）语义不变，审计面不缩小；rhai 侧仍为 AST 提取。同批：机制 1/2/3 对 Lua 生效、定稿点（逃逸检查/deny 终审）引擎无关复用。
18. 「Lua 引擎定型：限流 = 指令数 hook……死循环/深递归/OOM 有界」→ **2026-09-06 补正（审查修复）**：hook 须为**全局形态**（`set_global_hook`）才覆盖脚本自建协程——初版用线程级 `set_hook`，协程内代码完全逃逸指令预算（探针实测：协程内 200 万次循环毫秒级完成、正常返回）。修复后主线程与协程均被计数；**语义边界**：`coroutine.resume` 类 pcall 吞协程内错误——超预算协程被终止（DoS 已阻止）但脚本不报错、继续走到返回值，不转化为 fail-safe confirm（rhai 侧限流错误直接中断脚本、Err → confirm，两引擎在此形态上有差异，安全性等价：循环均被有界终止）。
19. DSL 节「安全原语（`writes_file`/`path_escapes`/`deny`）」清单 → **2026-09-06 如实化**：实际注册原语为 `path_escapes`/`inside_repo`/`kb_*` 知识库数据源族 + `allow("bin")` 受控激活通道（M4.0）；`writes_file`/`deny` 原语从未存在——写特征经 ctx 字段 `writes_redirect`/`pipe_to_shell` 暴露，deny 是脚本返回值不是原语。历史遗留措辞（P3 前草案期写入），非近期引入。
1. 「能声明表达的进 `rules.toml`」→ 收窄为「**无条件的纯查表**进 `rules.toml`，一切条件判断进脚本」。
2. 「`[[rules]]` 规则链 + first-match-wins」→ **删除**，替换为「`[local]`/`[global]` 双表 + 每命令三桶查表 + 可调桶间优先级 `precedence`」。
3. 「命令集合并集（只增不减，`exclude` 表剔除）」→ 2026-09-05 替换为 token 级合并，**2026-09-06 再修订为字段级继承**（数组覆盖 / inline table 增删；「挪桶即剔除」弃用——挪桶是改判不是删除）；无需 `exclude` 表（见 [D-02](decisions.md#d-02-字段级继承合并模型)）。
4. guard.py `WRITE_FLAG_PREFIXES` 前缀匹配（`--output=` 等）→ 放弃，flag 显式枚举；「长写+简写双配」后由第 8 条收窄为 `same_flag` 等价闭包。
5. guard.py `WRITE_FLAGS` 中的 `-h` 疑似笔误（`git -h` 可改写行为的判定依据待查），草案 1:1 保留，实现期确认后剔除并在此登记。
6. 作用域由「每命令 scope 属性」方案（曾讨论）收敛为 `[local]`/`[global]` 顶层双表。
7. 「损坏 ≠ 缺失：留档 `.bak-<时间戳>` 后重生成默认」（2026-09-04 定稿）→ 收窄为「仅文件不存在才生成；解析失败 → 告警 + fail-safe confirm 兜底，原文件不动」（2026-09-06，见 [D-03](decisions.md#d-03-损坏重生成收窄)）。
8. 「flag 长写+简写显式并排枚举（双配）」→ 收窄为「配置写任一形态，等价由知识库 `same_flag` 闭包覆盖」（2026-09-06，知识库框架见 [D-01](decisions.md#d-01-命令知识库框架)）。

## 待定决策（见 cairn/ROADMAP.md）

> 原有三项（抽取方式 / 是否重写 / fail-safe 时机）已于 2026-09-04 定稿，见 [CAIRN-MOVED]；当前无未定项。P2+ 实现里程碑见 `cairn/ROADMAP.md` 推进计划。

- **抽取方式**：【当前】纯全局工具（本仓库为唯一实现，mdor 不再保留 crush-guard；Python 版随回归用例平移后退役）。
- **是否重写**：【当前】Rust 重写已落地（P0+P1 完成，`src/{model,cmd_parse,engine,channel}` + check 模式 + 回归测试 9/9 绿）。
- **fail-safe**：guard 任何环节崩了都保守降级为确认（输出 none）而非放行；解析失败（含 heredoc 无终止符）走 `unparseable` → confirm。
- **规则来源**：【当前】零内置策略（2026-09-04 定稿）——二进制为纯引擎，默认策略由项目侧生成的外部 `rules.toml` + `rules.rhai` 提供；三层皆缺才生成；损坏 ≠ 缺失（告警 + fail-safe confirm 兜底，原文件不动，[D-03](decisions.md#d-03-损坏重生成收窄)）；全局/用户层生成由命令提供（后期设计）。见 [零内置策略与默认配置生成（定稿）](#零内置策略与默认配置生成定稿)。

## 实现时的安全目标（fail-safe）

- guard 任何环节崩了都该**保守降级为确认**（输出 none），而不是放行。
- hook 失败 = fail-open 是安全上的反模式，必须用「输出 none」而非「依赖失败」来兜底。
