---
type: project_topic
status: active
summary: guard.py → Rust 重写的实现要点与踩坑：tree-sitter-bash AST 结构差异（重定向/fd/展开节点）、路径归一化、管道 sink 判定策略；脚本引擎接入（rhai/mlua）的沙箱与 API 坑；声明式规则函数（M8.6）的 FnPtr 原语；M9 解析层事实（$VAR 丢弃/命令替换旁路）与「数据+机制+消费者三件套成对落地」教训；平台敏感词法谓词与夹具根（inside_repo 相对分支双前置，CI linux 红灯）。
tags: [crush_tether, rust, tree-sitter, migration, rhai, mlua]
contains: [lesson, decision]
created: 2026-09-04
updated: 2026-09-13
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
- **教训：mlua 指令限流 hook 分线程/全局两档**——`set_hook` 只挂当前线程，脚本自建协程（C 层 `coroutine.create`）不继承，协程内死循环完全逃逸预算（实测 200 万次循环毫秒级完成）；须 `set_global_hook` 才覆盖（实测协程内 budget 正常计数并在阈值处终止循环）。语义边界：`coroutine.resume` 类 pcall 吞协程内错误——超预算协程被终止（DoS 已阻）但脚本不报错、不转化为 fail-safe confirm。验证写法用**副作用标记**（协程内置完成标志，resume 后查标志）而非墙钟断言。方法论：沙箱限流的验证必须覆盖该运行时的并发执行原语（协程/回调向量），只测主线程死循环会留洞。

## CLI 自用工具坑（2026-09-11）

- **教训：`crush-tether check "cmd"` 的裸参数被静默忽略**——check 模式从 stdin 读 agent 载荷（hook 同构），命令行参数不是裁决输入，任何裸命令都 exit 0 无输出（=保守 confirm），极易误读为「引擎裁决通过」。命令行侧的裁决查询口是 `check --batch`（stdin 一行一命令出裁决表）。
- **坑：`uv run --directory <dir> python` 会把进程 cwd 切到 `<dir>`**——脚本内相对路径（如 `plugin/…`）以仓库根为预期时全部指错，且不报错（FileNotFoundError 才暴露）。跨目录驱动脚本时路径一律绝对化，勿信调用方 cwd。

## 声明式规则函数接入（M8.6，2026-09-12）

- **教训：rhai 1.26 匿名函数的落地原语是 `FnPtr`**——脚本把 `|ctx| {…}` 传进引擎注册函数（`register_fn("rule", |name: &str, p: i64, f: rhai::FnPtr| …)`，`FnPtr` 走 blanket `Variant` 自动成立），`FnPtr` 可 Clone 存储，evaluate 时 `f.call::<Dynamic>(&engine, &ast, (ctx,))` 用实例字段借用调用。rhai 无装饰器语法，「元数据绑定 + 框架组装」的最短路径就是注册器收 FnPtr。
- **教训：rhai 限流预算按调用次重置**——`call_fn`/`FnPtr::call` 每次新建 global runtime state，`max_operations` 计数归零。声明式规则链 N 次调用 = N×预算（每次自身有界，最坏 128×100k 仍有上界）；Lua 侧 `set_global_hook` 预算跨规则共享（更严侧）——双引擎限流语义不逐位对齐，设计文档已注明。
- **教训：mlua chunk 顶层执行错误属加载期拒载（`Rejected`）非编译错误**——语法错误在 `into_function` 阶段已暴露，`chunk.call(())` 的失败全是执行期语义（如 rule() 注册边界报错），映射错类别会让「拒载」断言族静默失真。
- **教训：serde 反序列化 `Option<&'static str>` 字段不可能**——`&str` 反序列化借用输入，凑不出 'static；JSON 行结构体字段用 `String`，还原引擎侧 `&'static str` 枚举值时按已知值映射（kind 仅三个常量，`String→&'static str` 映射安全）。同批：`skip_serializing_if` 写 `Vec::is_empty` 不是 `Vec::new`（前者是判谓词后者是构造器，写错报 "expected bool"）。
- **教训：设计拍板的安全收敛必须配反向用例**——「default 兜底退整命令粒度」拍板后，第一版实现只拦了 `layer=="script"`，`layer=="default"`/`entry=*.default` 溯源路径仍按条目记便签；测试全绿是因为用例只覆盖「同命令重放」，覆盖不到「同 bin 异参数被误放行」。碰巧通过的测试 ≠ 语义实现：凡是「退回/收敛/降级」类拍板，用例必须包含**本不该被收敛覆盖的变体**（此处 = `frobnicate --deep x` 批后 `frobnicate evil` 必须仍弹窗）。

## 解析层事实与坑（M9.1/M9.2，2026-09-13）

