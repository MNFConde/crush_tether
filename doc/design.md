# design.md — crush_tether 设计文档

`crush_tether` 是 Crush 的**命令级 bash 权限门**：通过 PreToolUse hook 对每条 bash 命令做三档分类（`allow` / `confirm` / `deny`），返回给 Crush 决定是否放行。本文件是 crush-guard 抽取/重写的设计单一事实源；历史结论与教训见 `cairn/crush-guard-bash-gate.md` 和 `cairn/crush-guard-retrofit-options.md`。

## 目标

把 `mdor` 仓库内的 `crush-guard/`（Python + bashlex）独立化并（可选）用 Rust 重写，作为可复用、可配置的 bash 权限门，供本仓库和/或其他仓库（及多 agent）共用。除三档分类外，另含**可配置规则引擎**（TOML 声明层 + Rhai/Lua 脚本层）与**多 Agent 适配层**（Crush / ClaudeCode 首发），见 [规则引擎与配置（定稿）](#规则引擎与配置定稿)。

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
│   ├── main.rs           # #bin：装配壳，parse 参数、加载配置、调管线、输出裁决
│   ├── cli/              # 命令行入口/参数（--engine / --agent / --config）
│   ├── channel/          # agent 适配层（Crush / ClaudeCode）
│   ├── engine/           # 规则管线 + Rule trait + 内置规则 + 配置 merge
│   ├── config/           # TOML 声明层 + DSL 脚本层加载
│   ├── model/            # Cmd 特征对象 / Verdict / Decision 类型
│   └── cmd_parse/        # tree-sitter-bash 解析 + flatten + 各 has_* 边界检测
├── .cairn/               # Cairn 配置
├── cairn/                # Cairn 知识层
└── doc/design.md         # 本文档
```

> 说明：本工具为单一二进制 hook，核心逻辑（分类/配置/规则/适配）**无第二消费方**，故不拆 workspace（省去 resolver/共享依赖钉版/跨 crate 开销）。但为保留核心逻辑的可复用性与干净单测，采用**单 crate + `src/lib.rs` + `src/main.rs` 双入口**——库入口装 `model`/`engine`/`cmd_parse` 等分类逻辑，`main.rs` 仅做装配。逻辑模块间靠契约边界划分（`module-boundary-contract-design.md` 思想），不在目录上拆 crate。

### 分类器输入/输出契约（hook 协议）

- 输入：stdin（命令正文）+ 环境变量 `CRUSH_PROJECT_DIR` / `CRUSH_TOOL_INPUT_COMMAND`。
- 输出：如上三档分类语义表。

## 规则引擎与配置（定稿）

> 本节描述 crush-tether 的**可配置规则引擎**、**配置分层**、**脚本 DSL** 与 **Agent 适配层**。本节为这些主题的单一事实源，其余文档只回指、不复述。

### 配置分层与优先级（定稿）

- 解析优先级：**项目 > 用户 > 全局**（高层覆盖低层，不粘性）。`deny` 不被全局粘性锁定，均可被高层覆盖，属门卫而非沙箱。
- 显式覆盖：`--config <path>` 或环境变量 `CRUSH_TETHER_CONFIG`，优先级高于所有层。
- 项目配置最可靠来源为 `CRUSH_PROJECT_DIR`（Crush 对 hook 注入，且为路径逃逸检查基准）；缺失时从 cwd 逐级上溯最近 `.git` 或 `.crush-tether/`。
- merge 语义：标量覆盖；命令集合用并集（只增不减，`exclude` 表可显式剔除）；`[[rules]]` 在高层**前插**（first-match-wins）。

### 配置拆分（定稿）

```text
.crush-tether/                 # 项目级配置目录
├── rules.toml                 # 声明层：数据/默认值/命令集合/简单 when→decision 规则/security
└── rules.rhai 或 rules.lua    # 脚本层：跨命令逻辑/自定义谓词/fn 规则（按 --engine 选后缀）
```

用户级 `~/.config/crush-tether/`、全局（编译内置默认 + 系统路径）与项目同构。脚本层**同文件按优先级**：项目脚本最后执行，可作最终裁决。

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

内置规则以 `static RULES: &[&dyn Rule]` 编译期组装；配置中的 `[[rules]]` 实例化为 `DataRule` 塞入同一 Match 链。状态机在编译期拼好，无反射/无运行时注入。

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

## 待定决策（见 cairn/ROADMAP.md）

> 已有较多项在本轮定稿（配置引擎 / DSL / Channel），移至上方「规则引擎与配置（定稿）」节；此处仅保留**仍未定**的项。

1. **抽取方式**：子模块（保留 `crush-guard/` 路径、`.crushrc` 少改） vs 纯全局工具（mdor 删目录、所有仓库共用、需迁移 cairn/LOG）。**仍未定。**
2. **是否重写**：现在就 Rust 重写，还是先保持 Python（依赖 `uv sync` + venv）、重写留作独立里程碑。**仍未定。**
3. **fail-safe**（三层 guard 内部兜底 / wrapper 兜底 / `.crushrc` 指向 wrapper）：方向已定（见下「实现时的安全目标」），具体落地时机待定。

## 实现时的安全目标（fail-safe）

- guard 任何环节崩了都该**保守降级为确认**（输出 none），而不是放行。
- hook 失败 = fail-open 是安全上的反模式，必须用「输出 none」而非「依赖失败」来兜底。
