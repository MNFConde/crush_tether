# crush-tether

Crush 命令级 bash 权限门（Rust 实现）。拦截 agent 即将执行的 bash 命令，按三档裁决：**allow**（放行）/ **confirm**（升级人工确认）/ **deny**（阻断）——宁可误拦、绝不误放（fail-safe confirm）。

- **零内置策略**：二进制是纯引擎，不内嵌任何规则；默认规则以**生成到项目侧**的外部 `rules.toml` + `rules.lua|rules.rhai` + `knowledge.toml` 提供（三层皆缺才生成，任一层有效即尊重）。
- **双脚本引擎**：Rhai（默认）或 Lua（`--engine lua`），同一 `RuleEngine` 接口、同等沙箱限流。
- **多 agent 适配**：Crush / ClaudeCode / zcode 契约适配层。

设计与决策论证见 [doc/design.md](doc/design.md) 与 [doc/decisions.md](doc/decisions.md)。

## 构建

```sh
cargo build --release   # 产物 target/release/crush-tether
cargo install --path .  # 开发测试推荐：装入 cargo bin（已在 PATH），升级加 --force
```

**开发测试推荐 `cargo install --path .`**（2026-09-08 定稿）：agent 插件/hook 以裸命令名解析二进制，二进制必须在 PATH 可达——进程起不来时 agent 侧 fail-open 放行（失效模式 #2，见 `doc/design.md`「Hook 接入失效模式」），故安装后建议 `where crush-tether` 验证一次。

工具链钉版见 `rust-toolchain.toml`；依赖版本约束只钉在根 `Cargo.toml` 一处。

## 配置

三层发现与效力顺序：**项目 > 用户 > 全局**（不粘性）。

