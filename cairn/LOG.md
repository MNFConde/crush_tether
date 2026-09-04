# Project Cairn Log

This file records substantive progress in reverse-chronological order — newest entry at the top, right below this line. Keep each entry short — summary and pointer only; conclusions settle into `cairn/<topic>.md`.

## 2026-09-04 · 推进计划（P0–P6）与运行模式/热重载定稿

- 定稿**双运行模式**：serve 常驻（stdout 行协议 + bash 客户端壳 + `--idle-exit` 自愈）优先、check 单发兜底先行；两模式共用管线。
- 定稿**配置热重载**：notify + 600ms debounce，整段重编译 + `Arc<RuleSet>` 原子换指针；脚本编译失败保留旧快照；监听失效降级 stat mtime 校验。
- 定稿**资源预算**：常驻 <10MB、P95 < 5ms（serve）/ < 10ms（check）、零 busy-loop，低配友好。
- ROADMAP 重写为 P0–P6 分阶段推进计划（每阶段带验收产物）；开闭原则落点与模块依赖方向写入 design.md「扩展点」。
- Details: 见 `doc/design.md`「运行模式与配置热重载（定稿）」节、`cairn/ROADMAP.md` 推进计划。

## 2026-09-04 · 结构决策 + 搬迁 mdor 过程经验

- 定案：**不拆 workspace**，改为**单 crate + `src/lib.rs` + `src/main.rs` 双入口**（核心逻辑无第二消费方）；同步更新 `doc/design.md`「目标结构」与根 `AGENTS.md` 架构约定。
- 从 mdor 搬迁 8 篇通用过程/工具经验至 `cairn/history/`（cargo-audit / PS 编码 / Cargo config / MCP 检索 / Windows 脚本 / 测试坑 / 元数据可靠性 / 可观测性原则），已去项目特定实例、带 `source` 溯源。
- `cairn/history/` 加入 `.gitignore`（不随仓库分发，为 cairn 跟踪的**唯一例外**）；在 `cairn/AGENTS.md` 注明此例外。
- Details: 见 `doc/design.md`「目标结构」节、`cairn/AGENTS.md`「例外：cairn/history」节。

## 2026-09-04 · crush-guard 规则引擎与 Agent 适配方案定稿

- 定案：可配置规则引擎 + DSL + 多 Agent 适配层方案写入 `doc/design.md`「规则引擎与配置（定稿）」节。
- 关键决策：Rhai（默认）+ Lua 双引擎（去 Roto）；配置拆分 `.crush-tether/rules.toml` + `rules.rhai|lua`（声明/脚本分文件）；配置优先级 项目 > 用户 > 全局（不粘性）；Agent 首发 Crush → ClaudeCode，OpenCode 延后。
- 核实：Crush 与 ClaudeCode 的 PreToolUse hook 契约（输入/输出/聚合规则），ClaudeCode 走 `hookSpecificOutput`、`decision/reason` 已废弃。
- Details: 见 `doc/design.md`「规则引擎与配置（定稿）」节。

## 2026-09-04 · Project Cairn initialized

- Initialized Project Cairn structure（含 `cairn/AGENTS.md` 规则分发 + 根 `AGENTS.md` 导航/指针）。
- 定案：设计文档放 `doc/design.md`（工程资产，不入 `cairn/`）；cairn 按 skill 要求存知识；过程约定入根 `AGENTS.md`，过长则用指针。
- Historical migration mode: `start_fresh`.
- Details: see `AGENTS.md` and `.cairn/config.yaml`.
