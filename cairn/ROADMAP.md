# crush_tether Roadmap

**Current focus**: P0+P1 已落地（Rust 重写 + 回归用例全绿）；配置格式草案 v1 + 命令知识库框架已纸面定稿（决策论证 `doc/decisions.md` D-01~D-06）。P2–P6 已细化为逐项带验收标准的里程碑（2026-09-06，见「推进计划」M 编号条目）：**P2→P5 共 15 项可一口气连续推进、无外部决策点**（三个实现期定点见「推进节奏」节）；P6 的 mdor 退役需用户确认。**下一步 = P2（M2.1 起），启动待用户授权。** → **执行中（2026-09-06 用户下令开启完全访问模式）：M2.1–M2.7（P2 全部）已完成并收口（2026-09-06），P3（M3.1–M3.3）已全部完成——**本次授权范围（P2 开工 → P3 收尾）执行完毕（2026-09-06）**；下一步 P4（serve 生命周期）待用户新授权。** → **2026-09-06 追加：script_allow（脚本条件放行）设计定稿并登记 M4.0 独立里程碑（启动需用户授权）；挂账执行完毕——脚本词汇约定修订（M4.0 前置小改）、ctx 封装与决策枚举化（P6 同批）、编辑器支持（P6 后候选）均已显式落点。** → **2026-09-06 追加：zcode adapter 并入 P5（新增 M5.3——复用 ClaudeCode 信封薄变体 + 插件分发；stdin 键名与 PermissionRequest 决策能力两处实现期探针实测，不预设）。** → **2026-09-06 用户预授权（待「开始」+ 完全访问模式后执行）：范围 = M4.0 + P4 + P5 一段推进（B 方案），顺序 = 默认包缺口 docs 小步 → M4.0 → P4（M4.1–M4.3）→ P5（M5.1–M5.3），M4.0 先于 serve 热重载；实现期小点就地定 + 登记，不打断。默认包缺口照推荐值执行：git remote/tag 写形态补 knowledge.toml `write_tokens` 落 confirm、sudo/mkfs/dd/shutdown 落 deny 桶——先改 design.md 定稿示例再动模板，同步更新回归用例变更记录（D-05）。P4 日志默认开（M4.3 就地登记 ADR）。** → **2026-09-06 执行中：M4.0 已完成收口（script_allow 全链路落地，见 M4.0 条）；下一步 P4（M4.1–M4.3）→ P5（M5.1–M5.3）按预授权顺序推进。** → **2026-09-06：P4 已收口（M4.1 c127205 / M4.2 8999ea8 / M4.3 0b36b42，ADR-07 日志默认开）；P5 已收口（M5.1/M5.2/M5.3 实现完成，M5.3 实机 hook 触发验证挂部署时探针——需项目外写入授权）；P2–P5 全部达成，仅余 P6（M6.1 Lua/M6.2 质量收口/M6.3 mdor 退役需用户确认）。** → **2026-09-06：设计-实现一致性审查修复完成（用户授权，13 笔提交 7958cd5→94ec9aa）——批一行为修复（热重载 load 事件、source.layer explicit/script 接线、--config 进 serve + 端点名、响应读 5s deadline、CRUSH_TOOL_INPUT_COMMAND 兜底、项目根解析统一、用户层脚本链、lint write_flags/delegates 消费者）、批二文档对齐（更正登记 15/16、D-04 更正、M2.2 勾选更正）、批三代码健康（deny(missing_docs)、错误结构化、测试反 flaky）；M6.2 质量收口的部分项已随批三提前完成，P6 里程碑本体未开工。** → **2026-09-06：M6.1 + M6.2 已完成收口（用户授权，b5c3ec8 → 34ebf13 → 文档收尾）——Lua 引擎（mlua）落地，ctx 封装与决策枚举化同批定型（脚本侧属性语法保留，模板零改动），script_allow 五件套 Lua 侧等价，README 落地，audit 警告口径名册化（D-08）；仅余 M6.3（mdor 退役 + M5.3 实机探针，按裁定合并处理，待用户确认节奏）。** → **2026-09-06：M6.3 执行中（用户授权，探针先行 + 退役收尾）——探针首测即抓到 M4.1 Windows 句柄继承洞并修复（943b205：spawn_serve 前清 stdio 句柄继承标志，node 祖先下 hook 31s 挂死 → ~90ms，门禁全绿）；mdor 侧退役完成（13d175e，mdor 仓库）；探针资产就绪待新 zcode 会话激活实测（工作区 hook 配置轨 + 插件目录轨双轨）。** → **2026-09-06：M6.3 全部完成（P0–P6 收官）——新会话实机探针四项观察闭环（stdin 双命名并存 / PermissionRequest JSON 不被采纳故挂点保持 PreToolUse / 用户选择不回传 / hook 错误 fail-open 补齐失效模式 #2）；正式插件 `plugin/` 入库（探针插件形态实测通过——正式版实机验证登记 M7 前置）；探针插件（M6.3 探针期产物，非 M6.2 遗留——2026-09-07 更正标签）待用户禁用，mdor 是否实挂 crush-tether 待用户拍板。** → **2026-09-07：登记 P7 体验与适配专项（M7.0 写目标感知逃逸检查 / M7.1 规则测试工具含用户面 REPL / M7.2 探针 python 化 / M7.3 多 agent 兼容性实测+特性降级矩阵，均待授权）；zcode 原生权限四档与 hook 三值叠加关系实测入档（hook ask 覆盖 yolo、原生「以后都放行」结构性失效 → 权限学习候选动机补强）。** → **2026-09-08：M7 前置完成（用户授权）——正式插件实机验证闭环：三档裁决 / 裸命令名 PATH 解析 / connect-or-spawn（serve 常驻）/ JSONL 裁决日志全链实测通过，失效模式 #2 部署项在开发机闭环；顺带发现探针插件已不在册（M7.2 退役清单收缩）与 zcode 工作区 hook 审核门（M5.3 配置轨未生效真因，design.md 更正登记 21）；M7.2/M7.3 + 可选加固搁置（用户拍板），M7.0/M7.1 待授权（M7.0 的 explain 验证并入 M7.1）。**

