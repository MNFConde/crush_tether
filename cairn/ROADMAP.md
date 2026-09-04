# crush_tether Roadmap

**Current focus**: 设计定稿完成（见 `doc/design.md`），按 P0–P6 推进实现；先 check 模式最小闭环，再 serve 常驻 + 热重载。

## 推进计划（P0–P6）

> 依赖关系：P0 → P1 → P2（最小可用闭环）→ P3/P4 可并行 → P5 → P6。每个阶段有可验收产物，不跨阶段欠账。

- [ ] **P0 骨架**：`rust-toolchain.toml` 钉版；Cargo.toml 加 `[lib]`+`[[bin]]` + serde/serde_json/toml/tree-sitter-bash 依赖；建 `src/{model,cmd_parse,engine,config,channel}` 空模块 + `lib.rs` 装配；`cargo clippy -D warnings` 过。
  - 验收：hello-level 三档类型 + 单测骨架就位，CI 门禁可跑。
- [ ] **P1 分类核心（check 模式最小闭环）**：`cmd_parse`（tree-sitter-bash 解析 + flatten + `has_writing_redirect`/`find_mutates`/写 flag/路径逃逸）+ `model::Cmd` + 内置 `Rule` 链 + 三档判定表平移；`Channel` trait + Crush adapter（stdin JSON/env 进，三档 JSON/exit 出）。
  - 验收：mdor `test_guard.py` 回归用例全部平移为 Rust 单测并全绿；`.crushrc` 可直配 `crush-tether check` 跑通。
- [ ] **P2 配置声明层**：`.crush-tether/rules.toml` 加载 + 三层 merge（项目 > 用户 > 全局，语义见 design.md）+ `[[rules]]` 实例化进 Match 链；`--config`/`CRUSH_TETHER_CONFIG` 覆盖。
  - 验收：三层覆盖/并集/前插语义单测全绿；样例仓库用自定义规则改变裁决生效。
- [ ] **P3 脚本层（Rhai 默认）**：`RuleEngine` trait + Rhai 引擎（`max_operations` 等限流）+ `rules.rhai` 加载进管线；`--engine` 切换。
  - 验收：脚本可组合安全原语（`writes_file`/`path_escapes`/`deny`）但不可绕过；死循环脚本被限流兜底。
- [ ] **P4 常驻服务 + 热重载**：`serve` 模式（stdout 行协议 + ping/shutdown + `--idle-exit`）+ 客户端壳函数；`notify` + debounce 监听 + `Arc<RuleSet>` 原子换快照；mtime 兜底校验。
  - 验收：改 `rules.toml`/`rules.rhai` 保存后不重启即生效；常驻内存 <10MB、P95 < 5ms（`--benchmark`）。
- [ ] **P5 ClaudeCode adapter**：`hookSpecificOutput` 信封 + `CLAUDE_PROJECT_DIR`/cwd 基准 + `updated_input` 全替换语义（复用 Crush 输出逻辑，见 design.md 契约）。
  - 验收：ClaudeCode 三档行为与 Crush 等价；双 adapter 契约测试共用用例集。
- [ ] **P6 收尾**：Lua（mlua）兼容引擎 `--engine lua`；`cargo audit` 干净；README + 使用文档；crush-guard 抽取方案落地（子模块 vs 全局工具，见 Open Questions）。

## Milestones（已达成）

- [x] 确认 crush-guard 抽取/重写方向（见 doc/design.md）
- [x] 确认三档分类语义（allow/confirm/deny）
- [x] 落地 doc/design.md 设计文档（含规则引擎 + DSL + Channel 章节）
- [x] 定稿运行模式与热重载方案（serve/check 双模 + Arc 快照热重载，见 design.md「运行模式与配置热重载」）

## Open Questions

1. 抽取方式：子模块（保留 `crush-guard/` 路径）vs 纯净全局工具？（P6 前定）
2. `[project.scripts] crush-guard = "guard:main"` 保留还是回滚？（随抽取方案定）

## Settled（历轮定稿）

- DSL 引擎：**Rhai**（默认，`--engine rhai`）+ **Lua（mlua）**（`--engine lua`，兼容旧习惯）；Roto 已否决。
- 配置拆分：`.crush-tether/rules.toml`（声明层）+ `rules.rhai`/`rules.lua`（脚本层）。
- 配置优先级：项目 > 用户 > 全局（不粘性，`deny` 可被高层覆盖）。
- Agent 首发：**Crush**（一）→ **ClaudeCode**（二）；OpenCode 延后至稳定，其余留空壳。
- 语言：**Rust**（tree-sitter-bash + 三 DSL 生态）。
- 结构：**单 crate + `src/lib.rs` + `src/main.rs` 双入口**（核心逻辑无第二消费方，故**不拆 workspace** 分 core/cli 两 crate）。
- 运行模式：**serve 常驻（默认）+ check 单发（兜底/冒烟）**，check 先行落地；serve 走 stdout 行协议 + bash 客户端壳（进程替换拉起），`--idle-exit` 自愈；故障降级不误放行。
- 热重载：`notify` 事件监听 + 600ms debounce，**整段重编译 + `Arc<RuleSet>` 原子换指针**（不增量 patch）；编译失败保留旧快照；监听失效降级 stat 校验。
