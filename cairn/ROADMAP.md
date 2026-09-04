# crush_tether Roadmap

**Current focus**: 规划 crush-guard 的抽取/重写（Rust 实现 Crush bash 权限门），含可配置规则引擎与多 Agent 适配。

## Milestones

- [x] 确认 crush-guard 抽取/重写方向（见 doc/design.md）
- [x] 确认三档分类语义（allow/confirm/deny）
- [x] 落地 doc/design.md 设计文档（含规则引擎 + DSL + Channel 章节）
- [ ] 确认 crush-guard 抽取方案（子模块 vs 全局工具）
- [ ] 确认是否 Rust 重写（tree-sitter-bash）
- [ ] 实现 guard 核心逻辑与三档分类
- [ ] 实现配置引擎（TOML 声明层 + Rhai/Lua 脚本层）
- [ ] 实现 Agent 适配层（Crush / ClaudeCode）

## Open Questions

1. 抽取方式：子模块（保留 `crush-guard/` 路径）vs 纯净全局工具？
2. 现在就 Rust 重写，还是先保持 Python、重写留作独立里程碑？
3. `[project.scripts] crush-guard = "guard:main"` 保留还是回滚？

## Settled（本轮定稿）

- DSL 引擎：**Rhai**（默认，`--engine rhai`）+ **Lua（mlua）**（`--engine lua`，兼容旧习惯）；Roto 已否决。
- 配置拆分：`.crush-tether/rules.toml`（声明层）+ `rules.rhai`/`rules.lua`（脚本层）。
- 配置优先级：项目 > 用户 > 全局（不粘性，`deny` 可被高层覆盖）。
- Agent 首发：**Crush**（一）→ **ClaudeCode**（二）；OpenCode 延后至稳定，其余留空壳。
- 语言：**Rust**（tree-sitter-bash + 三 DSL 生态）。
- 结构：**单 crate + `src/lib.rs` + `src/main.rs` 双入口**（核心逻辑无第二消费方，故**不拆 workspace** 分 core/cli 两 crate）。