## 推进计划（P0–P6）

> 依赖关系：P0 → P1 → P2（最小可用闭环）→ P3/P4 可并行 → P5 → P6。每个阶段有可验收产物，不跨阶段欠账。
> 2026-09-06 细化：P2–P6 拆为 M 编号子里程碑（每项带验收标准），依据草案 v1（design.md「配置格式与脚本边界」）与 `doc/decisions.md` D-01~D-06；推进节奏与实现期定点见「推进节奏」节。

- [x] **P0 骨架**：rust-toolchain.toml 钉 1.97.1；`[lib]`+`[[bin]]` + serde/serde_json/toml/tree-sitter(-bash) 依赖；`src/{model,cmd_parse,engine,channel,config}.rs` 就位。
  - 验收：`cargo clippy -D warnings` 零告警。
- [x] **P1 分类核心（check 模式最小闭环）**：`cmd_parse`（tree-sitter-bash flatten + 写重定向/fd dup/路径逃逸检测）+ 判定表平移 + `Verdict::combine`；`channel` Crush/ClaudeCode 契约输出；`check` 模式（stdin JSON → allow JSON/静默/exit 2）。
  - 验收：`tests/guard_regression.rs` 89 用例全绿（test_guard.py 1:1 平移）；release 单次冷启动 ~9ms（budget <10ms 达标）；冒烟四形态（allow/deny/管道 sink/写 flag confirm）正确。
- [x] **P2 配置声明层 + 知识库 main**（M2.1–M2.7；格式细则见 design.md「配置格式与脚本边界（v1 定稿）」；**已升格定稿（2026-09-06，M2.7 验收后）**）：
  - **M2.1 rules.toml 解析模型**✅（2026-09-06，373df67）：裸键区（`version`/`default`/`precedence`）+ `[local]`/`[global]` 双表 + 命令节三桶 + `sub`/`flag` 子键 + 列表双形态（数组 / inline table）反序列化；`--config`/`CRUSH_TETHER_CONFIG` 显式覆盖入口（加载失败 → stderr 告警 + fail-safe confirm 已接线 check 模式）。
    - 验收：design.md 示例文件整体解析通过；非法键报错可定位；解析失败 → stderr 告警 + fail-safe confirm（不 panic、不误放行）。
  - **M2.2 三层发现与字段级继承合并**✅（2026-09-06，ce5148e；**更正 2026-09-06**：v1 全局层无发现路径——`FoundLayers.global` 恒 None，为后期设计留位，实现/验收实际覆盖用户 → 项目两层 + `CRUSH_PROJECT_DIR`/`CLAUDE_PROJECT_DIR` 优先、缺失逐级上溯）：未定义即继承 / 定义即覆盖；数组 = 覆盖、inline table `add`/`remove` = 增删、标量写值即覆盖；`version` 过旧明确报错，不静默误解析。全局层发现登记 P6 后专项。
    - 验收：覆盖 / 继承 / 增删三类合并单测全绿（含 flag 桶剔除、节内 `default` 继承链）；效力顺序（项目 > 用户 > 全局，不粘性）单测全绿。
  - **M2.3 双表三桶查表 + 多命中合成**✅（2026-09-06，e19252f）：命令节优先、裸列表为语法糖被同层节遮蔽；`[global].allow` 命中整命令豁免；`[local]` allow 带路径逃逸检查；`precedence`（deny > confirm > allow、default 恒链尾）做多命中有序合成。
    - 验收：节 vs 裸列表遮蔽用例；`git show --output=x` 型多维度命中合成 confirm；路径逃逸转 confirm 与 global 豁免用例；复合命令组合裁决不退化。
  - **M2.4 知识库 main + 别名归一**✅（2026-09-06，a616f02）：`knowledge.toml` 解析（10 槽位、`sub`/`flag` 保留结构键，随默认配置生成机制一并落盘）；`alias_of`/`same_flag`/`takes_value` 归一（链式到不动点、加载期防环、`--output=x`/`-o x`/`-oX` 值边界分解）；归一只改名不做语义变换；日志记归一链（`classify_traced` 已备好数据，落盘在 P4）。
    - 验收：`npm exec/x → npx`、`pip3 → pip`、`pnpm dlx → npx` 归一单测；`same_flag` 闭包单边配置双边生效；`a→b→a` 环检测报错；知识库删光后判定不受影响且日志 `kb:[]`。
  - **M2.5 lint 双层**✅（2026-09-06，a3365c4）：结构类（同 token 多桶 / 同 bin 裸列表与节并存 / precedence 死词条）+ 语义类（allow may_write 建议 / 等价冗余死词条 / same_flag 跨桶冲突 / 未知子命令拼写提示）；只告警不拒绝加载。
    - 验收：每条规则正反用例单测全绿；无知识库时降级纯结构检查不报错；告警进 `type:"load"` 事件行。
  - **M2.6 默认配置生成 v1（项目层）**✅（2026-09-06，57c30d7）：三层皆缺有效配置才在项目 `.crush-tether/` 生成默认 `rules.toml` + `rules.rhai` + `knowledge.toml`；损坏（存在但解析失败）→ 告警 + confirm 兜底、原文件不动；temp+rename 原子幂等；生成动作不经规则链。
    - 验收：触发 / 不触发（任一层有效）、损坏不生成、幂等重生成、并发生成收敛单测全绿；重复生成字节一致。
  - **M2.7 样例仓库端到端 + 草案升格**✅（2026-09-06，db5401a + 升格 docs）：临时样例仓库自定义规则改变裁决全链路；三个「可改回」项按草案推荐值生效展示（`go run` 落 confirm、`git reset` 取 confirm 档、`-h` 保留 confirm.flag）；验收通过后 design.md 草案 v1 升格定稿。
    - 验收：自定义规则（覆盖 / 增删两种写法）改变裁决生效；升格文档动作完成、更正登记同步。
