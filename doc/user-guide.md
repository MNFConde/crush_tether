# crush-tether 用户手册

> 面向使用者的操作手册：怎么安装、怎么配置、怎么接入 agent、怎么排查。
> 机制设计与决策论证见 [design.md](design.md) 与 [decisions.md](decisions.md)——schema 细则与安全机制以 design.md 为单一事实源，本文只给最小可用示例并回指。

## 简介

crush-tether 是**命令级 bash 权限门**：以 PreToolUse hook 拦截 agent（Crush / Claude Code / zcode）即将执行的每条 bash 命令，做三档裁决：

| 档位 | 含义 | 用户看到 |
|---|---|---|
| **allow** | 只读 / 项目内安全写 | 直接放行，无感 |
| **confirm** | 有风险、可逆 | agent 原生权限弹窗 |
| **deny** | 不可逆 / 破坏性 | 硬阻断，不弹窗 |

判定特点：

- **命令级而非前缀级**：复合命令按 AST 拆成简单命令逐条裁决——`echo hi && rm -rf /` 里的 `rm` 逃不掉；
- **fail-safe**：任何配置/脚本/解析错误都保守落 confirm，绝不误放行；
- **零内置策略**：二进制是纯引擎，规则全部来自外部配置文件（`crush-tether init` 显式生成默认包）。

## 安装

需要 Rust 工具链（版本钉在 `rust-toolchain.toml`）：

```sh
cargo build --release   # 产物 target/release/crush-tether
cargo install --path .  # 推荐：装入 cargo bin（PATH 可达），升级加 --force
```

装完验证一次（crush/claude 的 hook 以裸命令名解析二进制）：

```sh
where crush-tether     # Windows；Unix 用 which
```

