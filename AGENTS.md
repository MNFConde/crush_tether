# AGENTS.md

crush_tether —— Crush 命令级 bash 权限门（Rust 实现 crush-guard 独立化/重写）。当前状态：**P0–P6（M6.1/M6.2）已落地**（分类核心 + 配置 v1 全链：三层发现/字段级继承合并/双表三桶查表/知识库归一/双层 lint/默认包生成；脚本层双引擎：Rhai 默认 + Lua（mlua），ctx 封装与决策枚举化定型（M6.1），script_allow 受控放行五件套；serve 服务化：命名端点/connect-or-spawn/热重载/JSONL 裁决日志（D-07）；三 adapter：Crush/ClaudeCode/zcode；README 已落地），仅余 M6.3（mdor 退役 + M5.3 实机探针，待用户确认节奏）见 [cairn/ROADMAP.md](cairn/ROADMAP.md)；格式规范见 [doc/design.md](doc/design.md)，决策论证见 [doc/decisions.md](doc/decisions.md)。

> 本项目使用 Project Cairn 组织项目知识：Cairn 全套规则（初始化配置/阅读顺序/文档职责/冲突仲裁/知识库消费反射/知识沉淀规则）见 `cairn/AGENTS.md`。
> 本机装有 project-cairn skill 且仓库存在 `cairn/` 时生效；否则视为不适用，跳过。

## 阅读顺序

1. 先读本文件（AGENTS.md）。
2. 若存在 `cairn/AGENTS.md`，先读其中 Cairn 规则（含 ROADMAP / LOG 的阅读顺序）。
3. 按需阅读相关 `cairn/` 知识专题文档与 `doc/` 规划文档。

## 文档协作规则

- 改动前判断用户要「讨论/建议」还是「直接改文档」；说「先看看/先评估」时先给分析，别直接重写正式文档。
- 纠正过往判断时追加更正说明，不静默覆盖。
- 未经确认的判断不写成既成事实。
- 与用户交流一律使用中文。
- **文档一律用中文撰写**：`doc/`、`cairn/`、根 `AGENTS.md`、`README` 及任何项目文档均以中文书写；YAML frontmatter key、`{{PLACEHOLDER}}` 令牌、文件名保持英文模板原样（见 `cairn/AGENTS.md` 中 Cairn 规则）。
- **设计文档统一放 `doc/design.md`**（工程资产，不入 `cairn/`）；cairn 按 skill 要求存知识结论/教训；项目内过程约定入本文件，过长则用指针。

## 协作约定

- 与用户交流一律使用中文。
- 指令若与仓库文档（`doc/`）或既有约定不符，先指出冲突点、说明取舍，再执行。
- 项目状态发生变化（如核心逻辑落地、crate 结构建立、工具链变更）时，同步更新本文件对应状态描述，避免误导后续会话。
- 提交格式规范见 `@.agents/rules/commit.md`，仅在准备 commit 时读取；提交相关经验/坑须登记于该文件「六」节。
- 目录级专属约束见 `doc/AGENTS.md`（doc/ 写作约定 + 决策记录规则）与 `script/AGENTS.md`（script/ 目录约定 + 临时脚本三次法则），读取对应目录下文件时自动生效。

## 质量门禁

- `cargo fmt --check` → `cargo clippy -- -D warnings` → `cargo test` → `cargo audit`（需 `cargo install cargo-audit --locked`）。
- 单测重点在核心逻辑（平台无关）；skill 相关约定见下节「项目特化约束（skill 使用）」。

## 项目特化约束（skill 使用）

- **实现前参照**：实现 Rust 代码前按需参照环境中对应 skill 的模式——`rust-best-practices`（惯例与 API 用法）、`rust-testing`（测试组织与模式）、`rust-async-patterns`（涉及异步时）——第一次就写成惯例形态，不等提交前审查返工。
- **提交前审查**：提交前用环境中对应语言的 skill 审查改动，按审查结果修正后再提交（自质量门禁条目移入本节，2026-09-06）。

## 工具链钉版

- `rust-toolchain.toml` 钉版（minimal profile）；本地 MSVC 钉版；勿随手升级。
- 版本约束只钉在根 `Cargo.toml` 一处；升级后必须 `cargo test` + `cargo audit`。

## 架构约定（改代码前先读 doc/design.md）

- 目标结构：**单 crate + `src/lib.rs` + `src/main.rs` 双入口**（库装 `model`/`engine`/`config`/`cmd_parse`/`channel` 分类逻辑，`main.rs` 仅做装配）；核心逻辑无第二消费方，故**不拆 workspace** 分 `core`/`cli` 两 crate。
- 三档分类语义（allow / confirm / deny）与判定表（DESTRUCTIVE/READONLY/GIT_*/写 flag/路径逃逸/管道 sink）见 `doc/design.md`，是纯语义可平移的。
- **零内置策略（2026-09-04 定稿）**：二进制为纯引擎，不内嵌任何规则数据；默认策略由生成到项目侧的外部 `rules.toml` + `rules.rhai` 提供（三层皆缺才生成），见 `doc/design.md`「零内置策略与默认配置生成（定稿）」。
