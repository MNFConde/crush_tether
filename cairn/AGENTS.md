# Cairn 规则（本文件）

> 由根 `AGENTS.md` 指针引用；本机装有 project-cairn skill 且仓库存在 `cairn/` 时，本节规则生效。

## 初始化配置

- 毕业 provider：Obsidian (vault: SecondBrain)
- 知识库索引：Obsidian → Cairn/INDEX.md
- 毕业目标：Obsidian → Cairn

## 进入项目后的阅读顺序

1. 先读根 `AGENTS.md`。
2. 阅读 `cairn/ROADMAP.md`：路线图、当前焦点与开放问题。
3. 阅读 `cairn/LOG.md` 最近条目（最新在上）了解近期进展与关键决策。
4. 按需阅读相关 `cairn/` 知识专题文档。

## 文档职责

| 文件 | 职责 | 维护 |
|---|---|---|
| `AGENTS.md`（根） | 规则与导航 | 极少改动，≤ 60 行 |
| `cairn/AGENTS.md`（本文件） | Cairn 规则与职责 | 极少改动 |
| `cairn/ROADMAP.md` | 路线图与进展 | 就地更新，保持精简 |
| `cairn/LOG.md` | 时间序日志 | 顶部新增条目（最新在前），每条 ≤ 20 行，摘要 + 指针 |
| `cairn/<主题>.md` | 知识专题文档（当前真相） | 就地更新；坑写入正文小节，经 `contains` 标记；修订留 LOG 指针 |
| `cairn/Reference/` | 外部原始输入 | 按需创建；只增不改 |
| `cairn/Cited.md` | 知识库引用清单 | 仅指针，绝不复制原文 |

> 其余内容只在有具体信号时才创建（需记录决策、坑已解决、目标超出单次会话）——不预建空壳。工程资产（代码/配置/规范消费的合同）不归本系统管理，留在代码树，不入 `cairn/`。

### 例外：`cairn/history/`（已归档迁移的经验）

- `cairn/history/` 存放从**其他仓库**（如 mdor）搬迁来的**过程/工具经验**（cargo-audit 判据、PS 编码坑、Cargo config 机制、测试坑、可靠性/边界原则等），已与原仓库剥离、去项目特定实例，每篇 frontmatter 带 `source: <原仓库> (<原路径>)` 溯源。
- **`history/` 不参与 git 跟踪**（已加入 `.gitignore` 的 `/cairn/history/`）——仅作本地做参考的归档，不在仓库版本中分发。这是对下方「cairn 与 .cairn 均随仓库分发」的唯一例外。
- 其余 cairn 内容（`cairn/*.md`、`.cairn/`）仍按 `git_policy: track` 随仓库分发。


## 对 Cairn skill 模板的偏离（项目级约束）

以下偏离为**项目级约束**：后续遇到同样场景按本项目约定执行，不按 skill 模板默认。

- **不用 Claude Code**：不创建 `CLAUDE.md` stub。
- **Cairn 规则集中在 `cairn/AGENTS.md`**：根 `AGENTS.md` 仅作导航 + 指针（守卫指向本文件），偏离模板「根文件一文件装全部规则」约定。
- **cairn 与 .cairn 均随仓库分发**：两者均已由 git 跟踪（`git_policy: track`），不构成对 skill「永不忽略 `.cairn/config.yaml`」默认的偏离。

## 冲突仲裁规则

- 优先级：**知识专题文档 > LOG 历史**；规则级冲突由本文件裁定。
- 业务/设计结论以 `cairn/` 知识专题文档的最新记录为准，而非更早的 LOG 条目。

## 知识库消费反射

- 在开展可复用内核——其产出或依赖的任何结论——够格毕业的工作前，先查知识库索引（Obsidian → Cairn/INDEX.md）；只有实际影响产出的笔记才添加 `cairn/Cited.md` 条目（仅指针，不复制正文）。

## 知识沉淀规则

- 每有实质进展，在 `cairn/LOG.md` 顶部加一条（摘要 + 指针）；结论沉淀进 `cairn/` 知识专题文档。
- **完成回复门禁**：任何完成断言前——包括但不限于工作完成/已实现、定稿、已更新、已同步、已验证或测试通过、问题已修复/已解决、交付可用、声称工作已结束及语义等同措辞——先执行 `references/maintenance.md` 中的 Cairn 检查点；仅更新其触发矩阵要求的记录、验证后，再回复。明确的只读/不改动请求禁止 Cairn 写入。
- **提交前与压缩前沉淀（2026-09-06 用户约定）**：每次 git 提交前、以及长任务中会话上下文临近压缩时，各执行一次 project-cairn 沉淀检查——把会话中尚未落盘的结论/决策/边界裁定写入 LOG / 主题笔记 / ROADMAP，确保压缩不丢失应沉淀信息；检查后无新增则无需强写。
- 跨项目可复用经验经毕业机制沉淀到知识库（Obsidian → Cairn）。