- [x] **P3 脚本层（Rhai 默认）**（M3.1–M3.3；完成后零内置策略迁移收口）——已收口（2026-09-06）：
  - **M3.1 RuleEngine trait + Rhai 接入**✅（2026-09-06，93e38a8）：trait 抽象 + `rhai` 钉版引入；Engine 单例 + AST 缓存；`max_operations` 等限流；不可绕过的安全原语注册；`--engine` 参数。
    - 验收：死循环脚本被限流兜底（有界时间返回 confirm）；沙箱内越权 API 不可达、原语可组合不可绕过。
  - **M3.2 默认 rules.rhai 承载全部条件判断**✅（2026-09-06，8443e23）：四类谓词（两态子命令——数据读知识库 `write_tokens`/`write_arg_count`、`find` 突变、`curl|sh` 参数内容、管道 sink / 写特征升级）；脚本 allow 契约就此定稿（限显式枚举、禁无条件兜底——方向已定，仅钉语法细节）。
    - 验收：四类谓词用例全绿；无条件 allow 兜底被契约拒绝；知识库删光 → 脚本查不到数据 → confirm 兜底。
  - **M3.3 删内置表 + 89 用例迁移**✅（2026-09-06，82cf80e）：删除 engine.rs 内置判定表残留；`tests/guard_regression.rs` 89 用例改「引擎 + 默认规则 fixture」驱动；断言冲突以草案为准更新用例留变更记录（guard.py 是参考对象非验收标准，D-05）。
    - 验收：89 用例全绿；内置表删除后质量门禁全过；变更记录逐条登记。
- [x] **M4.0 脚本条件放行（script_allow）**✅（2026-09-06，64ef27b 声明文法 + f8496d0 引擎五件套 + b88f70d lint 三条 + 本提交端到端；设计定稿见 design.md「脚本条件放行（script_allow，定稿）」与更正登记 11）：
  - 内容：声明文法双形态（顶级列表 `script_allow = [...]` + 命令节键 `script_allow = true`，D-02 跨层合并）；脚本 `allow("bin")` 原语（裸 `"allow"` 字符串仍违约）；引擎五件套（加载期字面量提取 / 声明集差集拒载 / 运行时对账双保险 / 定稿点作用域化逃逸检查 / deny 终审拦截）；lint 三条新规则（死声明 / may_write 建议 / deny 冲突提示）；脚本词汇约定修订随其前置小改落地（`decision::` 只读常量四值含 PASS、ctx 可选字段空串约定——design.md「脚本层职责边界」词汇约定条）。
  - 验收：a（local 声明）逃逸 → confirm、b（global 声明）逃逸 → allow、未声明 bin 拒载、动态名拒载、deny 之上激活无效、lint 三条正反用例全绿、decision:: 常量与空串约定单测全绿、全门禁过——全部达成（端到端 `tests/script_allow.rs` + 单测）。实现注记：字面量提取用 rhai `internals` feature 的 `AST::walk`（含函数体；rhai 锁版钉死，升级须回归）；`return allow(...)` 被优化器折叠为语句级 `Stmt::FnCall`，提取集对其单独布点；字符串拼接被常量折叠为字面量后按折叠值对账（静态提取与运行值恒一致）；定稿点逃逸检查与查表层同原语（命令参数词元），重定向目标不在词元内（已知边界，LOG 登记）。