- **事实：tree-sitter-bash 的 `$VAR` 是 `simple_expansion` 节点（不是 `variable_name`）**——extract_command 的 push_word match 不含它，词元被**静默丢弃**（`cd $DIR` 的 words 只剩 `cd`，与 `cd` 无参同形）。字符串内的展开（`"$D/x"`）经 unquote 保留 `$` 字面，反而可检测。修复（M9.2）：`has_expansion` 标记覆盖 variable_name/simple_expansion/special_variable_name/expansion 族 + command/process substitution；`cd` 目标含展开即基准毒化。
- **旁路：`$( )` 内层命令原先完全不裁决**——command_substitution 是 command 节点的**子节点**，collect_commands 的容器递归列表既不含它、command 分支也不下潜，`echo $(sudo rm x)` 的内层被整体丢弃（解析层免检区）。修复：command 分支对 substitution 子节点以**独立子 shell 组**下潜收集（内层命令入列裁决 + 组语义正确）。
- **教训：「kb 数据 + 引擎机制 + 消费者」三件套必须成对落地**（M9.1 git -C 修复的实证）——只补 kb 登记（数据）不修探测（机制）：sub 提取不消费 takes_value，`-C` 依旧顶掉子命令槽，无效；只修探测不补登记：带值 flag 的值被误当子命令，跳值失败；机制修了但 `ctx.sub` 不同步（消费者缺位）：`git -C x config a b` 查表修成 allow 后 two_state 拿 raw sub 查不到 write_arg_count——**写形态漏放**；不改全词元 flag 扫描：前导 flag 永远进不了 flag 桶（`git -c k=v log` 的 -c 漏检成放行洞）。四个半成品里两个是安全洞——跨层语义变更的关联面要一次列全再动手。
- **坑：bash 内联 `node -e "…"` 写含反引号/`${}` 的补丁内容会被 Git Bash 展开**——模板字面量被 shell 当命令替换执行（`touch x`、`engine::segment_bases` 等杂命令现场出现），目标文件被污染且不报错；本批两次踩中、一次污染 decisions.md 靠 `git checkout` 恢复。多行补丁一律用 Write 工具落**临时脚本文件**再 node 执行后删除（同族：uv run 的 cwd 坑、commit.md 6.5 追加型编辑静默丢失——「拼接/内联不经语法边界校验」是同一类静默损坏）。

## 平台敏感的词法谓词与夹具根（CI linux 红灯，2026-09-13）

P10 八笔推送 CI 首跑即红灯（quality job ubuntu 三用例期望 confirm 得 allow，windows 全绿），归因三层，是「词法路径谓词 × 平台」类的原型坑：

- **事实：`inside_repo` 的分叉点是 `Path::is_absolute()`**——`D:/x` 在 Windows 是绝对路径，在 Linux 是普通相对路径（`D:` 只是名字组件）。M9.2 起逃逸检查改为「`resolve_against_base` 先解析基准 → `inside_repo` 再判归属」，解析产物在 Linux 上仍相对 → 相对分支 `project_root.join(expanded)` 把项目根**再前置一次**，双前缀词法归一后仍判「根内」→ 逃逸漏判成 allow。旧版（M8 及以前）对**原始词元**做 `path_escapes`，不含盘符、两平台一致——管线升级把平台敏感性带了进来。
- **夹具层根因**：两处测试常量硬编码 `D:/...` 形态根（`tests/fixture/mod.rs` 的 `PROJECT`、`src/lookup.rs` 测试的 `PROJ`）。三个失败用例恰是仅有的三个依赖「根为绝对路径」语义的逃逸断言；生产面不受影响（真实项目根恒绝对路径），纯测试夹具的平台可移植性缺陷。修复（e2bb03c）= 两常量改 `env!("CARGO_MANIFEST_DIR")`（两平台皆真绝对路径）。
- **教训**：①跨平台语义的词法谓词，夹具**自写起就该平台中立**（CARGO_MANIFEST_DIR），不要依赖 CI 兜底——「本地全绿」在单平台开发下是假信心，平台矩阵 CI 的价值恰在首跑兑现（P9 从未 push，八笔同车首跑才爆）；②Windows 反斜杠路径嵌 bash 命令词元会被 tree-sitter 当转义符——测试里把路径嵌入命令串一律正斜杠化（`replace('\\', "/")`）。
- **残留**：`script_engine.rs`/`script_lua.rs`/`config/seed.rs`/`script::mod` 测试/`recoverability.rs` 等仍用 `D:/code/tmp/*` 形态常量——今日两平台均绿属侥幸（断言不依赖根绝对性），清理挂账 test-and-ci §5。

## 决策记录

| 决策 | 结论 |
|---|---|
| 重写验收标准 | test_guard.py 用例 1:1 平移，全绿为过 |
| 解析库 | tree-sitter-bash 0.25（bashlex 无 Rust 等价物） |
| 三层配置/DSL/serve | 骨架留位（config.rs 占位 / P2-P4 引依赖），首版不引入未用依赖 |
| mdor 退役节奏 | 本仓库为唯一实现；目录/`[project.scripts]` 删除待用户确认 |