> **为什么「二进制可达」是必查项**：hook 进程起不来时，agent 侧对 hook 失败采取 fail-open（放行）——门静默消失。zcode 插件轨经 wrapper 启动，二进制缺席时 wrapper 会 exit 2 响亮报错并给安装指引（不静默放行），见[接入](#接入你的-agent)。

## 快速开始

三步：

```sh
# 1. 在目标项目根生成默认配置包
crush-tether init

# 2. 在 agent 的 hook 配置里把 PreToolUse 指向引擎（见「接入你的 agent」）
#    crush-tether hook --agent <crush|claudecode|zcode>

# 3. 跑一条命令验证：.crush-tether/decisions.jsonl 出现裁决记录即链路通
```

默认包裁决画像（以生成的 `rules.toml` 为准，可自行调整）：

- **放行**：读类命令（`ls`/`cat`/`grep`/`find`…）与项目内安全写（`git add`/`commit`、`touch`/`mkdir`、`cargo build`/`test`、`npm run` 等）；
- **确认**：写重定向、写 flag、包管理器安装（`npm install`/`pip`）、`rm`/`curl`/`wget`；
- **阻断**：`sudo`/`mkfs`/`dd`/`shutdown`、`git push`/`reset --hard` 等；
- **未匹配命令一律确认兜底**——`node -e`、`go run` 等任意代码执行刻意不入 allow 表。

改规则文件即热重载生效，无需重启。

## 配置

### 三层发现

解析优先级：**项目 > 用户 > 全局**（不粘性；显式 `--config` 或 `CRUSH_TETHER_CONFIG` 顶替一切）：

| 层 | 位置 |
|---|---|
| 项目 | `<项目根>/.crush-tether/` |
| 用户 | `~/.config/crush-tether/` |
| 全局 | Unix `/etc/crush-tether/`；Windows `%PROGRAMDATA%\crush-tether\`（`CRUSH_TETHER_GLOBAL_DIR` 可覆盖） |

三层皆缺 → 引擎按裸兜底运行（未匹配一律 confirm）+ stderr 提示 init；**配置不自动生成**，`init` 是唯一创建路径。文件**损坏 ≠ 缺失**：解析失败 → stderr 告警 + confirm 兜底，原文件不动。发现机制细节见 [design.md](design.md#配置分层与优先级定稿)。

### init：生成默认配置包

```sh
crush-tether init                # 缺省生成项目层 .crush-tether/
crush-tether init --user         # 用户层
crush-tether init --global       # 全局层
crush-tether init --engine lua   # 生成 rules.lua 而非 rules.rhai
```

生成三个文件（已存在的跳过，幂等）：

- `rules.toml` —— 声明层：无条件查表（三桶 + 命令节）；
- `rules.rhai` / `rules.lua` —— 脚本层：条件判断（两态子命令、管道 sink 等）；
- `knowledge.toml` —— 命令知识库：别名/flag 等价、写特征等事实数据。

### rules.toml 速览

组织方式：**命令 → 三桶（allow/confirm/deny）**，桶内可细到子命令（`sub`）与 flag（`flag`）：

```toml
version    = 1
default    = "confirm"                      # 未命中任何配置的命令
precedence = ["deny", "confirm", "allow"]   # 桶间优先级（可调）

[local]                  # 写入不出项目：此表内 allow 带写目标感知逃逸检查
allow   = ["ls", "cat", "grep", "touch", "mkdir"]
confirm = ["rm", "pip", "npx", "curl", "wget"]
deny    = ["sudo", "dd", "shutdown", "mkfs"]

[local.git]
allow.sub   = ["status", "log", "diff", "add", "commit"]
confirm.sub = ["rm", "restore", "reset"]
deny.sub    = ["push", "pull", "clean", "rebase"]
deny.flag   = ["--hard"]

[local.npm]
confirm.sub = ["install", "ci", "publish"]
default     = "allow"    # 节内 default：npm 其余子命令（run 等）放行

[global]                 # 允许影响项目外：此表内 allow 豁免逃逸检查（如团队统一放行 docker）
allow = []
```

要点（完整语义与默认包全文见 [design.md「配置格式与脚本边界」](design.md#配置格式与脚本边界v1-定稿)）：

- `[local]`/`[global]` 是**作用域**不是路径过滤：`[local]` 的 allow 放行时检查**写目标**不出项目（读路径豁免），`[global]` 的 allow 豁免该检查（更强的承诺）；
- deny/confirm 写在 `[local]` 即可（安全侧与作用域无关），`[global]` 实际只承载 allow；
- 同命令的**节**（如 `[local.git]`）遮蔽裸列表词条；同表内按 `precedence` 顺序查桶，多命中有序合成；节内 `default` 覆盖顶层 `default`。

### knowledge.toml 速览

记录命令世界的**事实**（读写属性、别名、等价），与策略分离、不产生任何裁决：

```toml
[npx]
may_write = true                 # 属性：下载并执行任意包（lint 建议用）

[npm]
sub.exec = { alias_of = "npx" }  # 联系：npm exec ≡ npx，查表前归一

[curl]
may_write   = true
write_flags = ["-o", "--output"] # 带这些 flag 才会写文件

[cp]
write_position = "last"          # 最后一个位置参数是写目标（读源豁免）

[git]
sub.branch     = { write_tokens = ["-d", "--delete", "-m"] }  # 两态判定的脚本数据源
flag."--force" = { same_flag = "-f" }                        # flag 等价闭包
```

删光知识库 → 判定完全不受影响（别名/flag 等价失效、脚本查不到数据时 confirm 兜底）；decisions.jsonl 的 `kb` 字段自证当前 bucket 状态。槽位全集与消费机制见 [design.md「命令知识库」](design.md#命令知识库bucket-框架定稿)。

### 脚本层

声明层表达不了的条件判断住在这里（`rules.rhai` 或 `rules.lua`）。默认包含四条具名规则：`pipe_sink`（管道 sink → deny）、`find_mutator`（`find` 突变参数 → confirm）、`two_state`（两态子命令按知识库数据判定）、`write_redirect`（查表放行 + 写重定向 → 降 confirm）：

```rhai
rule("pipe_sink", 10, |ctx| {
    if ctx.pipe_to_shell { return decision::DENY; }
    decision::PASS
});
```

词汇约定（[design.md「声明式规则函数」](design.md#声明式规则函数rule-注册器定稿)）：

- **决策值**：`decision::PASS`（不表态，交下一个规则/查表基线）/ `CONFIRM` / `DENY`；返回 `ALLOW` 即契约违约——**脚本只上调、不放行**；
- **`rule(名字, 优先级, 函数)`**：注册器形态，优先级数值小先执行、表态即短路；具名规则在 decisions.jsonl 的 `script.rule` 与 explain 报告中可追溯；`confirm_as("子名")` 可上报更细的分支子名（如批准 `-d` 不放行 `-D`）；
- **旧形态 `fn check(ctx)`** 仍兼容：文件内无 `rule()` 注册时生效（双形态长期并存）；
- **ctx 常用字段**：`ctx.bin` / `ctx.sub`（无子命令为 `""`）/ `ctx.args` / `ctx.verdict` / `ctx.writes_redirect` / `ctx.pipe_to_shell`；
- **可用原语**：`kb_present()` / `kb_write_tokens()` / `kb_write_arg_count()` 等知识库数据源 + `allow("bin")` 受控激活（见下）。

脚本需要条件放行时走 `script_allow` 受控开口：先在 `rules.toml` 声明（`script_allow = ["ls", "docker"]`），脚本内 `allow("bin")` 只能激活声明过的条目；逃逸检查与 deny 终审在引擎定稿点强制执行，脚本绕不开也代不了劳。机制五件套见 [design.md「脚本条件放行」](design.md#脚本条件放行script_allow定稿)。

### 层间合并

字段级继承（低层 = 父类，高层 = 子类）：子层未定义的键整体继承，定义了即覆盖：

```toml
# 项目层示例
[local]
allow   = { add = ["jq"], remove = ["curl"] }   # inline table = 继承低层并增删
confirm = ["rm", "pip"]                          # 数组 = 覆盖：本份就是全部

[local.git]
deny.sub = ["push", "filter-branch"]             # 覆盖用户层的 deny.sub
```

标量（`default`/`precedence`）写值即覆盖；脚本层同文件按 全局 → 用户 → 项目 顺序执行，项目层可作最终裁决。语义细节见 [design.md「层间合并」](design.md#层间合并字段级继承定稿)。

### 热重载

serve 常驻时，规则文件改动经文件监听（600ms debounce）整段重编译后整体替换——**改完即生效，无需重启**；编译失败保留旧快照 + stderr 告警。重载信号在请求间隙消费：改规则后的第一个请求触发重载（最坏滞后一个请求），decisions.jsonl 留 `type:"load"` 事件行可对齐时间线。

## 接入你的 agent

| agent | slug | hook 命令 | 项目目录来源 |
|---|---|---|---|
| Crush | `crush` | `crush-tether hook --agent crush` | `CRUSH_PROJECT_DIR` |
| Claude Code | `claudecode` | `crush-tether hook --agent claudecode` | `CLAUDE_PROJECT_DIR` |
| zcode | `zcode` | 由插件 wrapper 拉起（见下） | `${ZCODE_PROJECT_DIR}` → `${CLAUDE_PROJECT_DIR}` 回退 |

三档行为等价：allow 放行 / confirm 走 agent 原生确认 / deny 阻断（exit 2）。契约细节与实测差异见 [design.md「Agent 适配层」](design.md#agent-适配层定稿)与 [agent-compat-matrix.md](agent-compat-matrix.md)。

### Crush

在 Crush 的 hook 配置（项目/全局 `crush.json`，或内置 `crush hook add PreToolUse --command "crush-tether hook --agent crush" --matcher Bash`）把 PreToolUse 指向引擎。契约细节（JSON envelope、`updated_input`、聚合语义）见 [design.md「Crush 契约」](design.md#crush-契约实测定稿2026-09-09)。

### Claude Code

项目 `.claude/settings.json`（或 `--settings` 指定文件）：

```json
{
  "hooks": {
    "PreToolUse": [
      {
        "matcher": "Bash|PowerShell",
        "hooks": [{ "type": "command", "command": "crush-tether hook --agent claudecode" }]
      }
    ]
  }
}
```

matcher 记得包含 `PowerShell`——Windows 下 bash 类工具可能叫这个名，漏配会静默不触发。契约与实测行为见 [design.md「ClaudeCode 契约」](design.md#claudecode-契约实测定稿2026-09-09)。

### zcode（插件，推荐）

前置：`crush-tether` 已安装并在 PATH。

1. zcode Settings → Plugin Management → Discover 页 **+** 号 → 本地目录 → 选择本仓库 `plugin/` 目录（内含 marketplace.json 与 crush-tether 插件）；
2. 启用插件——插件贡献的 hook 自动启用 zcode 的 hook runner（配置文件 hooks 默认禁用的坑由此绕开）；
3. hook 经插件内 wrapper 拉起（`${ZCODE_PLUGIN_ROOT}/bin/crush-tether-wrapper.cmd`）：PATH 找到二进制即转发裁决，缺席时 exit 2 + 安装指引——失效模式 #2（二进制不可达静默放行）由此结构性堵上。

> 配置文件轨（工作区 `.zcode/config.json` 写 hooks）另有工作区信任门（须 Desktop App UI 批准），headless 不可首授——交付与日常使用走插件轨。

### 验证 hook 真的触发

部署后必查——「没弹窗」也可能意味着「门没在」（fail-open）：

```sh
# 任意会话跑一条 bash 命令后：
tail .crush-tether/decisions.jsonl   # 出现新裁决记录 = 链路通
```

更系统的探针方法见 [test-and-ci.md §6](test-and-ci.md#6-探针与工具设计)。

### headless（无头）注意

无头形态（`claude -p` / `crush run` / zcode `-p`）下无法弹窗，confirm/ask 的收场按 agent 分化（2026-09-12 实测定性）：**Claude Code / zcode = 拒绝执行**；**crush = 原生权限自动接受、命令照跑**。会话临时放行（见下）在无头流量下不生长——便签需要真实弹窗批准。

## CLI 参考

| 命令 | 用途 |
|---|---|
| `hook --agent <slug> [--engine] [--config]` | agent 接入主路径：connect-or-spawn（连 serve → 拉起 serve → 降级本进程全量管线，绝不无裁决放行） |
| `serve --project <dir> [--idle-exit 30]` | 常驻服务（hook 自动拉起，也可手动）：命名端点、热重载、空闲自动退出 |
| `check [--agent] [--engine] [--config]` | 单发全量管线；无子命令参数时的默认模式（兜底/冒烟/嵌入） |
| `check --batch` | stdin 一行一命令 → 裁决表（配置加载一次，exit 恒 0） |
| `check --cases <file>` | 断言用例批量对账（TOML `[[case]] cmd/expect`），全过 exit 0、任一失败 exit 1——规则变更的回归护栏 |
| `benchmark` | 双跑对比（in-process vs serve 路径），裁决 diff 为空即路径等价 |
| `init [--user\|--global] [--engine lua]` | 显式生成默认配置包 |
| `explain '<command>'` | 单发全溯源报告：命中层级/桶/token、归一链、写效果扫描集、脚本改判——调规则先看它 |
| `repl` | 规则调试器：每条输入重载配置（改规则即测免重启）、空行重跑上一条、`!N` 回放历史、`:lint` 看告警 |
| `suggest [--threshold 3] [--window 30] [--format toml\|table]` | 权限建议，见[下节](#会话临时放行与-suggest) |

规则测试四件套的设计与基建复用见 [design.md「规则测试工具」](design.md#规则测试工具m71定稿)。

## 环境变量参考

| 变量 | 作用 |
|---|---|
| `CRUSH_TETHER_CONFIG` | 显式配置文件路径（优先级高于三层发现） |
| `CRUSH_TETHER_GLOBAL_DIR` | 覆盖全局层目录（测试/便携） |
| `CRUSH_TETHER_LOG` | `0`\|`off`\|`false` 关裁决日志（默认开） |
| `CRUSH_TETHER_LEARN` | `off` 关执行记录采集（默认开；suggest 与统计的信号源） |
| `CRUSH_TETHER_SESSION_ALLOW` | `off` 关会话内临时放行（默认开） |
| `CRUSH_TETHER_IDLE_EXIT` | 覆盖 serve 空闲退出秒数（默认 30） |
| `CRUSH_TETHER_DISABLE_SERVE` | `1` = hook 跳过 connect-or-spawn，强制单进程降级路径 |
| `CRUSH_TOOL_INPUT_COMMAND` | agent 载荷缺失时的命令兜底来源 |

三个 `CRUSH_TETHER_*` 功能开关互相独立。agent 注入的 `CRUSH_PROJECT_DIR` / `CLAUDE_PROJECT_DIR` / `ZCODE_PROJECT_DIR` 用于项目根定位与路径逃逸检查基准。

## 日志与观测

全部落在项目配置目录 `.crush-tether/` 下：

| 文件 | 内容 | 开关 |
|---|---|---|
| `decisions.jsonl` | 审计面：一行一裁决（含热重载 `type:"load"` 事件行） | 默认开，`CRUSH_TETHER_LOG=off` 关 |
| `executions.jsonl` | 学习信号：一行一执行（ts/agent/session_id/tool_use_id/命令/成败） | 默认开，`CRUSH_TETHER_LEARN=off` 关 |
| `session-cache.jsonl` | 降级态（serve 不可达时）会话放行便签 | 随会话放行开关 |

`decisions.jsonl` 要点字段：`command`（原文）/ `decision` / `reason` / `source`（命中层级/文件/条目）/ `kb`（知识库 bucket 状态）/ `normalized`（归一链）/ `script.rule`（具名规则溯源）/ `session_id` + `tool_use_id`（会话放行关联主键）。会话放行改判的请求**照记最终裁决**，`reason` 标注 `session allow`——审计可见，非黑箱。字段全集见 [design.md「日志」](design.md#日志格式先行开关位置-p4-定)。

`explain` 与 `repl` 不落裁决日志（调试防噪音）。

## 会话临时放行与 suggest

两个特性共用一套采集面，分工：**便签信当下（当场亲手批的），suggest 守长期（跨会话统计）**。

### 会话内临时放行（默认开）

弹窗批准并真实执行过的 confirm 命令，**本次会话内**同触发原因不再重复问：

- **粒度 = 触发原因**：批准 `npm install` 后，同一条规则触发的后续询问免问（参数变化天然覆盖）；无稳定溯源键的（default 兜底命中的命令等）退回完整命令粒度；
- **红线**：deny 永不放行；跨会话不串；会话结束即失效（另有 24h TTL 兜底与容量上限）；**永不写配置文件**；
- **关闭**：`CRUSH_TETHER_SESSION_ALLOW=off` 恢复每条必问。

机制细节见 [design.md「会话内临时放行」](design.md#会话内临时放行session-allow定稿)。

### suggest（零写入）

```sh
crush-tether suggest                  # 默认 ≥3 次 / 30 天
crush-tether suggest --threshold 5 --window 14 --format table
```

统计最近反复**批准且执行成功**的 confirm 命令（deny 永不学习），输出三部分：

1. 可直接粘贴进 `rules.toml` 的建议块；
2. 摘要表；
3. 跳过清单——四类永久不建议（即便反复批准）：知识库 `may_write`、不可逆、网络类命令、自由参数无法安全收窄的形态。

**人工审阅后自行粘贴**——git diff 就是回滚面与审计面，无任何自动写入。

> agent 差异（实测定性）：Claude Code 全量可用；zcode 可用（PostToolUse 仅执行结果，降级）；crush 无 PostToolUse 事件，**suggest 不可用**。无 executions 文件时输出「无执行记录」说明后正常退出。

## 安全模型

- **脚本沙箱**：`rules.rhai`/`rules.lua` 在二进制内沙箱执行（Rhai 指令限流 / Lua 库白名单 + 全局指令数 hook + 内存上限），永不以 OS 进程形态存在，无文件/进程/网络 API；
- **受控放行**：脚本无自由放行权——`allow("bin")` 只能激活 `rules.toml` 里声明过的条目，加载期字面量提取、声明集对账拒载、运行时双保险、定稿点逃逸检查、deny 终审五件套机械强制；
- **deny 终审**：查表落 deny 的命令，任何机制都翻不动；
- **fail-safe**：unparseable → confirm；配置/脚本任何错误 → confirm 兜底；hook 进程起不来的唯一失效洞由「二进制可达」部署检查与 zcode wrapper 守卫对冲。

设计论证见 [design.md「脚本条件放行」](design.md#脚本条件放行script_allow定稿)、[design.md「DSL 引擎」](design.md#dsl-引擎定稿)与 [decisions.md](decisions.md)。

## 故障排查 FAQ

**Q：装好了，但一条命令都没拦/没弹窗？**
hook 大概率没被拉起（agent 侧 fail-open 放行）。按序查：① `where crush-tether` 二进制可达吗；② agent 配置里 hook 注册了吗（zcode 走插件轨）；③ 跑一条命令后 `.crush-tether/decisions.jsonl` 有新行吗——没有 = hook 没触发。

**Q：所有命令都要求确认？**
三层皆缺，引擎按裸兜底运行（未匹配一律 confirm）。到项目根跑 `crush-tether init`；stderr 里也有同样提示。

**Q：某条命令被误拦，想放行？**
`crush-tether explain '<command>'` 看命中溯源（哪层/哪个桶/哪个 token）→ 改 `rules.toml`（挪桶，或 `{ add = [...], remove = [...] }` 增删）→ 热重载即生效 → 把该命令写进 `check --cases` 用例文件作回归护栏。

**Q：改了规则没生效？**
serve 热重载有 600ms debounce 且信号在请求间隙消费——最坏滞后一个请求；看 decisions.jsonl 的 `type:"load"` 事件行确认重载发生。脚本编译失败会保留旧快照并 stderr 告警。

**Q：会话放行没生效？**
① 无头会话不生长便签（无 confirm 弹窗）；② 检查 `CRUSH_TETHER_SESSION_ALLOW` 未设为 off；③ 便签跨会话天然失效——本设计如此，不是故障。

**Q：lint 告警要看吗？**
lint 只告警不拒绝加载。典型告警：同 token 双桶、等价冗余（allow `pip` 又 allow `pip3`）、allow 了 `may_write` 命令、`script_allow` 死声明。按提示清理即可。

## 文档地图

| 文档 | 定位 |
|---|---|
| 本文（user-guide.md） | 使用者操作手册（how-to） |
| [design.md](design.md) | 设计单一事实源：机制、契约、schema 细则 |
| [decisions.md](decisions.md) | 决策记录：为什么这么选 + 被否决方案 |
| [agent-compat-matrix.md](agent-compat-matrix.md) | 实测兼容性矩阵（各 agent 行为差异与版本结论） |
| [test-and-ci.md](test-and-ci.md) | 测试方法、CI 覆盖与挂账 |
| [../cairn/agent-hook-testing.md](../cairn/agent-hook-testing.md) | hook 测试方法论与踩坑史（知识库） |