- [x] **P4 常驻服务 + 热重载**（M4.1–M4.3；多 bucket 管理与配置编写提示为后置专项，不阻塞主线）——已收口（2026-09-06，c127205 + 8999ea8 + 本提交）：
  - **M4.1 命名端点 serve + hook connect-or-spawn**✅（2026-09-06，c127205）：端点名 hash(项目根, engine)；独占 bind 单实例裁定（输者静默转 connect）；spawn + ~200ms 有界等就绪重试 → 仍失败降级本进程 check；`--idle-exit`（默认 30s）；v1 串行 accept + per-request deadline；端点 ACL 限当前用户。
    - 验收：并发冷启动惊群收敛单实例；连接归零 idle 退出；降级路径仍出裁决绝不放行；`--benchmark` 双跑 diff 为空。
  - **M4.2 热重载**✅（2026-09-06，8999ea8）：notify + 600ms debounce；三层整段重编译 + `Arc<RuleSet>` 原子换指针；编译失败保留旧快照 + stderr 告警；监听失效降级 stat（mtime+size+hash 三重校验）。
    - 验收：改规则文件不重启即生效（端到端）；坏文件期间新旧请求分别用新旧快照、无半更新；降级路径正确性不损。
  - **M4.3 裁决日志落盘 + 资源预算达标**✅（2026-09-06，本提交）：JSONL 字段全集（含 `kb`/`normalized`/`script`）；serve 单点写 + hook 降级自写；`type:"load"` 事件行含 lint 告警；日志默认开关就此定并登记 ADR（建议默认开——P4 内唯一实现期定点）。
    - 验收：日志字段与 design.md 示例一致；load 事件冷热路径都留痕；常驻 <10MB、P95 <5ms、零 busy-loop，CI benchmark 门槛防退化。
- [x] **P5 Adapter 完整化：ClaudeCode + zcode**（M5.1–M5.3；zcode 于 2026-09-06 并入；**全部完成 2026-09-06**）：
  - **M5.1 契约适配**✅（2026-09-06，本提交）：`hookSpecificOutput` 信封（permissionDecision allow/ask/deny）；输入键名与 `CLAUDE_PROJECT_DIR` 适配；权限基准 cwd 优先、回退 env；`updated_input` 全替换语义（区别于 Crush 浅合并）。
    - 验收：三档行为与 Crush 等价；exit 2 覆盖 JSON 的规则正确。
  - **M5.2 双 adapter 共用用例集**✅（2026-09-06，本提交）：契约测试参数化（`tests/contract_adapters.rs`），同一用例集驱动 Crush / ClaudeCode / zcode 三 adapter（M5.3 并入后扩为三）。
    - 验收：共用用例集多跑全绿。
  - **M5.3 zcode adapter**✅ 实现完成（2026-09-06）+ **实机探针闭环（2026-09-06，M6.3 批执行）**：复用 M5.1 的 `hookSpecificOutput` 信封做薄变体（输入键容差 + `${ZCODE_PROJECT_DIR}`/`${CLAUDE_PROJECT_DIR}` 项目目录回退链）；**探针四项全部有结论并回填 design.md**——① stdin 载荷 = ClaudeCode 蛇形键与 zcode 驼峰键双命名并存（adapter 按蛇形键零改动可用）；② `PermissionRequest` JSON 信封不被采纳（exit 2 可否决、用户最终选择不回传）→ **挂点保持 `PreToolUse`**（其三值 JSON 全部实测生效，不改代码）；③ 顺带观察项 = 用户选择不回传（PostToolUse 只有执行结果），「权限学习」候选的保守路线（suggest）由此更稳；④ hook 进程错误（非 2）→ agent 侧 **fail-open**，失效模式表 #2 补齐（部署必查二进制可达）。**探针首测另抓到 M4.1 Windows 句柄继承洞并修复（943b205，更正登记 20）**；交付形态 = 插件分发（本地目录 marketplace 安装 + hook 实际触发全链实测通过，正式插件 `plugin/` 入库）；配置文件 hook 轨实测未生效（`process` 型混入 `statusMessage` 疑因，交付不依赖，登记为坑；**更正 2026-09-08：真因 = zcode 工作区 hook 审核门，statusMessage 疑因作废，见 design.md 更正登记 21 与 M7 前置**）。
    - 验收：三档行为与 Crush 等价 ✅（契约测试）；M5.2 共用用例集纳入第三 adapter 全绿 ✅；插件分发在 zcode 侧实际触发 hook 生效 ✅（含插件自动启用 hook runner 路径——工作区配置轨未生效而插件轨生效，反证自动启用成立；**实测对象为探针插件形态，正式插件实机验证 = M7 前置**）。
