# Project Cairn Log

This file records substantive progress in reverse-chronological order — newest entry at the top, right below this line. Keep each entry short — summary and pointer only; conclusions settle into `cairn/<topic>.md`.

## 2026-09-06 · 设计-实现一致性审查修复（13 笔提交，P6 前收口）

- **审查**：双探索代理 + 人工复核（代理结论被修正 3 处：merge_with_labels 是 merge 的默认包装非未调用；check 自写日志 D-07 正文已载；default 档溯源 lookup 侧已接线——过度延伸的「default 未接线」撤回）。结论：M2 管线与 script_allow 五件套一致性最好，偏差集中在 P4 收尾承诺、文档滞后与测试基建。
- **批一行为修复**（600033a 热重载 load 事件/27dd6d7 source.layer explicit+script 接线/4e063d2 --config 进 serve + 端点名纳入/284d63f 响应读 5s deadline/0b35b8e env 兜底+项目根统一/e2504a5 用户层脚本链/fe8691d lint write_flags+delegates 消费者/5798317 降级 mode=hook）。
- **批二文档对齐**（22c1a52）：design.md 目标结构/四模式/协议字段/端点构成/stale 首请求边界/.bak 残留清理；更正登记 15（拼接 allow 按折叠值对账）/16（pnpm dlx 注释如实化，延续 D-05 定性：guard.py 非验收标准）；D-04 更正（组合裁决硬编码 deny 优先不随 precedence）；ROADMAP M2.2 勾选更正（全局层 v1 不做，登记 P6 后专项）。
- **批三代码健康**（869b6f8/94ec9aa）：死参数清理、KB Arc 共享、RuleSetError 三类结构化、词汇 Decision::parse 单源、deny(missing_docs) 补 87 处、惊群「恰好一个存活」不变量、sleep→deadline 轮询、墙钟断言删除、TempDir 三次法则收敛。
- **教训（追加型编辑静默丢失）**：93ee1a8 给 .gitignore 追加的 `/.crush-tether/` 实际入库为空行（拼接点出错不报错），运行时目录此后未被忽略——commit.md 六节新增 6.5（追加型编辑提交前 diff 逐行核对）。
- **测试基建教训**：同线程顺序执行 `accept()`→`connect()` 会死锁（accept 阻塞等连接）——本地端点自测连接对必须先 connect 后 accept（Windows 命名管道单实例，二次 connect 无空闲实例会阻塞，每用例一对连接）。
- 用户裁定：默认包是本项目自定策略，不向 guard.py 规则对齐（A1 注释如实化而非补 [pnpm] 别名）。
- Details: `doc/design.md`（更正登记 15/16）、`doc/decisions.md`（D-04 更正）、`.agents/rules/commit.md` 6.5、`tests/{script_chain,service_reload,service_serve}.rs`。

## 2026-09-06 · P4+P5 落地：服务化三里程碑与三 adapter 契约

- **M4.3**（0b36b42 + 4a209ad/942cc00/d6b5c50 溯源管道）：JSONL 裁决日志落盘 `<project>/.crush-tether/decisions.jsonl`（默认开，ADR-07；写入失败静默）；`source.layer` 全层溯源 = merge 层 Provenance 映射（词条→生效层，Set 覆盖清表/Delta 增删随层）+ lookup `EntrySource`（entry 形态 `<bin>.<bucket>.<dim>`/`<bucket>`/`default`）；`type:"load"` 事件行冷/热路径留痕含 lint 告警；`ts` 用 UTC RFC3339（std 无本地时区，不引依赖，Hinnant 算法自实现）。
- **M5.1–M5.3**：ClaudeCode 契约补全（confirm → `permissionDecision:"ask"` 信封、stdin `cwd` 权限基准优先）；契约测试参数化共用例集驱动三 adapter；zcode adapter 薄变体（`ZCODE_PROJECT_DIR`→`CLAUDE_PROJECT_DIR`→stdin `cwd` 容差链、同构信封）。**M5.3 实机 hook 触发验证挂部署时探针**（实测需写 `~/.zcode` 配置，项目外写入需用户授权）。
- **教训**：Windows 下测试构造 JSON 载荷须用 serde_json 序列化路径——`display()` 反斜杠产生非法 `\U` 转义导致 stdin 解析静默失败（fail-safe 路径生效，表面症状是「裁决丢失」）。
- 已知 flake：`config::seed::tests::concurrent_seeding_converges_to_same_bytes`（Windows 并发 rename 竞态，单独跑稳定通过，M2.6 遗留）。
- Details: `src/{channel,service}.rs`、`src/config/merge.rs`（Provenance）、`tests/{contract_adapters,service_log}.rs`、design.md 更正登记 14。

