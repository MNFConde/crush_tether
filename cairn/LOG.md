# Project Cairn Log

This file records substantive progress in reverse-chronological order — newest entry at the top, right below this line. Keep each entry short — summary and pointer only; conclusions settle into `cairn/<topic>.md`.

## 2026-09-06 · M3.3 落地：删内置判定表 + 89 用例迁移（P3 收口，本次授权完成）

- `engine.rs` 收缩为纯管线原语（解析拉平 / 管道拓扑 / decide_with 注入式 / 组合裁决）；guard.py 判定表平移全部删除，零内置策略收口。「管道 → deny」策略移至默认 rules.rhai 谓词 3。
- `tests/guard_regression.rs` 改「引擎 + 默认规则 fixture」驱动（tests/fixture：默认包三模板 → 合并 → 查表 → 脚本 → 组合，与二进制管线一致）；断言冲突按 D-05 以草案为准逐条登记变更记录（文件头部表）：remote/tag 写形态与 ls --format=json 落 allow（默认知识库缺口，补数据须先修订 design.md 示例）、git reset 降 confirm（草案推荐值）、mkfs/dd/shutdown/sudo 降 confirm（DESTRUCTIVE 表不入二进制）。
- **挂账**：默认知识库缺 remote/tag write_tokens、默认桶缺 sudo/破坏性工具 deny 条目——均须先修订 design.md 定稿示例再动模板（留待用户/后续授权）。
- 128 测试全绿 + 全门禁过。commit 82cf80e。授权范围（M2.1–M3.3）完成。

## 2026-09-06 · M3.2 落地：默认 rules.rhai 四类谓词 + allow 契约定稿

- 默认包并入 rules.rhai（M2.6 挂账闭环）：四类谓词 = 两态子命令（数据读知识库）/ find 突变 / 管道 sink（引擎算拓扑 ctx.pipe_to_shell + 脚本承载策略 + curl/wget 参数含 |）/ 写特征升级（allow + 写重定向 → confirm）。
- **allow 契约定稿（更正登记 10，与原方向偏离）**：脚本 v1 无放行权——返回 allow 即契约违约 → fail-safe confirm。理由：图灵完备脚本上「禁无条件兜底」无法机械校验（`if true` 平凡绕过），结构性禁止才有可保证性质；[global] 放行特例由 TOML 承载；条件 allow 为后续扩展（须配机械校验）。
- **知识库删光语义**：两态谓词 kb_present 失效 → 有子命令的 allow 一律 confirm（M3.2 验收）；查表层 literal 命中不受影响（M2.4 语义）——两层各自成立。
- 128 测试全绿。commit 8443e23。Details: `src/config/templates/default-rules.rhai`、`src/script.rs`、`tests/script_engine.rs`。

## 2026-09-06 · M3.1 落地：Rhai 脚本层引擎 + RuleEngine trait

- `script.rs`：trait 开闭落点（v1 RhaiEngine）；限流按定稿（max_operations 100k / call_levels 64 / expr_depths / string+array 上限）；原语 = path_escapes / inside_repo + 知识库数据源 kb_*（删光 → 空数据脚本自兜底）；沙箱无 IO API。
- 契约：`fn check(ctx) -> ""|allow|confirm|deny`；ctx = bin/sub/words/args/verdict/writes_redirect/project。AST 编译一次缓存（serve 复用 P4）。
- 策略方向对比：**脚本损坏 → fail-safe confirm**（脚本产生裁决，不能跳过）；**知识库损坏 → 降级字面查表**（不产生裁决）——两类数据文件损坏语义相反，已在代码注释与测试钉死。
- `--engine` 接入：未知引擎告警 + confirm 不静默回退。13 例验收全绿（含 e2e 死循环限流/越权不可达/脚本改判）。122 测试全绿。commit 93e38a8。
- Details: `src/script.rs`、`tests/script_engine.rs`、`cairn/ROADMAP.md` M3.1 条。

## 2026-09-06 · M2.7 落地：样例端到端验收 + 草案 v1 升格定稿（P2 收口）

- `tests/sample_repo_e2e.rs`：全链路（发现→合并→归一→查表→组合）在真实二进制上验收——默认包推荐值展示（`-h` 保留 confirm、`git reset` confirm、`--hard` 双命中合成 deny、`go run` 落 confirm）、覆盖写法改判、增删写法跨层改判（用户层 -h confirm 被项目层 remove 剔除后 status -h 变 allow）。
- 升格动作：design.md「配置格式与脚本边界」升格 v1 定稿（标题/锚点/状态注全仓同步，check-links 44/44 过）；更正登记第 9 条登记升格与遗留（89 用例迁移 M3.3、rules.rhai 入包 M3.2）；根 AGENTS 状态行更新为 P0–P2 已落地。
- P2 收口：ROADMAP P2 复选框打勾。commit 21ee744(refactor) + db5401a(e2e) + 本提交(docs)。
- Details: `doc/design.md`「配置格式与脚本边界（v1 定稿）」、`cairn/ROADMAP.md` P2/M2.7 条。

## 2026-09-06 · M2.6 落地：默认配置生成 v1

- `config/seed.rs`：模板内嵌仅为生成源数据（零内置策略不变）；模板自 design.md 示例块逐行提取，测试钉死「模板=文档」。触发 = 发现层 Ok + 三层皆缺 + 无显式覆盖；损坏不生成不动原文件（D-03）；任一层有效尊重现状；temp+rename 原子幂等，8 线程并发收敛同字节。
- check 模式接入引导：首跑生成后立即按默认包裁决。**挂账**：生成包 v1 仅 rules.toml + knowledge.toml，rules.rhai 待 M3.2 默认脚本就位后并入生成包；窗口期默认包缺 find 突变 / git config 双位置参数 / 管道 sink / 写特征升级四类脚本判定（M3.2 补齐）。
- commit 57c30d7。Details: `src/config/seed.rs`、`tests/seed_defaults.rs`、`cairn/ROADMAP.md` M2.6 条。