- [x] **P6 收尾**（M6.1/M6.2/M6.3 ✅ 2026-09-06，P0–P6 收官）：
  - **M6.1 Lua 引擎**✅（2026-09-06，b5c3ec8 接口层定型 / 3d9b7bc Lua 引擎与引擎感知装配 / 34ebf13 script_allow Lua 侧）：mlua 0.12（lua54+vendored）实现同一 RuleEngine trait；**同批挂账兑现**——ctx 彻底封装（ScriptCtx 自定义类型，字段只读 getter 保持 `ctx.bin` 属性语法，模板零改动）与决策值枚举化（ScriptDecision 四变体构造封闭，裸字符串返回边界统一解析双保险）。
    - 验收：与 Rhai 同一 RuleEngine trait（Box<dyn>）✅；默认规则 lua 版行为等价（双引擎同用例集对账测试，载体 = `src/config/seed.rs` 内联 `default_lua_matches_rhai_predicate_semantics` 双跑对账 + nil 等价单测；`tests/script_lua.rs` 为引擎行为面）✅；限流同等（**2026-09-06 审查修复后成立**：全局指令数 hook 覆盖协程 + 内存上限 vs rhai max_operations，更正登记 18——初版 set_hook 有协程逃逸洞）✅；ctx 封装后词汇约定 Lua 侧 nil 等价成立（nil→Pass 单测）✅。实现注记：mlua 0.12 无 sandbox feature——沙箱改由 StdLib 白名单 + new_with 安全模式 + base 危险全局消毒实现；Function 不保活 Lua state（实例字段锚定）；script_allow 机制 1 Lua 侧为注释剥离后保守词法扫描（design.md 更正登记 17）；脚本文件按引擎选择 rules.rhai/rules.lua，本层缺失但有他引擎脚本文件时 stderr 告警。
  - **M6.2 质量收口 + 文档**✅（2026-09-06）：`cargo audit` 无漏洞；README 落地（安装/配置/四运行模式/agent 接入/安全模型）。
    - 验收：audit 零告警 → **口径修正（D-08，用户裁定）**：rhai 传递依赖 smartstring unmaintained（RUSTSEC-2026-0249，无 CVE）接受并名册化，监控点 = rhai 发布移除该依赖即升级。
  - **M6.3 mdor 侧退役**（Open Questions 1）✅ **2026-09-06 全部完成**：mdor 侧（mdor 提交 13d175e）——删 crush-guard 目录（含 egg-info/pycache 与 uv.lock 脏改）、`uv tool uninstall` 解除全局安装、`[project.scripts]` 核实已回滚、`.crushrc` 摘除指向已删 guard.py 的**死 hook**（失效模式 #4 实例；bash 回退 Crush 原生确认，留 crush_tether 重挂指引；是否实挂属新部署决策待用户拍板）、cairn 两篇主题文档 archived + 指针。M5.3 zcode 实机探针按裁定挪至本项一并处理，随 M5.3 条闭环（探针四项结论 + M4.1 修复 943b205 + 正式插件 `plugin/` 定稿入库）。
    - 验收：mdor 侧无 crush-guard 残留 ✅；本仓库为唯一实现 ✅；实机探针闭环 ✅。
  - （**P6 后体验专项候选**，2026-09-06 登记）**编辑器支持**：taplo JSON schema（rules.toml 全键含 script_allow）/ `crush-tether script-stubs` 生成补全桩（EmmyLua 注解喂 lua-language-server；Rhai 走工作区文件索引）/ SchemaStore 发布。纯开发期工具，不进运行时路径，不触碰零内置策略。
  - （**P6 后候选**，2026-09-06 讨论，未立项）**权限学习**：裁决日志（M4.3）+ PostToolUse 执行记录交叉推断「用户批准过的 confirm 命令」→ 保守路线先行（`suggest` 命令离线生成规则建议、人确认后写入）；**前置探针已完成（M5.3 实测）**：用户最终选择不回传任何 hook、PostToolUse 载荷只有执行结果。**动机补强（2026-09-07 实装反馈）**：hook ask 无状态 + 原生「以后都放行」被 hook 评估覆盖 → 用户每条 confirm 类命令都被重新询问，权限学习是该痛点的正解。安全红线：deny 永不参与学习；落盘条目收窄到最小作用域（bin+sub）；带「来源=学习」标记可审计可回滚。事实底座：`PermissionRequest` 事件为 zcode 独有（ClaudeCode 无同语义事件、Crush 未记录），跨 agent 一致的信号源 = PostToolUse。
  - （**P6 后候选**，2026-09-06 登记）**新 agent 接入对照表**：接入任何新 agent（OpenCode / Codex 等）时，逐行对照 design.md「Hook 接入失效模式与保障边界」表走验收清单（失效模式 × 三层兜底责任 × 验证方法），不重新推导。

