# script/ 脚本索引

本目录存放需要持久化、本地运行的脚本，由 uv 管理共享环境（`pyproject.toml`，单环境，纯标准库零依赖）。目录组织与三次法则见 [AGENTS.md](AGENTS.md)。

| 脚本 | 作用 | 用法 |
|---|---|---|
| [check-links.py](check-links.py) | 校验 doc/ 与 cairn/ 各 Markdown 的跨文件与站内锚点引用一致性（平移自 mdor） | `uv run --directory script check-links.py` |
| [check-commit-msg.py](check-commit-msg.py) | 校验 git 提交信息格式（Conventional Commits），由 `.githooks/commit-msg` 调用（平移自 mdor） | `uv run --directory script check-commit-msg.py <提交信息文件>` |

## check-links.py

- **作用**：扫描 doc/ 与 cairn/ 顶层全部 `*.md`（不递归，自然排除 `archive_doc_v*` 与 `cairn/history/`），对 `](…md#anchor)` 跨文件链接与 `](#anchor)` 站内链接，逐一与目标文件标题（`#` 1–6 级）生成的 GitHub slug 比对；跳过 fenced code block 与行内反引号代码
- **用法**：`uv run --directory script check-links.py [--doc-root <doc目录>] [--cairn-root <cairn目录>]`（默认分别为仓库根下 `doc/`、`cairn/`）
- **退出码**：0 = 全部锚点一致；1 = 存在不匹配（逐条列出 MISMATCH 行）

## check-commit-msg.py

- **作用**：校验提交信息首行 `类型(范围): 主题` 结构与 11 种类型白名单、主题规则、主题行与正文行显示宽度 ≤72 列（全角字符按 2 列计）、正文有效行 ≤20 行（脚注段不计入）、`BREAKING CHANGE:` / `Closes #` footer 格式；豁免 `Merge` / `Revert` / `fixup!` / `squash!` 开头与 `#` 注释行。规则全文见 `.agents/rules/commit.md`
- **用法**：`uv run --directory script check-commit-msg.py <提交信息文件>`（git 提交时由 commit-msg 钩子自动调用）
- **退出码**：0 = 格式通过；1 = 存在违规（逐条列出）；2 = 参数错误
- **启用**（一次性，本地配置不入库）：`git config core.hooksPath .githooks`

## 临时探针台账（三次法则登记处）

一次性诊断探针在此登记（规则见 [AGENTS.md](AGENTS.md)「临时脚本三次法则」）；同一探针跨会话累计 3 次必须固化为正式脚本。写探针前先查本表。

| 日期 | 探针 | 用途 | 次数 | 状态 |
|---|---|---|---|---|
| 2026-09-05 | 会话内 `uv run python` + tomllib 校验 design.md 中 TOML 片段合法性 | 草案 v1 文档 TOML 片段验证 | 1 | 待固化 |
| 2026-09-06 | 同上（rules.toml 默认包 + knowledge.toml 案例片段校验） | 知识库/合并模型修订验证 | 2 | 待固化——再出现 1 次即固化为 check-toml.py |
