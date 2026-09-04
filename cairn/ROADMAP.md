# crush_tether Roadmap

**Current focus**: P0+P1 已落地（Rust 重写 + 回归用例全绿）；设计变更定稿「零内置策略 + 默认配置生成」（见 design.md 同名节）；下一步 P2 配置声明层（含生成），或按需先做 P4 serve。

## 推进计划（P0–P6）

> 依赖关系：P0 → P1 → P2（最小可用闭环）→ P3/P4 可并行 → P5 → P6。每个阶段有可验收产物，不跨阶段欠账。

- [x] **P0 骨架**：rust-toolchain.toml 钉 1.97.1；`[lib]`+`[[bin]]` + serde/serde_json/toml/tree-sitter(-bash) 依赖；`src/{model,cmd_parse,engine,channel,config}.rs` 就位。
  - 验收：`cargo clippy -D warnings` 零告警。
- [x] **P1 分类核心（check 模式最小闭环）**：`cmd_parse`（tree-sitter-bash flatten + 写重定向/fd dup/路径逃逸检测）+ 判定表平移 + `Verdict::combine`；`channel` Crush/ClaudeCode 契约输出；`check` 模式（stdin JSON → allow JSON/静默/exit 2）。
  - 验收：`tests/guard_regression.rs` 89 用例全绿（test_guard.py 1:1 平移）；release 单次冷启动 ~9ms（budget <10ms 达标）；冒烟四形态（allow/deny/管道 sink/写 flag confirm）正确。
- [ ] **P2 配置声明层**：`rules.toml` 三层 merge（项目 > 用户 > 全局，效力同序）+ `[[rules]]` 实例化进 Match 链；`--config`/`CRUSH_TETHER_CONFIG` 覆盖；**默认配置生成 v1（项目层）**：三层皆缺有效配置 → 项目 `.crush-tether/` 生成默认 `rules.toml` + `rules.rhai`（损坏先留档 `.bak-<时间戳>` 再生成，temp+rename 原子，生成动作不经规则链）。
  - 验收：三层覆盖/并集/前插/效力顺序单测全绿；样例仓库自定义规则改变裁决生效；生成触发（三层皆缺 vs 任一层有效）、损坏留档、幂等重新生成单测全绿；生成出的默认 `rules.toml` 能完整承载现有判定表中可声明表达的部分。
- [ ] **P3 脚本层（Rhai 默认）**：`RuleEngine` trait + Rhai 引擎（`max_operations` 限流）+ `rules.rhai` 进管线；`--engine` 切换；默认 `rules.rhai` 承载声明层表达不了的语义（`find` 突变检测、`git config` ≥2 位置参数写判定等），**随后删除 engine.rs 中内置判定表残留，完成零内置策略迁移**。
  - 验收：脚本可组合安全原语但不可绕过；死循环被限流兜底；`tests/guard_regression.rs` 89 用例改为「引擎 + 默认规则 fixture」驱动后仍全绿（默认配置包成为验收对象）。
- [ ] **P4 常驻服务 + 热重载**：`hook`/`serve` 模式（命名端点 + connect-or-spawn + 独占 bind 单实例 + `--idle-exit`）；`notify` + debounce + `Arc<RuleSet>` 原子换快照。
  - 验收：改规则文件不重启即生效；并发冷启动不重复起 serve；常驻内存 <10MB、P95 < 5ms。
- [ ] **P5 ClaudeCode adapter 完整化**：`hookSpecificOutput` 信封 + `CLAUDE_PROJECT_DIR`/cwd 基准 + `updated_input` 全替换语义。
  - 验收：ClaudeCode 三档行为与 Crush 等价；双 adapter 契约测试共用用例集。
- [ ] **P6 收尾**：Lua（mlua）`--engine lua`；`cargo audit` 干净；README + 使用文档；mdor 侧 crush-guard 退役（删目录/回滚 `[project.scripts]`）。

