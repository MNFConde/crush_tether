# Project Cairn Log

This file records substantive progress in reverse-chronological order — newest entry at the top, right below this line. Keep each entry short — summary and pointer only; conclusions settle into `cairn/<topic>.md`.

## 2026-09-04 · Rust 重写 P0+P1 落地（check 模式 + 回归用例全绿）

- P0：rust-toolchain.toml 钉 1.97.1；Cargo.toml 加 `[lib]`+`[[bin]]` + tree-sitter(0.25)/serde/serde_json/toml；五模块骨架。
- P1：cmd_parse（tree-sitter-bash flatten + 写重定向/fd dup/路径逃逸）+ 判定表 1:1 平移 + Verdict::combine；channel 双 agent 契约；check 模式冒烟通过。
- 回归：test_guard.py 89 用例平移至 tests/guard_regression.rs 全绿；release 冷启动 ~9ms（budget <10ms）；clippy 零告警。
- 三项待定决策定稿：纯全局工具（本仓库唯一实现）/ 现在 Rust 重写（已落地）/ fail-safe=unparseable→confirm。mdor 退役节奏留 Open Questions。
- 踩坑：tree-sitter-bash 重定向是 redirected_statement 兄弟节点非子节点；Path::join 不归一化 `..`（自写 norm 词法消解）；【备选】mdor cairn 有同类教训。
- Details: 见 `doc/design.md`（待定决策节/目标结构）、`cairn/rust-rewrite-notes.md`。

## 2026-09-04 · serve 生命周期与命名端点单实例定稿

- 定案：serve 生命周期**使用驱动**（hook connect-or-spawn + 在途归零 idle 退出），不精确耦合 agent 进程（Crush 无会话事件 / SessionEnd 不覆盖 crash / 平台 pid 探测复杂度不成比例）。
- 定案：serve 传输改**本机命名端点**（pipe / unix socket，优先 abstract namespace），**一项目一实例**（端点名 hash(项目根, engine)），独占 bind 一个 syscall 同步裁定唯一性与角色（输者转 connect）。
- 定稿：**项目内无可执行脚本**——connect/独占 bind/DSL 沙箱全在二进制内，rules.rhai|lua 是数据文件；分工表与运行架构图（进程拓扑 + 模块分层）入 design.md。
- 否决初稿「客户端壳 + bash 进程替换持 fd」（Go 子进程 fd 全 CLOEXEC + hook 每次全新 bash）。
- Details: 见 `doc/design.md`「运行模式与配置热重载（定稿）」、`cairn/serve-lifecycle-named-endpoint.md`。

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