- [ ] **P7 体验与适配专项**（2026-09-07 登记；各条目均待用户授权逐项启动，登记不等于开工；编号项 = M7.0–M7.3 四项，另有 M7 前置与可选加固各一。**2026-09-08 更新：M7 前置 ✅ 完成；M7.2/M7.3 + 可选加固搁置（用户拍板）；M7.0/M7.1 待授权**）：
  - **M7 前置：正式插件实机验证**（2026-09-07 会话审查登记，P7 开工后先做；在此之前正式插件不得视为已验证）：前置 = `crush-tether` 进 PATH（登记时不在 PATH——`cargo install --path .` 或入 shims 目录；不装 PATH 直接装插件会落进失效模式 #2 的 fail-open 放行）；然后本地 marketplace 安装正式 `plugin/`，实测 hook 触发与三档行为。范围澄清：M5.3 实测通过的是**探针插件形态**（node wrapper + 二进制绝对路径），正式版 `type:"process"` + PATH 解析链路未跑过。**2026-09-08 更新**：二进制可达路线定稿 = `cargo install --path .`（用户确认开发测试推荐，已实机安装并验证 PATH 解析 + check 裁决，README 构建节已注明；实装落点 = scoop persist rustup `.cargo\bin`，rustup 升级不丢）；插件分发形态分析（三形态取舍/平台坑本机实测/marketplace schema 实查/装载守卫三轴模型/分发两期分解建议）登记于 design.md「插件分发形态与装载守卫」+ `cairn/plugin-distribution-analysis.md`，scoop/Release 管线与捆绑等分发路线后置正式分发期拍板。**✅ 2026-09-08 完成（用户授权）**：实装链 = UI Discover `+` 加本地目录 marketplace（`known_marketplaces.json` directory 型注册）→ Get 安装（`installed_plugins.json` + 缓存拷贝）→ `zcode plugins enable`（CLI `plugins` 子命令仅 list/enable、无 install/marketplace 能力；其间 `enabledPlugins` 曾显式 `false` 且与 `plugins list` 渲染的 enabled 不一致，CLI enable 后归 `true`）→ 重启恢复会话 hook 即生效。三档实测：`cat`/`tail` 单命令 allow 无弹窗；`curl --version` confirm 弹窗 → 用户批准 → 执行；`sudo --version` deny 工具调用直接阻断（`sudo blocked (deny list)`）；复合命令（`echo && powershell`）按多命中合成落 confirm，语义正确。链路点：裸命令名 PATH 解析 ✅、serve 常驻进程 + 裁决全走 `mode:"serve"` ✅、JSONL 含 `type:"load"` 事件（lint 告警留痕）✅。顺带发现：①探针插件已不在册（退役清单收缩，见 M7.2）；②zcode 工作区 hook 审核门（design.md 更正登记 21）。
  - **M7.0 写目标感知逃逸检查**（用户策略：「读取默认都通过、写入默认只能本仓库内」）：现状 = `[local]` allow 逃逸检查对**任意参数词**生效（lookup.rs 两处 + `path_escapes`），读项目外文件也被翻 confirm、且 `[global]` 豁免是整体的（读外写内与读内写外无法区分表达）；升级 = 逃逸检查只作用于**写效果路径**（重定向目标、knowledge `write_tokens` 写参数位、写 flag 值——flag 型写已被 confirm.flag 桶覆盖），读源路径豁免；design.md `[local]`「带逃逸检查」承诺语义随之精化（更正登记）；配套 = cp/mv 等双位置写命令的 knowledge 条目。验收：读外纯读 / 读外写内 / 读内写外 / flag 写四形态用例 + M7.1 `explain` 验证。
  - **M7.1 规则测试工具**：命令式 = `explain '<cmd>'`（人读单发：裁决 + 命中层级/桶/token/kb/归一/脚本 全溯源）+ `check --batch`（一行一命令 → 裁决表）；断言式 = 规则用例文件（输入 + 期望档位）批量对账；**REPL = 用户面调试器**（读配置快照逐条调试自己的规则配置与脚本，即时显示命中与未命中原因——定位是放开给用户自助调试，非内部工具）。基建复用：trace/裁决日志（M4.3）+ 热重载（改规则即测，免重启）。
  - **M7.2 探针工具 python 化入库**（**搁置，2026-09-08 用户拍板**）：`script/hook_probe.py`（经 `uv run python` 调用，本机 python 由 uv 管理无全局环境；路径参数化换项目可复用；控制文件切模式 perm-out/perm-exit/fail-exit 不改代码换实验；零第三方依赖）；design.md「hook 探针方法（定稿）」为语言无关方法论，python 版为参考实现；node 版 `.zcode/probe/` 维持 gitignore 临时件随探针插件退役。验收：本仓库自测 + 外部项目实测 hook 触发各一次。**退役清单（M7.2 落地时或用户提前禁用时执行；2026-09-07 审查项 6/7——config.json 残留与探针三副本漂移——均在此清单内消解，不单独处理；全部为未入库临时件，持久文档（design.md 探针方法节/LOG/ROADMAP 记录）不在退役范围。2026-09-08 收缩：探针插件已不在册——`installed_plugins.json` 空、`enabledPlugins` 无条目、`known_marketplaces.json` 无注册，原第一项「禁用/卸载探针插件」免做；余项 = 删 `marketplaces/crush-tether-probe-marketplace/` 数据目录与 `cache/` 空壳 + 删工作区 `.zcode/probe/`（含 dump.jsonl——删前用户过目；三副本版本漂移不修、随删消灭）+ 删 `.zcode/config.json`（`enabled:true` 残留；审核门发现后此项防双 hook 的意义上升——配置轨经批准即活，见 M7 前置）**：~~禁用/卸载探针插件~~（2026-09-08 已不在册）~~+ 清 `~/.zcode/cli/plugins/cache/crush-tether-probe-marketplace/`~~（收缩并入上句）；删工作区 `.zcode/probe/`（含 dump.jsonl——删前用户过目；三副本版本漂移不修、随删消灭）；删 `.zcode/config.json`（`enabled:true` 残留，防双 hook 叠跑）；~~清用户级 `~/.zcode/cli/config.json` 的 enabledPlugins 与 `known_marketplaces.json` 中探针 marketplace 注册~~（2026-09-08 核实已无残留）。
  - **M7.3 多 agent 实机兼容性实测 + 特性降级矩阵**（**搁置，2026-09-08 用户拍板**；zcode 侧三档实机行为已随 M7 前置闭环，Crush/ClaudeCode 侧待本项解冻）：逐 agent（Crush / ClaudeCode / zcode，后续含新接入者）实测——原生权限档位 × hook 三值交叉行为、超时与 fail-open 语义、信封细节差异（`updated_input` 浅合并 vs 全替换等）、agent 独有事件（zcode `PermissionRequest`）；结果回填 design.md 失效模式表与各 agent 契约节。**特性降级原则**：依赖 agent 特有能力的功能（权限学习依赖选择回传等）在不支持的 agent 上**默认不生效**（能力探测 + 静默关闭），绝不报错——与 fail-safe 哲学同源。与「新 agent 接入对照表」候选互引；顺带逐 agent 评估权限学习/suggest 可行性。
  - （**可选加固，非里程碑验收项**，2026-09-07 审查登记，用户裁定记录前因后果；**搁置，2026-09-08 用户拍板**）**Windows 句柄继承回归测试**：前因 = M4.1 修复（943b205，`spawn_serve` 前清 stdio 句柄 `HANDLE_FLAG_INHERIT`）只有人工验证（node 父进程实测 31s→~90ms），无自动化守护；该 bug 需「父进程持有可继承管道」的特定环境才复现，普通 CI shell 的管道不可继承，恰好是测试盲区（当初 bash 自测漏掉它即此原因）。风险 = 若此段代码被删，全部测试照样全绿、无任何报警，症状只在实际使用中重现（zcode 里 hook 挂死超时）。方案 = CI windows-test job 增加专用用例：以持有可继承句柄的 helper 进程作父，断言 serve 子进程不继承（可行但脆弱，故不设为正式里程碑，按需再议）。

