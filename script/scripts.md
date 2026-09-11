# script/ 脚本索引

本目录存放需要持久化、本地运行的脚本，由 uv 管理共享环境（`pyproject.toml`，单环境，纯标准库零依赖）。目录组织与三次法则见 [AGENTS.md](AGENTS.md)。

| 脚本 | 作用 | 用法 |
|---|---|---|
| [check-links.py](check-links.py) | 校验 doc/ 与 cairn/ 各 Markdown 的跨文件与站内锚点引用一致性（平移自 mdor） | `uv run --directory script check-links.py` |
| [check-commit-msg.py](check-commit-msg.py) | 校验 git 提交信息格式（Conventional Commits），由 `.githooks/commit-msg` 调用（平移自 mdor） | `uv run --directory script check-commit-msg.py <提交信息文件>` |
| [hook_probe.py](hook_probe.py) | hook 探针（design.md「hook 探针方法（定稿）」的 python 参考实现，M7.2）：dump 载荷 / 转发真实引擎 / 控制文件切模式，注册进任意项目实测 hook 触发 | `uv run python hook_probe.py <bash\|perm\|post\|fail>`（可带 `--probe-dir`/`--engine` 等，见下） |
| [mock_llm.py](mock_llm.py) | 双协议 LLM mock（agent-matrix CI 后端，M7.3）：驱动 agent 零凭证走完对话与 hook 全链 | `uv run --directory script mock_llm.py [--port 8787] [--cmd 命令] [--log 请求日志.jsonl]` |
| [ci_scenario.sh](ci_scenario.sh) | agent-matrix CI 场景组（deny / fail-open / updated_input）setup/assert 两段式（M7.3） | `bash ci_scenario.sh {setup\|assert} <deny\|failopen\|rewrite> <probe-dir> <agent-slug> [workspace]` |

## check-links.py

- **作用**：扫描 doc/ 与 cairn/ 顶层全部 `*.md`（不递归，自然排除 `archive_doc_v*` 与 `cairn/history/`），对 `](…md#anchor)` 跨文件链接与 `](#anchor)` 站内链接，逐一与目标文件标题（`#` 1–6 级）生成的 GitHub slug 比对；跳过 fenced code block 与行内反引号代码
- **用法**：`uv run --directory script check-links.py [--doc-root <doc目录>] [--cairn-root <cairn目录>]`（默认分别为仓库根下 `doc/`、`cairn/`）
- **退出码**：0 = 全部锚点一致；1 = 存在不匹配（逐条列出 MISMATCH 行）

## check-commit-msg.py

- **作用**：校验提交信息首行 `类型(范围): 主题` 结构与 11 种类型白名单、主题规则、主题行与正文行显示宽度 ≤72 列（全角字符按 2 列计）、正文有效行 ≤20 行（脚注段不计入）、`BREAKING CHANGE:` / `Closes #` footer 格式；豁免 `Merge` / `Revert` / `fixup!` / `squash!` 开头与 `#` 注释行。规则全文见 `.agents/rules/commit.md`
- **用法**：`uv run --directory script check-commit-msg.py <提交信息文件>`（git 提交时由 commit-msg 钩子自动调用）
- **退出码**：0 = 格式通过；1 = 存在违规（逐条列出）；2 = 参数错误
- **启用**（一次性，本地配置不入库）：`git config core.hooksPath .githooks`

## hook_probe.py

- **作用**：hook 探针参考实现——注册进 agent 工作区 hook（`type:"process"`，`command: "uv"`），实测「hook 真的被拉起、载荷与裁决真的流转」。方法论（语言无关）见 doc/design.md「hook 探针方法（定稿）」。四角色与已退役的 node 探针（`.zcode/probe/probe.js`）语义 1:1：
  - `bash`：dump stdin 载荷 → 转发真实引擎（`<engine> hook --agent <slug>`）→ 原样回传引擎 stdout 与退出码
  - `perm`：dump 后，探针目录有 `perm-out.txt` 则原样写到 stdout（模拟许可信封），退出码取 `perm-exit.txt`（默认 0）
  - `post`：仅 dump（PostToolUse 只有执行结果、无用户选择回传）
  - `fail`：按 `fail-exit.txt` 内容作退出码（默认 0），dump 截断 stdin——模拟 hook 进程异常退出
