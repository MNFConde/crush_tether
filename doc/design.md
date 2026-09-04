# design.md — crush_tether 设计文档

`crush_tether` 是 Crush 的**命令级 bash 权限门**：通过 PreToolUse hook 对每条 bash 命令做三档分类（`allow` / `confirm` / `deny`），返回给 Crush 决定是否放行。本文件是 crush-guard 抽取/重写的设计单一事实源；历史结论与教训见 `cairn/crush-guard-bash-gate.md` 和 `cairn/crush-guard-retrofit-options.md`。

## 目标

把 `mdor` 仓库内的 `crush-guard/`（Python + bashlex）独立化并（可选）用 Rust 重写，作为可复用的 Crush bash 权限门，供本仓库和/或其他仓库共用。

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

### 目标 workspace 结构

```
crush_tether/
├── Cargo.toml            # workspace
├── crates/
│   ├── crush-tether-core/  # 纯 Rust：分类逻辑（AST 拉平/判定表/边界检测）
│   └── crush-tether-cli/   # 可执行包装：读 stdin/env 输出 JSON
├── .cairn/               # Cairn 配置
├── cairn/                # Cairn 知识层
└── doc/design.md         # 本文档
```

### 分类器输入/输出契约（hook 协议）

- 输入：stdin（命令正文）+ 环境变量 `CRUSH_PROJECT_DIR` / `CRUSH_TOOL_INPUT_COMMAND`。
- 输出：如上三档分类语义表。

## 待定决策（见 cairn/ROADMAP.md）

1. **抽取方式**：子模块（保留 `crush-guard/` 路径、`.crushrc` 少改） vs 纯全局工具（mdor 删目录、所有仓库共用、需迁移 cairn/LOG）。
2. **是否重写**：现在就 Rust 重写，还是先保持 Python（依赖 `uv sync` + venv）、重写留作独立里程碑。
3. **fail-safe**：hook 命令失败时 Crush 把非 0/2 退出当**非阻塞错误** → 工具照常放行 = **fail-open**，恰好要避免。正确做法三层 fail-safe：guard 内部兜底（永不崩成非 0/2）+ wrapper 兜底（唤不起 guard 输出 `{"decision":"none"}`）+ `.crushrc` 指向 wrapper。

## 实现时的安全目标（fail-safe）

- guard 任何环节崩了都该**保守降级为确认**（输出 none），而不是放行。
- hook 失败 = fail-open 是安全上的反模式，必须用「输出 none」而非「依赖失败」来兜底。
