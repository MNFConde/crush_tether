# Project Cairn Log

This file records substantive progress in reverse-chronological order — newest entry at the top, right below this line. Keep each entry short — summary and pointer only; conclusions settle into `cairn/<topic>.md`.

## 2026-09-06 · skill 使用约定单列（实现前参照 + 提交前审查）

- 根 AGENTS.md 新增「项目特化约束（skill 使用）」节：实现 Rust 代码前按需参照 rust 系 skill（rust-best-practices / rust-testing / rust-async-patterns）写成惯例形态；提交前 skill 审查条目自质量门禁移入本节，集中维护。
- Details: `AGENTS.md`「项目特化约束（skill 使用）」。

## 2026-09-06 · 执行授权登记 + 沉淀触发扩展

- 用户授权 P2 开工 → P3 收尾（M2.1–M3.3）：每里程碑 ≥1 commit、大改动按功能拆分；外部写入边界裁定——构建缓存（cargo registry / advisory DB / rustup 下载）不算禁写对象，P3 引入 rhai 无阻。
- 沉淀纪律增补（cairn/AGENTS.md）：每次提交前、上下文临近压缩时各做一次 Cairn 沉淀检查，防长任务压缩丢失应沉淀信息。
- Details: `cairn/ROADMAP.md`「推进节奏」、`cairn/AGENTS.md`「知识沉淀规则」。

## 2026-09-06 · P2–P6 里程碑细化（逐项验收标准入 ROADMAP）

- ROADMAP 推进计划细化：P2 7 项（M2.1–M2.7）/ P3 3 项 / P4 3 项 / P5 2 项 / P6 3 项，每项带验收标准；原阶段级验收并入对应子项。
- 节奏钉死：P2→P5 共 15 项可一口气连续推进（无外部用户决策点）；P6 的 M6.3（mdor 退役）需用户确认不并入；三个实现期定点（`-h` 确认剔除 / 脚本 allow 契约语法 / 日志默认开关）登记于「推进节奏」节。
- 启动实施待用户授权；建议串行 M2.1 → M6.2，每项过质量门禁、每阶段末 Cairn 登记与提交。
- Details: `cairn/ROADMAP.md`「推进计划」「推进节奏」节。

## 2026-09-06 · 设计评审 + 命令知识库/继承模型增补（草案 v1 扩充 + ADR 机制建立）

- 评审发现默认包偏差（npm exec ≡ npx 等价绕过洞、git reset 双桶死词条、cargo/go 整命令过宽）与规格空洞（查表顺序/裸列表跨层合并/节内 default/version），逐条钉死进草案 v1。
- 定案：**命令知识库**（bucket：事实/策略分离、10 槽位封闭、别名归一参与运行时、属性仅 lint/脚本、删光=不做语义检查）+ **层间合并 = 字段级继承**（数组覆盖 / inline table `add`/`remove` 增删）+ **单命令建模**完备性标准（槽位跟着消费机制走）+ 损坏重生成收窄（仅缺失才生成）+ guard.py 重定位为参考对象非验收标准。
- 文档基建（借鉴 mdor）：新建 `doc/decisions.md`（轻量 ADR，首批 D-01~D-06）；`script/AGENTS.md` 目录约定 + 临时脚本三次法则 + `scripts.md` 台账；doc/AGENTS.md 补单源三判定与标记排版细则。
- 沉淀：新建 `cairn/command-knowledge-base.md`（可复用模式）；`zero-builtin-policy-seeding.md` 损坏收窄更正 + 知识库边界辨析。
- Details: `doc/design.md`「配置格式与脚本边界（草案 v1）」（含命令知识库/单命令建模/层间合并节）、`doc/decisions.md`、`cairn/command-knowledge-base.md`。

## 2026-09-05 · 配置格式草案 v1 纸面定稿（双表三桶查表 + 条件判断下沉脚本层）

- 定案（草案，待 P2/P3 验收后升格）：`rules.toml` 顶层 `[local]`/`[global]` 双表——local 内 allow 带路径逃逸检查、global allow 豁免（团队统一放行出口）；每命令 allow/confirm/deny 三桶 + sub/flag 子键，flag 长写/简写显式并排（弃前缀匹配）；头部 `allow`/`confirm`/`deny` 裸列表 + `precedence`（桶间优先级可调）+ `default` 标量。
- 分界原则收窄：**声明层零条件判断**（纯查表），两态子命令（branch/remote/tag/config）、`find` 突变、`git config` ≥2 位置参数、管道 sink、`curl|sh` 全部下沉脚本层；脚本 allow 契约限显式枚举、禁无条件兜底。
- 配套：merge 改 token 级（高层胜出、低层补缺，弃集合并集/`[[rules]]` 前插）；新增 JSONL 裁决日志格式（command/decision/source.layer/source.entry/script.rule，默认开关 P4 定）。
- 流程验证：多轮草案迭代（用户逐条拍板：简洁优先、条件判断归脚本、作用域即结构、配置自足），全部确认后才写入文档；未推进任何 P2 实现。
- 更正登记：design.md 旧定稿处（merge 语义/零内置策略默认包界定/命令建模规则链）已加【已替换】指针，不静默覆盖。
- Details: 见 `doc/design.md`「配置格式与脚本边界（草案 v1）」（含更正登记节）。

## 2026-09-04 · 定稿零内置策略与默认配置生成（设计变更）

- 定案：**二进制纯引擎、零内置策略**——原「判定表编译进 engine.rs / 全局层=编译内置默认」设定被推翻；默认策略由项目侧生成的外部 `rules.toml` + `rules.rhai` 提供，内嵌的只是生成模板（不参与判定）。
- 细节钉死：三层皆缺有效配置才生成（任一层有效即尊重，效力顺序恒为 项目 > 用户 > 全局）；损坏先留档 `.bak-<时间戳>` 再生成；生成动作不经规则链（引导豁免）；temp+rename 原子幂等；生成前/失败按 fail-safe confirm；全局/用户层生成由命令提供（后期设计）。
- 计划影响：P2 加生成 v1（项目层）验收项；P3 默认 `rules.rhai` 承载跨参数语义后删除 engine.rs 内置表残留，89 回归用例迁移为「引擎 + 默认规则 fixture」驱动。
- Details: 见 `doc/design.md`「零内置策略与默认配置生成（定稿）」、`cairn/zero-builtin-policy-seeding.md`。

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
