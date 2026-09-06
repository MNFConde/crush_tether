# crush-tether

Crush 命令级 bash 权限门（Rust 实现）。拦截 agent 即将执行的 bash 命令，按三档裁决：**allow**（放行）/ **confirm**（升级人工确认）/ **deny**（阻断）——宁可误拦、绝不误放（fail-safe confirm）。

- **零内置策略**：二进制是纯引擎，不内嵌任何规则；默认规则以**生成到项目侧**的外部 `rules.toml` + `rules.lua|rules.rhai` + `knowledge.toml` 提供（三层皆缺才生成，任一层有效即尊重）。
- **双脚本引擎**：Rhai（默认）或 Lua（`--engine lua`），同一 `RuleEngine` 接口、同等沙箱限流。
- **多 agent 适配**：Crush / ClaudeCode / zcode 契约适配层。

设计与决策论证见 [doc/design.md](doc/design.md) 与 [doc/decisions.md](doc/decisions.md)。

## 构建

```sh
cargo build --release   # 产物 target/release/crush-tether
cargo install --path .  # 或装入 PATH
```

工具链钉版见 `rust-toolchain.toml`；依赖版本约束只钉在根 `Cargo.toml` 一处。

## 配置

三层发现与效力顺序：**项目 > 用户 > 全局**（不粘性）。

| 层 | 位置 |
|---|---|
| 项目层 | `<项目根>/.crush-tether/` |
| 用户层 | `~/.config/crush-tether/` |
| 显式覆盖 | `--config <path>` 或 `CRUSH_TETHER_CONFIG`（单文件顶替项目层） |

首次运行且三层皆缺有效配置时，在项目 `.crush-tether/` 生成默认包：

- `rules.toml` —— 声明层：`[local]`/`[global]` 双表 + 每命令 allow/confirm/deny 三桶查表（数组 = 覆盖，inline table `add`/`remove` = 增删；跨层字段级继承合并）。
- `rules.rhai` 或 `rules.lua` —— 脚本层：声明层表达不了的条件判断（两态子命令、`find` 突变参数、`curl|sh` 参数内管道、管道 sink、写特征升级）。脚本只上调、不放行——返回 allow 即契约违约。
- `knowledge.toml` —— 命令知识库：别名归一、写词元/写参数计数等数据源。

规则文件**损坏 ≠ 缺失**：解析失败 → 告警 + fail-safe confirm、原文件不动、不覆盖重生成。lint 只告警不拒绝加载。格式细则见 design.md「配置格式与脚本边界（定稿）」。

## 运行模式

```sh
crush-tether hook     --agent <crush|claudecode|zcode> [--engine rhai|lua] [--config p]
crush-tether serve    --project <dir> [--idle-exit 30]
crush-tether check    # 无参数时也走 check（冒烟/测试用）
crush-tether benchmark
```

- **hook**（agent 接入的主路径）：connect-or-spawn——尝试连接项目 serve 端点；无实例则 detached 拉起 serve 并有界等就绪；仍失败则本进程跑全量管线，绝不无裁决放行。
- **serve**：常驻服务，端点名 `hash(项目根, engine, --config)` 每项目一实例；notify 热重载（失败保留旧快照）；连接归零 + idle 退出；裁决日志默认开（`CRUSH_TETHER_LOG=0|off|false` 关闭）。
- **check**：单发全量管线（in-process），兜底与冒烟。

## agent 接入

在 agent 的 hook 配置中，把 PreToolUse 类钩子指向 `crush-tether hook --agent <slug>`：

| agent | slug | 项目目录来源 |
|---|---|---|
| Crush | `crush` | `CRUSH_PROJECT_DIR` |
| ClaudeCode | `claudecode` | `CLAUDE_PROJECT_DIR` |
| zcode | `zcode` | `${ZCODE_PROJECT_DIR}` → `${CLAUDE_PROJECT_DIR}` 回退 |

stdin 传 hook JSON（命令取 `tool_input.command`，兜底 `CRUSH_TOOL_INPUT_COMMAND` 环境变量）。三档行为等价：allow 放行 / confirm 要求确认 / deny 阻断（exit 2）。zcode 插件分发形态已实现，实机 hook 触发验证挂部署时探针。

## 安全模型

- 脚本在二进制内沙箱执行（Rhai 限流 / Lua 库白名单 + 指令数 hook + 内存上限），永不以 OS 进程形态存在，无文件/进程/网络 API。
- `script_allow` 受控放行：脚本只能**激活**用户在 `rules.toml` 声明过的放行条目——加载期字面量提取、声明集对账拒载、运行时双保险、定稿点逃逸检查、deny 终审（五件套，见 design.md）。
- 组合裁决 deny 优先；unparseable → confirm；任何配置/脚本错误 → fail-safe confirm。