| 层 | 位置 |
|---|---|
| 项目层 | `<项目根>/.crush-tether/` |
| 用户层 | `~/.config/crush-tether/` |
| 全局层 | Unix `/etc/crush-tether/`；Windows `%PROGRAMDATA%\crush-tether\`（`CRUSH_TETHER_GLOBAL_DIR` 可覆盖） |
| 显式覆盖 | `--config <path>` 或 `CRUSH_TETHER_CONFIG`（单文件顶替项目层） |

配置**不自动生成**。`crush-tether init` 显式生成默认包（缺省项目层；`--user`/`--global` 切目标层；`--engine lua` 生成 `rules.lua`）：

- `rules.toml` —— 声明层：`[local]`/`[global]` 双表 + 每命令 allow/confirm/deny 三桶查表（数组 = 覆盖，inline table `add`/`remove` = 增删；跨层字段级继承合并）。
- `rules.rhai` 或 `rules.lua` —— 脚本层：声明层表达不了的条件判断（两态子命令、`find` 突变参数、`curl|sh` 参数内管道、管道 sink、写特征升级）。脚本只上调、不放行——返回 allow 即契约违约。
- `knowledge.toml` —— 命令知识库：别名归一、写词元/写参数计数等数据源。

规则文件**损坏 ≠ 缺失**：解析失败 → 告警 + fail-safe confirm、原文件不动、不覆盖重生成。三层皆缺时引擎按裸兜底运行（未匹配一律 confirm）并提示先跑 init。lint 只告警不拒绝加载。格式细则见 design.md「配置格式与脚本边界（定稿）」。

**默认包裁决画像**（以生成的 `rules.toml` 为准，可自行挪档）：读类命令与项目内安全写（`git add`/`commit`、`touch`/`mkdir`、`cargo build`/`test`、`npm run` 等）放行；写重定向、写 flag、包管理器安装、`rm`/`curl`/`wget` 等确认；`sudo`/`mkfs`/`git push`/`reset --hard` 等阻断；**未匹配命令一律确认兜底**（`node -e`、`go run` 等任意代码执行刻意不入 allow 表）。改完即热重载生效，无需重启。

## 运行模式

```sh
crush-tether hook     --agent <crush|claudecode|zcode> [--engine rhai|lua] [--config p]
crush-tether serve    --project <dir> [--idle-exit 30]
crush-tether check    # 无参数时也走 check（冒烟/测试用）
crush-tether benchmark
crush-tether explain '<command>'   # 单发全溯源报告（M7.1 调试）
crush-tether repl                  # 规则调试器：改规则即测（M7.1）
crush-tether init [--user|--global]  # 显式生成默认配置包（缺省项目层）
crush-tether suggest [--threshold 3] [--window 30] [--format toml|table]
```

- **hook**（agent 接入的主路径）：connect-or-spawn——尝试连接项目 serve 端点；无实例则 detached 拉起 serve 并有界等就绪；仍失败则本进程跑全量管线，绝不无裁决放行。
- **serve**：常驻服务，端点名 `hash(项目根, engine, --config)` 每项目一实例；notify 热重载（失败保留旧快照）；连接归零 + idle 退出；裁决日志默认开（`CRUSH_TETHER_LOG=0|off|false` 关闭）。
- **check**：单发全量管线（in-process），兜底与冒烟。
- **规则测试工具**（M7.1）：`explain` 逐条溯源（命中层级/桶/token、归一链、写效果扫描、脚本改判）；`check --batch`（stdin 一行一命令 → 裁决表）；`check --cases <file>`（断言用例文件 `[[case]] cmd/expect` 批量对账，规则变更的回归护栏）；`repl` 调试器（空行重复上一条、`!N` 重跑、每条输入重载配置——改规则即测免重启）。
- **会话内临时放行**（M8.6）：弹窗批准并执行过的命令，**本次会话内**不再重复问（按触发原因匹配——批 `git push origin x` 后 `git push y` 也不问了）；拒绝档永不放行、跨会话不串、永不改配置文件；`CRUSH_TETHER_SESSION_ALLOW=off` 恢复每条必问。
- **suggest 权限建议**（M8.6，零写入）：统计最近反复批准且执行成功的命令（默认 ≥3 次/30 天），输出可粘贴进 `rules.toml` 的建议块 + 跳过清单（危险类别永不建议）。**人工审阅后自行粘贴**——git diff 即回滚面与审计面。

## agent 接入

在 agent 的 hook 配置中，把 PreToolUse 类钩子指向 `crush-tether hook --agent <slug>`：

| agent | slug | 项目目录来源 |
|---|---|---|
| Crush | `crush` | `CRUSH_PROJECT_DIR` |
| ClaudeCode | `claudecode` | `CLAUDE_PROJECT_DIR` |
| zcode | `zcode` | `${ZCODE_PROJECT_DIR}` → `${CLAUDE_PROJECT_DIR}` 回退 |

stdin 传 hook JSON（命令取 `tool_input.command`，兜底 `CRUSH_TOOL_INPUT_COMMAND` 环境变量）。三档行为等价：allow 放行 / confirm 要求确认 / deny 阻断（exit 2）。

### zcode 插件安装（实测可用，2026-09-06）

前置：`crush-tether` 在 PATH 上（hook 以 `type:"process"` 参数向量直接拉起，不经 shell）。

1. Settings → Plugin Management → Discover 页 **+** 号 → 本地目录 → 选择本仓库 `plugin/` 目录（内含 marketplace.json 与 crush-tether 插件）；
2. 启用插件——插件贡献的 hook 自动启用 zcode 的 hook runner（配置文件 hooks 默认禁用的坑由此绕开）；
3. 部署验收：任意会话跑一条 bash 命令，确认 `.crush-tether/decisions.jsonl` 出现 `mode:"serve"` 记录（hook 确实触发）。**agent 侧对 hook 失败采取 fail-open（放行）**，「二进制在 PATH 可达」是部署必查项。

合同细节（stdin 双命名兼容、PermissionRequest 实测结论、挂点定型依据）见 `doc/design.md`「Agent 适配层」。

## 安全模型

- 脚本在二进制内沙箱执行（Rhai 限流 / Lua 库白名单 + 指令数 hook + 内存上限），永不以 OS 进程形态存在，无文件/进程/网络 API。
- `script_allow` 受控放行：脚本只能**激活**用户在 `rules.toml` 声明过的放行条目——加载期字面量提取、声明集对账拒载、运行时双保险、定稿点逃逸检查、deny 终审（五件套，见 design.md）。
- 组合裁决 deny 优先；unparseable → confirm；任何配置/脚本错误 → fail-safe confirm。