## 推进节奏（2026-09-06 细化时钉死）

- **P2→P5（M2.1–M5.3，16 项；M5.3 为 2026-09-06 追加）可一口气连续推进**，无外部用户决策点；P6 的 M6.3（mdor 退役）需用户确认，不并入。
- 三个实现期定点（方向已定，执行时就地钉死并登记，不构成阻塞）：
  1. `-h` 笔误：实现期确认后剔除并登记（design.md 更正登记第 5 条）；`go run` / `git reset` 档位按草案推荐值执行、随 M2.7 验收展示。
  2. P3 脚本 allow 契约语法细节：显式枚举、禁无条件兜底已定，仅钉表达形式。
  3. P4 日志默认开关：M4.3 内定并登记 ADR（建议默认开）。
- 单人串行节奏：按 M2.1 → M6.2 顺序推进（P3/P4 理论可并行，串行更稳）；每项过质量门禁（fmt → clippy → test → audit），每阶段末 Cairn 登记与提交。
- **执行授权（2026-09-06）**：用户已授权 **P2 开工 → P3 收尾（M2.1–M3.3）**；每里程碑 ≥1 commit、改动大按功能拆分提交；P4+ 不在本次授权内，完成后另行拍板。正式开工待用户开启完全访问模式后下令。
- **外部写入边界（用户裁定 2026-09-06）**：构建工具缓存（`~/.cargo` registry、advisory DB、rustup 工具链下载）**不算**「外部文件修改」；禁令范围 = 其他项目目录（如 mdor）与项目外普通文件。
- **沉淀纪律（用户约定 2026-09-06）**：每次 git 提交前、以及上下文临近压缩时，各执行一次 Cairn 沉淀检查（规则见 cairn/AGENTS.md「知识沉淀规则」），防止压缩丢失应沉淀信息。
- **启动实施待用户下令**——授权范围已定（M2.1–M3.3），agent 不自行启动、不自行扩大范围。

## Milestones（已达成）