- **用法**：`uv run python hook_probe.py <bash|perm|post|fail> [--probe-dir DIR] [--engine EXE] [--agent SLUG] [--source TAG]`
- **探针目录**（dump 的 `dump.jsonl` 与三个控制文件所在地）：`--probe-dir` > 环境变量 `HOOK_PROBE_DIR` > `<cwd>/.zcode/hook-probe`；换实验改控制文件，不改注册、不重启会话
- **引擎解析**：`--engine` > 环境变量 `CRUSH_TETHER_EXE` > `crush-tether`（裸命令名 PATH 解析，与正式插件分发路线一致）
- **退出码**：bash/perm/fail 按语义回传引擎或控制文件退出码；post 恒 0；探针自身故障不外泄（dump 写失败静默，引擎转发失败 stderr 告警 + exit 1）
- **注**：`uv run python` 在无 pyproject 的目录走默认解释器临时环境，故外部项目（含 python 项目外的一般项目）可直接注册本脚本，无需自带脚本环境

## mock_llm.py

- **作用**：`.github/workflows/agent-matrix.yml` 的 mock LLM 后端——Anthropic `/v1/messages` + OpenAI `/v1/chat/completions` 双协议、stream（SSE）与非流式都支持；固定一轮 tool_use（`echo mock-hook-test`），见到工具结果回 `spike done`。hook 链路是 agent 本地行为、与 LLM 无关，故 mock 驱动即可零凭证测「hook 被拉起、裁决流转」
- **用法**：`uv run --directory script mock_llm.py [--port 8787] [--log 请求日志.jsonl]`（`--log` 记录每轮 tool_use 的模型名与 agent 实际提供的工具名，诊断用；CI 用 `python3` 直跑）
- **三坑实录（改此脚本前必读）**：① 工具参数必须含 agent schema 全部必填字段——缺 `description` 会被 crush/fantasy 静默拒绝（error tool result、无告警，2026-09-09 实证）；② 工具名从请求 `tools` 列表自适应选（Windows 下 claude `-p` 可能提供 `PowerShell` 无 `Bash`，盲发 `Bash` 报 No such tool available）；③ OpenAI 协议 `stream=true` 必须回 SSE 分块，回 JSON 是 unexpected EOF
- **退出**：Ctrl+C / 杀进程；无守护逻辑（CI 后端，非长驻服务）

## 临时探针台账（三次法则登记处）

一次性诊断探针在此登记（规则见 [AGENTS.md](AGENTS.md)「临时脚本三次法则」）；同一探针跨会话累计 3 次必须固化为正式脚本。写探针前先查本表。

| 日期 | 探针 | 用途 | 次数 | 状态 |
|---|---|---|---|---|
| 2026-09-05 | 会话内 `uv run python` + tomllib 校验 design.md 中 TOML 片段合法性 | 草案 v1 文档 TOML 片段验证 | 1 | 待固化 |
| 2026-09-06 | 同上（rules.toml 默认包 + knowledge.toml 案例片段校验） | 知识库/合并模型修订验证 | 2 | 待固化——再出现 1 次即固化为 check-toml.py |

## ci_scenario.sh

- **作用**：agent-matrix CI 场景组的两段式脚本（bash；设计见 doc/test-and-ci.md「场景组」节）——探针 perm 角色直回控制文件信封（引擎不在场），断言物理副作用 `<workspace>/ci-exec.txt`（场景 mock `--cmd` 发带写命令）。`setup` 清探针目录与工作区痕迹后按场景写控制文件（信封形态按 agent 分支：claudecode/zcode = Claude 式 `hookSpecificOutput`，crush = 顶层 `decision`）；`assert` 校验 dump 有 perm 裁决 + 场景预期——failopen 按 agent 分化（claude 无头 = hook 失联兜底拒绝，crush/zcode = 放行，2026-09-11 定性）
- **用法**：`bash ci_scenario.sh {setup|assert} <deny|failopen|rewrite> <probe-dir> <agent-slug> [workspace]`（headless 调用与探针注册留在 workflow/调用方，见 agent-matrix.yml 各 job 的「Scenario suite」step）
- **退出码**：0 = 断言通过；1 = 失败（逐条 `ASSERT FAIL [场景]: 原因`）；2 = 参数错误
