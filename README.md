# crush-tether

Crush 命令级 bash 权限门（Rust 实现）。拦截 agent 即将执行的 bash 命令，按三档裁决：**allow**（放行）/ **confirm**（升级人工确认）/ **deny**（阻断）——宁可误拦、绝不误放（fail-safe confirm）。

- **零内置策略**：二进制是纯引擎，不内嵌任何规则；默认规则经 `crush-tether init` 显式生成到项目侧（`rules.toml` + `rules.rhai|rules.lua` + `knowledge.toml`）。三层皆缺 → 未匹配命令一律 confirm 兜底并提示 init。
- **双脚本引擎**：Rhai（默认）或 Lua（`--engine lua`），同一 `RuleEngine` 接口、同等沙箱限流；支持 `rule(名字, 优先级, 函数)` 声明式规则注册（规则可追溯、顺序显式）。
- **多 agent 适配**：Crush / ClaudeCode / zcode 契约适配层；会话内临时放行 + suggest 权限建议（零写入）。

## 文档

| 文档 | 内容 |
|---|---|
| [用户手册](doc/user-guide.md) | 安装、配置、agent 接入、CLI、日志、排查——使用者从这里开始 |
| [design.md](doc/design.md) | 设计单一事实源（机制与契约） |
| [decisions.md](doc/decisions.md) | 决策记录（为什么这么选） |
| [agent-compat-matrix.md](doc/agent-compat-matrix.md) | 实测兼容性矩阵 |
| [test-and-ci.md](doc/test-and-ci.md) | 测试方法与 CI 覆盖 |

## 构建

```sh
cargo build --release   # 产物 target/release/crush-tether
cargo install --path .  # 开发测试推荐：装入 cargo bin（已在 PATH），升级加 --force
```

crush/claude 的 hook 配置以裸命令名解析二进制，**必须在 PATH 可达**——hook 进程起不来时 agent 侧 fail-open 放行（失效模式 #2，见 design.md「Hook 接入失效模式」），装完建议 `where crush-tether` 验证一次。zcode 插件轨经 wrapper 拉起，二进制缺席时 wrapper exit 2 响亮报错（不静默放行）。

工具链钉版见 `rust-toolchain.toml`；依赖版本约束只钉在根 `Cargo.toml` 一处。

## 快速开始

```sh
crush-tether init   # 1. 生成默认配置包到 <项目根>/.crush-tether/
# 2. agent hook 配置把 PreToolUse 指向：crush-tether hook --agent <slug>（见用户手册）
# 3. 跑一条命令，确认 .crush-tether/decisions.jsonl 出现裁决记录
```

配置三层发现（**项目 > 用户 > 全局**，不粘性）、`rules.toml` 三桶查表与层间合并、`knowledge.toml` 知识库、脚本层写法、默认包裁决画像——详见[用户手册](doc/user-guide.md#配置)。规则文件改完即热重载生效，无需重启。

## 运行模式

```sh
crush-tether hook     --agent <crush|claudecode|zcode> [--engine rhai|lua] [--config p]
crush-tether serve    --project <dir> [--idle-exit 30]
crush-tether check    # 无子命令参数时默认 check（兜底/冒烟）
crush-tether benchmark
crush-tether init [--user|--global] [--engine lua]
crush-tether explain '<command>'   # 单发全溯源报告（调试）
crush-tether repl                  # 规则调试器：改规则即测
crush-tether suggest [--threshold 3] [--window 30] [--format toml|table]
```

- **hook**（agent 接入的主路径）：connect-or-spawn——尝试连接项目 serve 端点；无实例则 detached 拉起；仍失败则本进程跑全量管线，绝不无裁决放行。
- **serve**：常驻服务，一项目一实例；notify 热重载（失败保留旧快照）；空闲自动退出；裁决日志默认开（`CRUSH_TETHER_LOG=0|off|false` 关）落 `decisions.jsonl`；执行记录落 `executions.jsonl`（`CRUSH_TETHER_LEARN=off` 关采集）。
- **规则测试四件套**（explain / check --batch / check --cases / repl）：用户自助调试规则，见[用户手册](doc/user-guide.md#cli-参考)。
- **会话内临时放行**：弹窗批准并执行过的命令，本次会话内按触发原因免再问；deny 永不放行、跨会话不串、永不改配置文件；`CRUSH_TETHER_SESSION_ALLOW=off` 恢复每条必问。
- **suggest 权限建议**：统计最近反复批准且执行成功的命令，输出可粘贴进 `rules.toml` 的建议块 + 跳过清单（危险类别永不建议）。零写入，人工审阅后自行粘贴；crush 无 PostToolUse 事件，suggest 对 crush 不可用。

## agent 接入

在 agent 的 hook 配置中，把 PreToolUse 类钩子指向 `crush-tether hook --agent <slug>`：

| agent | slug | 项目目录来源 |
|---|---|---|
| Crush | `crush` | `CRUSH_PROJECT_DIR` |
| ClaudeCode | `claudecode` | `CLAUDE_PROJECT_DIR` |
| zcode | `zcode` | `${ZCODE_PROJECT_DIR}` → `${CLAUDE_PROJECT_DIR}` 回退 |

**zcode 走插件（推荐）**：Settings → Plugin Management → Discover 页 **+** 号 → 本地目录选 `plugin/` → 启用插件。hook 经插件内 wrapper 拉起（0.2.0）：PATH 找到二进制即转发，缺席时 exit 2 + 安装指引；配置文件轨另有工作区信任门，headless 不可首授。部署验收：任意会话跑一条命令，确认 `decisions.jsonl` 出现记录。

各 agent 的 hook 配置写法、实测行为差异（含 headless 收场按 agent 分化）见[用户手册](doc/user-guide.md#接入你的-agent)与 [agent-compat-matrix.md](doc/agent-compat-matrix.md)。

## 安全模型

- 脚本在二进制内沙箱执行（Rhai 限流 / Lua 库白名单 + 指令数 hook + 内存上限），永不以 OS 进程形态存在，无文件/进程/网络 API。
- `script_allow` 受控放行：脚本只能**激活**用户在 `rules.toml` 声明过的放行条目——加载期字面量提取、声明集对账拒载、运行时双保险、定稿点逃逸检查、deny 终审（五件套，见 design.md）。
- 组合裁决 deny 优先；unparseable → confirm；任何配置/脚本错误 → fail-safe confirm。