- [x] 确认 crush-guard 抽取/重写方向（见 doc/design.md）
- [x] 确认三档分类语义（allow/confirm/deny）
- [x] 落地 doc/design.md 设计文档（含规则引擎 + DSL + Channel 章节）
- [x] 定稿运行模式与热重载方案（命名端点 + Arc 快照热重载，见 design.md「运行模式与配置热重载」）
- [x] Rust 重写 P0+P1（check 模式 + 回归用例 9/9 组全绿 + 质量门禁全过）
- [x] 定稿**零内置策略 + 默认配置生成**（二进制纯引擎；默认策略 = 项目侧生成的外部 `rules.toml` + `rules.rhai`；三层皆缺才生成、任一层有效即尊重；损坏留档后重新生成；全局/用户层生成由命令提供后期设计；效力顺序项目 > 用户 > 全局）
- [x] 纸面定稿**配置格式草案 v1**（2026-09-05：`[local]`/`[global]` 双表 + 每命令 allow/confirm/deny 三桶查表 + 头部裸列表/precedence/default 标量；声明层零条件判断，两态子命令/find 突变/管道 sink 等全部下沉脚本层；token 级 merge；JSONL 裁决日志格式先行；见 design.md「配置格式与脚本边界（v1 定稿）」——已升格定稿（2026-09-06，P2 六项里程碑 + 样例端到端验收全绿））
- [x] 设计评审 + 草案 v1 增补（2026-09-06：**命令知识库框架**（bucket、10 槽位封闭、别名归一参与运行时、属性仅 lint/脚本、删光=不做语义检查）+ **层间合并改字段级继承**（数组覆盖 / inline table `add`/`remove` 增删）+ **单命令建模**完备性标准（槽位跟着消费机制走）+ 损坏重生成收窄 + guard.py 重定位为参考对象；新建 `doc/decisions.md` 轻量 ADR（首批 D-01~D-06）与 `script/` 目录约定（三次法则 + 台账）——见 design.md 草案 v1 增补节、`doc/decisions.md`）
- [x] 细化 P2–P6 推进计划为逐项里程碑（2026-09-06：P2 7 项 / P3 3 项 / P4 3 项 / P5 2 项 / P6 3 项，每项带验收标准；P2→P5 共 15 项一口气可推进、无外部决策点；P6 含 mdor 退役用户确认点；节奏与三个实现期定点见「推进节奏」节）

## Open Questions

1. ~~mdor 侧退役节奏：crush-guard 目录与 `[project.scripts]` 何时删除~~（2026-09-08 勾销：已随 M6.3 完成，mdor 提交 13d175e；mdor 是否实挂 crush-tether 属新部署决策，仍待拍板，见根 AGENTS.md「待用户拍板」）。

## Settled（历轮定稿）

- DSL 引擎：**Rhai**（默认，`--engine rhai`）+ **Lua（mlua）**（`--engine lua`，兼容旧习惯）；Roto 已否决。
- 抽取方式：**纯全局工具**（本仓库唯一实现）；Python 版重写已落地。
- 热重载：`notify` 事件监听 + 600ms debounce，**整段重编译 + `Arc<RuleSet>` 原子换指针**（不增量 patch）；脚本编译失败保留旧快照；监听失效降级 stat 校验。
- 配置拆分：`.crush-tether/rules.toml`（声明层）+ `rules.rhai`/`rules.lua`（脚本层）。
- 配置优先级：项目 > 用户 > 全局（不粘性，`deny` 可被高层覆盖；三层同时存在时效力同序）。
- 规则来源：**零内置策略**——二进制纯引擎（解析/特征/安全原语/管线），不内嵌任何策略数据；默认策略由生成到项目侧的外部 `rules.toml` + `rules.rhai` 提供，二进制内嵌的仅是生成模板（不参与判定）。生成触发 = 三层皆无有效配置；损坏 ≠ 缺失（告警 + fail-safe confirm 兜底，原文件不动）；生成动作不经规则链（引导豁免）；temp+rename 原子幂等；生成前/失败按 fail-safe confirm；全局/用户层默认文件由命令提供（后期设计）。
- 默认包分工：能声明表达的进默认 `rules.toml`；跨参数逻辑（`find` 突变、`git config` 多位置参数等）进默认 `rules.rhai`；89 回归用例迁移为「引擎 + 默认规则 fixture」驱动。
- Agent 首发：**Crush**（一）→ **ClaudeCode**（二）→ **zcode**（三，2026-09-06 并入 P5/M5.3——hook 协议与 ClaudeCode 同构，信封薄变体复用）；OpenCode 延后至稳定，其余留空壳。
- 语言：**Rust**（tree-sitter-bash + 三 DSL 生态）；工具链钉 1.97.1（rust-toolchain.toml）。
- 结构：**单 crate + `src/lib.rs` + `src/main.rs` 双入口**（核心逻辑无第二消费方，故**不拆 workspace** 分 core/cli 两 crate）。
- 运行模式：**hook（默认，connect-or-spawn）+ serve 常驻 + check 单发（兜底/冒烟）**，check 先行落地；生命周期使用驱动（连接归零 + idle 退出），不耦合 agent 进程；【已否决】客户端壳 + bash 进程替换持 fd（Go 子进程 fd 全 CLOEXEC，hook 每次全新 bash）。
- serve 传输：**本机命名端点**（Windows named pipe / Unix socket，优先 abstract namespace），**一项目一 serve**（端点名 hash(项目根, engine)），**独占 bind = 单实例 + 角色裁定**（输者静默转 connect）；连接生命周期 = 一次请求，断开感知靠内核 EOF，无心跳；v1 串行 accept（last_activity 代替计数）。
