# crush_tether Roadmap

**Current focus**: 规划 crush-guard 的抽取/重写（Rust 实现 Crush bash 权限门）。

## Milestones

- [ ] 确认 crush-guard 抽取方案（子模块 vs 全局工具）
- [ ] 确认是否 Rust 重写（tree-sitter-bash）
- [ ] 落地 doc/design.md 设计文档
- [ ] 实现 guard 核心逻辑与三档分类

## Open Questions

1. 抽取方式：子模块（保留 `crush-guard/` 路径）vs 纯净全局工具？
2. 现在就 Rust 重写，还是先保持 Python、重写留作独立里程碑？
3. `[project.scripts] crush-guard = "guard:main"` 保留还是回滚？