## 2026-09-06 · M4.1+M4.2 落地：命名端点 serve / hook connect-or-spawn / 热重载

- **M4.1**（c127205）：`src/service.rs`——端点名 `hash(canonical(project), engine)`（DefaultHasher 定种）；独占 bind 单实例裁定（interprocess 默认 `FILE_FLAG_FIRST_PIPE_INSTANCE`，输者静默退出 0）；JSON 行协议 `{id,op,command}`→`{id,verdict}`，一连接一请求；`RuleSet` 装配（查表+脚本+定稿点，check/hook/serve 三模式共用）；`hook` 模式 connect-or-spawn（~200ms 有界重试）+ 降级；watchdog 整秒醒一次 idle 退出（`--idle-exit`，默认 30s）；`benchmark` 双跑对比。验收 5/5（`tests/service_serve.rs`）。
- **M4.2**：notify 监听项目/用户配置目录 + 600ms debounce → watcher 线程只发重载信号，serve 主线程在请求间隙整段重编译 + 整体替换（串行设计无在途并发，天然无半更新）；重载失败保留旧快照 + 告警；监听失效降级逐请求 stat（mtime+size+内容 hash 三重指纹）。验收 2/2（`tests/service_reload.rs`：改规则即生效 / 坏文件保旧快照 / debounce 聚合）。
- **教训 1**：rhai `Engine` 非 Send（Rc 内部）——`Arc<RuleSet>` 跨线程方案不可行；v1 串行下裁决留主线程，watcher 只发信号。并发版升级点：启用 rhai `sync` feature + `Arc<RwLock<Arc<RuleSet>>>`（登记为开闭落点）。
- **教训 2**：rhai 优化器把 `return f(x)` 折叠为语句级 `Stmt::FnCall`（不作为 Expr 被 walk 访问）、把常量拼接折叠为字面量——AST 静态分析必须按优化后的 AST 形态设计。
- 测试钩子：`CRUSH_TETHER_IDLE_EXIT`（spawn 的 serve 空闲秒数）、`CRUSH_TETHER_DISABLE_SERVE=1`（强制降级路径）。
- Details: `src/service.rs`、`src/main.rs`（四模式）、`tests/service_{serve,reload}.rs`、design.md「serve 模式协议」「配置加载与热重载」。

## 2026-09-06 · M4.0 落地：script_allow 全链路（声明文法 + 五件套 + lint + 词汇约定）

- 五笔提交：4555cc1（默认包缺口，前置）→ f466b25（decision:: 四常量含 PASS + ctx.sub 空串约定）→ 64ef27b（声明文法双形态 + 声明集合并，两表皆现 global 胜）→ f8496d0（allow("bin") 原语 + 五件套 + finalize 定稿点）→ b88f70d（lint 三条）。端到端 `tests/script_allow.rs`：local 逃逸→confirm / global 逃逸→allow / 拒载 fail-safe / deny 终审，全绿。
- **实现教训 1**：`return allow("x")` 会被 rhai 优化器折叠为语句级 `Stmt::FnCall`，该形态不再作为 Expr 节点被 `AST::walk` 访问——提取集必须对 `ASTNode::Stmt(Stmt::FnCall)` 单独布点（探针实证，否则字面量提取静默漏报）。
- **实现教训 2**：字符串拼接实参（`"c"+"url"`）被常量折叠为字面量后进入提取集——静态提取与运行值恒一致（审计面不缩小），「拼接 → 拒载」的原始直觉按折叠后语义执行。
- **实现教训 3**：字面量提取启用 rhai `internals` feature（`AST::walk` 含函数体；`ScriptFuncPayload` 未在根导出，手写遍历器无法覆盖函数体）。rhai 锁版钉死，升级须回归（机制 3 运行时双保险兜底）。
- **已知边界**：定稿点逃逸检查与查表层同原语（命令参数词元），重定向目标不在词元内——脚本激活 + 重定向逃逸目标的组合由脚本侧激活条件把关（正当用例即「写重定向到仓库内放行」）。
- Details: `src/script.rs`（finalize/extract）、`src/config/{schema,merge}.rs`、`src/lint.rs`、`tests/script_allow.rs`、design.md 更正登记 11。