## 2026-09-06 · M2.5 落地：双层配置 lint

- `lint.rs`：lint_file(file, kb) 只告警不拒载。结构类 3 条（同 token 多桶 + 生效桶告警、precedence 死词条逐条点名、裸列表与命令节并存）；语义类 4 条（allow may_write 建议、别名等价冗余、same_flag 跨桶冲突、子命令拼写提示）。
- 实现期钉死解释：**拼写提示 = 配置内互查 + 知识库已知 sub 比对**——10 槽位封闭集没有「合法子命令清单」槽位（D-06），「git stauts → status」只能靠作者自己的其他词条或 kb sub 条目近似匹配（编辑距离 ≤2）。
- lint 对象是单份文件（「同文件」语义）；precedence 取文件自身键。无 kb 自动降级纯结构。11 例正反用例全绿。commit a3365c4。
- Details: `src/lint.rs`、`cairn/ROADMAP.md` M2.5 条。

## 2026-09-06 · M2.4 落地：知识库 main + 别名归一

- `knowledge.rs`：10 槽位封闭集解析（严格模式）+ 加载期防环。语义发现：**子命令别名吸收 sub 槽位、命令别名保留 sub → 归一状态中 sub 只减不增，任何环必退化为纯命令别名环**（防环校验只需覆盖命令链 + same_flag 链，已注释钉死）。
- 归一接入查表：pip3→pip、npm exec/x / pnpm dlx → npx、same_flag 等价类两侧规范形化（单边配置双边生效）、takes_value 剥值三形态（`--output=x`/`-o x`/`-oX`）；归一只改名字，逃逸检查用原始参数；`classify_traced` 输出归一链（P4 日志 kb 字段）。
- 知识库损坏 ≠ 规则损坏：按「缺失 + 告警」降级为字面查表，不触发 fail-safe（知识库不产生裁决）。KB 顶层是「version + 任意 [bin] 表头」，须 ScopeTable 式 visit_map 而非固定字段（同 M2.1 教训）。
- 新增 17 例全绿（含 design.md knowledge 示例提取解析 + 端到端 2 例）。commit a616f02。
- Details: `src/knowledge.rs`、`src/lookup.rs`、`cairn/ROADMAP.md` M2.4 条。

## 2026-09-06 · M2.3 落地：双表三桶查表 + 多命中合成，check 主路径翻转

- `lookup.rs`：查表顺序按草案钉死——`[global].allow` 整命令豁免（两表皆现 global 优先）＞ 同层命令节遮蔽裸列表 ＞ 头部裸列表按 precedence；节内多维度命中按 precedence 有序合成（`git show --output=x`→confirm、`git reset --hard` 双命中取 deny）；`[local]` allow 命中一律带逃逸检查；default 链 = 节内 → 顶层 → confirm 恒链尾。
- `engine::decide_with` 规则注入式顶层；check 模式主路径翻转：显式覆盖或三层发现 → 合并 → 查表。内置判定表仅存库内供回归测试（M3.3 删）。
- flag 匹配支持 `--flag=value` 剥值；`-oX` 粘连形态留给 M2.4 takes_value。
- 新增 13 例（12 单测 + 端到端）全绿。commit e19252f。
- Details: `src/lookup.rs`、`tests/check_mode_rules.rs`、`cairn/ROADMAP.md` M2.3 条。

## 2026-09-06 · M2.2 落地：三层字段级继承合并 + 分层发现

- `merge.rs`（D-02）：未定义即继承/定义即覆盖（项目>用户>全局，不粘性）；数组=覆盖、`{add,remove}`=增删（flag 桶可剔除）；标量高层写值即覆盖；命令节字段级合并 + 跨层并集；precedence 缺省回落默认序。合并产物 `MergedRules`（Delta 已消解为词条集），M2.3 查表直接消费。
- `discover.rs`：项目层 `.crush-tether/rules.toml`（项目根 `CRUSH_PROJECT_DIR` 优先/缺失逐级上溯 `.git`、`.crush-tether/`）+ 用户层 `~/.config/crush-tether/rules.toml`；全局层 v1 留位。损坏≠缺失（D-03）：任一层存在但解析失败整体 Err → 调用方 fail-safe confirm。
- 新增 23 例单测全绿。commit ce5148e。
- Details: `src/config/merge.rs`、`src/config/discover.rs`、`cairn/ROADMAP.md` M2.2 条。

## 2026-09-06 · M2.1 落地：rules.toml 草案 v1 解析模型 + 显式覆盖 fail-safe

- `src/config/{mod,schema}.rs` 替换旧 `[[rule]]` 占位骨架：双表三桶 / `sub`·`flag` / 列表双形态（数组=覆盖、`{add,remove}`=增删）；`version` 必填且须 =1，`precedence` 须三桶排列。
- 未知键报错可定位：手写 Visitor 反序列化（不用 untagged enum）——防 serde 私有类型名泄漏进报错文本；ScopeTable 固定桶键白名单化，拼错桶键不静默成新命令节。
- `--config`/`CRUSH_TETHER_CONFIG` 接线 check 模式：加载失败 → stderr 告警 + fail-safe confirm，不静默回落；design.md 示例现场提取解析（tests/config_design_example.rs）防文档漂移。
- 质量门禁全过（fmt / clippy -D / test 37 例 / audit），既有回归不受影响。commit 373df67。
- Details: `src/config/schema.rs`、`tests/explicit_config_failsafe.rs`、`cairn/ROADMAP.md` M2.1 条。

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