## Milestones（已达成）

- [x] 确认 crush-guard 抽取/重写方向（见 doc/design.md）
- [x] 确认三档分类语义（allow/confirm/deny）
- [x] 落地 doc/design.md 设计文档（含规则引擎 + DSL + Channel 章节）
- [x] 定稿运行模式与热重载方案（命名端点 + Arc 快照热重载，见 design.md「运行模式与配置热重载」）
- [x] Rust 重写 P0+P1（check 模式 + 回归用例 9/9 组全绿 + 质量门禁全过）
- [x] 定稿**零内置策略 + 默认配置生成**（二进制纯引擎；默认策略 = 项目侧生成的外部 `rules.toml` + `rules.rhai`；三层皆缺才生成、任一层有效即尊重；损坏留档后重新生成；全局/用户层生成由命令提供后期设计；效力顺序项目 > 用户 > 全局）

## Open Questions

1. mdor 侧退役节奏：crush-guard 目录与 `[project.scripts]` 何时删除（本仓库已是唯一实现，待用户确认后执行）。

## Settled（历轮定稿）

- DSL 引擎：**Rhai**（默认，`--engine rhai`）+ **Lua（mlua）**（`--engine lua`，兼容旧习惯）；Roto 已否决。
- 抽取方式：**纯全局工具**（本仓库唯一实现）；Python 版重写已落地。
- 热重载：`notify` 事件监听 + 600ms debounce，**整段重编译 + `Arc<RuleSet>` 原子换指针**（不增量 patch）；脚本编译失败保留旧快照；监听失效降级 stat 校验。
- 配置拆分：`.crush-tether/rules.toml`（声明层）+ `rules.rhai`/`rules.lua`（脚本层）。
- 配置优先级：项目 > 用户 > 全局（不粘性，`deny` 可被高层覆盖；三层同时存在时效力同序）。
- 规则来源：**零内置策略**——二进制纯引擎（解析/特征/安全原语/管线），不内嵌任何策略数据；默认策略由生成到项目侧的外部 `rules.toml` + `rules.rhai` 提供，二进制内嵌的仅是生成模板（不参与判定）。生成触发 = 三层皆无有效配置；损坏 ≠ 缺失（留档 `.bak-<时间戳>` 再生成）；生成动作不经规则链（引导豁免）；temp+rename 原子幂等；生成前/失败按 fail-safe confirm；全局/用户层默认文件由命令提供（后期设计）。
- 默认包分工：能声明表达的进默认 `rules.toml`；跨参数逻辑（`find` 突变、`git config` 多位置参数等）进默认 `rules.rhai`；89 回归用例迁移为「引擎 + 默认规则 fixture」驱动。
- Agent 首发：**Crush**（一）→ **ClaudeCode**（二）；OpenCode 延后至稳定，其余留空壳。
- 语言：**Rust**（tree-sitter-bash + 三 DSL 生态）；工具链钉 1.97.1（rust-toolchain.toml）。
- 结构：**单 crate + `src/lib.rs` + `src/main.rs` 双入口**（核心逻辑无第二消费方，故**不拆 workspace** 分 core/cli 两 crate）。
- 运行模式：**hook（默认，connect-or-spawn）+ serve 常驻 + check 单发（兜底/冒烟）**，check 先行落地；生命周期使用驱动（连接归零 + idle 退出），不耦合 agent 进程；【已否决】客户端壳 + bash 进程替换持 fd（Go 子进程 fd 全 CLOEXEC，hook 每次全新 bash）。
- serve 传输：**本机命名端点**（Windows named pipe / Unix socket，优先 abstract namespace），**一项目一 serve**（端点名 hash(项目根, engine)），**独占 bind = 单实例 + 角色裁定**（输者静默转 connect）；连接生命周期 = 一次请求，断开感知靠内核 EOF，无心跳；v1 串行 accept（last_activity 代替计数）。