## 2026-09-06 · 默认包缺口补齐（M3.3 挂账消解，M4.0 前置 docs 小步）

- 知识库 `[git]` 补 `sub.remote`/`sub.tag` 的 `write_tokens`（忠实平移 guard.py `GIT_ACTION`；裸创建 `git tag <名>` 原工具同样不算写，故不引入 write_arg_count）；`[local]` 补 deny 裸列表四族（sudo/dd/shutdown/mkfs.*——guard.py `DESTRUCTIVE` 收窄，`rm` 保留 confirm 档，reboot/halt/parted 留项目自补）。用户拍板推荐值。
- design.md 更正登记 13 + 两处示例修订；模板逐字节同步（钉死测试过）；回归用例补 9 条断言、变更记录两行标记消解（allow 45 / confirm 30 / deny 14）。全门禁过。
- Details: `doc/design.md` 更正登记 13、`src/config/templates/*`、`tests/guard_regression.rs`、`tests/config_design_example.rs`。

## 2026-09-06 · zcode adapter 并入 P5（M5.3 登记）

- 评估结论：zcode hook 协议与 ClaudeCode 高度同构（`${CLAUDE_PROJECT_DIR}` 双别名、`PreToolUse` 三值决策 allow/ask/deny 与三档一一映射、exit 0/2 一致、`type:"process"` 免 shell），M5.1 信封可薄变体复用。用户拍板并入 M5.3。
- 两处未文档化事实登记为 M5.3 实现期探针（不预设）：stdin 输入载荷键名、`PermissionRequest` 能否返回三值决策。交付形态取插件分发（配置 hooks 默认禁用，插件 hooks.json 自动启用）。
- Details: `doc/design.md`「Agent 适配层」、`cairn/ROADMAP.md` P5/M5.3 与 Settled「Agent 首发」。

## 2026-09-06 · script_allow 设计定稿 + M4.0 登记 + 挂账执行

- **script_allow（脚本条件放行）设计定稿**（design.md 新节）：注册式 + 声明对账——声明双形态（顶级列表 / 命令节键）、引擎五件套（字面量提取 / 差集拒载 / 运行时双保险 / 定稿点作用域化逃逸检查 / deny 终审）、lint 三条新规则。allow 契约由「绝对禁止」演进为「放行面 = 用户声明集 ∩ 脚本条件命中」（更正登记 11）；实现登记 **M4.0 独立里程碑，启动需用户授权**。
- **筛查管线重画**（更正登记 12）：双阶段图显式钉死执行顺序与三安全性质（定稿点唯一 / 逃逸检查挂定稿点 / deny 终审）；Rule trait 标记被 decide_with + RuleEngine 替代。
- **挂账清偿**：脚本词汇约定（decision:: 四常量含 PASS、ctx 空串约定）定稿随 M4.0 前置小改落地；ctx 封装 + 决策枚举化挂 P6 与 Lua 同批；编辑器支持登记 P6 后候选（taplo schema / script-stubs / SchemaStore）。
- 工具链：check-links 的 github_slug 修复下划线处理（GitHub slugger 保留 `_`，原实现删除导致含 `_` 标题无法被引用）。锚点 50/50 过。commits 8011be5 + 本提交。
- Details: `doc/design.md`「脚本条件放行」「筛查管线与编译期组装」、`cairn/ROADMAP.md` M4.0/P6 条。

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
