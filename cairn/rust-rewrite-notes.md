---
type: project_topic
status: active
summary: guard.py → Rust 重写的实现要点与踩坑：tree-sitter-bash AST 结构差异、路径归一化、管道 sink 判定策略；脚本引擎接入（rhai/mlua）的沙箱与 API 坑。
tags: [crush_tether, rust, tree-sitter, migration, rhai, mlua]
contains: [lesson, decision]
created: 2026-09-04
updated: 2026-09-06
related: [doc/design.md, tests/guard_regression.rs]
authoring_mode: ai_generated
---
# Rust 重写实现笔记（bashlex → tree-sitter）

## 形成背景

把 mdor 的 Python + bashlex 守卫（guard.py）重写为 Rust + tree-sitter-bash。回归策略：`test_guard.py` 全部用例 1:1 平移，保证语义对齐而非重设计。

## 当前结论

- **用例平移是重写的验收标准**：allow 41 / confirm 30 / deny 18 条用例 + 用户原始命令组合，直接映射为 Rust 测试常量表；重写过程中发现的每一个语义偏差都以「哪条用例会红」定位，无凭感觉的等价性争论。
- **冷启动实测 ~9ms**（release，Windows，含进程创建），对照 Python 时代 ~98-147ms；设计 budget（<10ms）达标，P4 serve 的边际收益缩小，serve 降级为并发/热重载场景的优化项。

## 经验与教训

- **教训：tree-sitter-bash 的 `cmd > file` 中 file_redirect 是 `redirected_statement` 的子节点、command 的兄弟节点**，遍历 command 子节点永远抓不到重定向。同类：fd 作用域 `2>` 的 `2` 是独立 `file_descriptor` 节点；`2>&-` 整个是匿名算子节点 `>&-`；`<>` 解析为 `<` + ERROR(>)（ERROR 节点要当语法错误处理，与 bashlex ParsingError → confirm 对齐）。先写 AST dump 探针看真实结构，再写提取逻辑。
- **教训：Rust `Path::join("../..")` 不做词法归一化**，`mdor\..\..\x` 的 `starts_with(mdor)` 仍为真 → 路径逃逸漏判。需自写 norm：components 逐个消解 ParentDir（栈顶非 `..` 则弹出，否则保留），Windows 再统一小写。Python 的 `os.path.abspath` 自带此语义，平移时易漏。
- **管道 sink（curl|sh）判定不能只依赖 flatten 序**：list（`;`、`&&`）会切断管道相邻性，「相邻命令即管道两侧」会误报；改为按原始文本按 `;`/`&`/换行切段、段内按 `|` 拆，仅段内下游首词命中 shell/解释器才 deny。`||`（逻辑或）与 `|` 的区分由切分后算子形态自然解决。
- **clippy 常见三连**：`from_str` 撞 `FromStr` trait（改名 parse）；`trim().split_whitespace()` 冗余；match 臂内 `if child.kind() ==` 可折叠为独立臂。首次 `cargo clippy -D warnings` 就开，别攒。
- **tree-sitter 0.25 + tree-sitter-bash 0.25 配对**：`tree_sitter_bash::LANGUAGE.into()` 得 `Language`；`node.utf8_text()` 返回 `&[u8]` 切片需 `as_bytes`。

### 脚本引擎接入（M6.1，2026-09-06）

- **教训：mlua 0.12 已无 `sandbox` feature**（记忆/旧文档差异）——沙箱由 `Lua::new_with` 安全模式 + **显式库白名单**（coroutine/table/math/string/utf8）+ base 危险全局消毒（`dofile`/`loadfile`/`load`/`print` 置 nil，mlua 不代劳）组合实现。注意 `StdLib::ALL_SAFE` **包含 IO/OS/PACKAGE**，不能直接当安全集用。
- **教训：mlua 对象不保活 Lua state**——`LuaEngine` 只存 `check: Function` 而不存 `Lua` 字段时，compile 返回后 state 被销毁，evaluate 报「Lua instance is destroyed」；state 必须随实例字段锚定。
- **教训：mlua 0.12 只有 IntoLua 的 UserData 毯式实现，没有 FromLua**——`__eq` 等元方法第二参收 `AnyUserData` 后手动 `borrow::<T>()`（借用失败视为不等，因为 Lua 的 `__eq` 不跨 userdata 类型保证同型）。
- **教训：rhai 1.x 的 getter API 是 `register_get`**（不是旧名 `register_getter`），闭包首参收 `&mut T`（`Mut<T>`）。rhai 属性访问本质是方法调用糖——自定义类型 + getter 可让脚本侧 `ctx.bin` 语法零改动地完成「暴露裸 map → 封装类型」迁移。
- **教训：Rust `concat!` 无分隔拼接**，多行脚本文本用例里 `"return nil"+\"end\"` 拼成 `nilend` 语法错误——多行脚本文本的每行要么带前导空格要么以 `\n` 结尾（与 commit.md 6.5 的追加型编辑静默丢失同族：拼接点出错不报错）。

## 决策记录

| 决策 | 结论 |
|---|---|
| 重写验收标准 | test_guard.py 用例 1:1 平移，全绿为过 |
| 解析库 | tree-sitter-bash 0.25（bashlex 无 Rust 等价物） |
| 三层配置/DSL/serve | 骨架留位（config.rs 占位 / P2-P4 引依赖），首版不引入未用依赖 |
| mdor 退役节奏 | 本仓库为唯一实现；目录/`[project.scripts]` 删除待用户确认 |
