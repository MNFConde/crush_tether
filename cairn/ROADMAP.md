# crush_tether Roadmap

**Current focus**: P0+P1 已落地（Rust 重写 + 回归用例全绿）；配置格式草案 v1 + 命令知识库框架已纸面定稿（决策论证 `doc/decisions.md` D-01~D-06）。P2–P6 已细化为逐项带验收标准的里程碑（2026-09-06，见「推进计划」M 编号条目）：**P2→P5 共 15 项可一口气连续推进、无外部决策点**（三个实现期定点见「推进节奏」节）；P6 的 mdor 退役需用户确认。**下一步 = P2（M2.1 起），启动待用户授权。** → **执行中（2026-09-06 用户下令开启完全访问模式）：M2.1–M2.7（P2 全部）已完成并收口（2026-09-06），P3（M3.1–M3.3）已全部完成——**本次授权范围（P2 开工 → P3 收尾）执行完毕（2026-09-06）**；下一步 P4（serve 生命周期）待用户新授权。** → **2026-09-06 追加：script_allow（脚本条件放行）设计定稿并登记 M4.0 独立里程碑（启动需用户授权）；挂账执行完毕——脚本词汇约定修订（M4.0 前置小改）、ctx 封装与决策枚举化（P6 同批）、编辑器支持（P6 后候选）均已显式落点。** → **2026-09-06 追加：zcode adapter 并入 P5（新增 M5.3——复用 ClaudeCode 信封薄变体 + 插件分发；stdin 键名与 PermissionRequest 决策能力两处实现期探针实测，不预设）。** → **2026-09-06 用户预授权（待「开始」+ 完全访问模式后执行）：范围 = M4.0 + P4 + P5 一段推进（B 方案），顺序 = 默认包缺口 docs 小步 → M4.0 → P4（M4.1–M4.3）→ P5（M5.1–M5.3），M4.0 先于 serve 热重载；实现期小点就地定 + 登记，不打断。默认包缺口照推荐值执行：git remote/tag 写形态补 knowledge.toml `write_tokens` 落 confirm、sudo/mkfs/dd/shutdown 落 deny 桶——先改 design.md 定稿示例再动模板，同步更新回归用例变更记录（D-05）。P4 日志默认开（M4.3 就地登记 ADR）。** → **2026-09-06 执行中：M4.0 已完成收口（script_allow 全链路落地，见 M4.0 条）；下一步 P4（M4.1–M4.3）→ P5（M5.1–M5.3）按预授权顺序推进。** → **2026-09-06：P4 已收口（M4.1 c127205 / M4.2 8999ea8 / M4.3 0b36b42，ADR-07 日志默认开）；P5 已收口（M5.1/M5.2/M5.3 实现完成，M5.3 实机 hook 触发验证挂部署时探针——需项目外写入授权）；P2–P5 全部达成，仅余 P6（M6.1 Lua/M6.2 质量收口/M6.3 mdor 退役需用户确认）。**

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
- [ ] **P5 Adapter 完整化：ClaudeCode + zcode**（M5.1–M5.3；zcode 于 2026-09-06 并入）：
  - **M5.1 契约适配**✅（2026-09-06，本提交）：`hookSpecificOutput` 信封（permissionDecision allow/ask/deny）；输入键名与 `CLAUDE_PROJECT_DIR` 适配；权限基准 cwd 优先、回退 env；`updated_input` 全替换语义（区别于 Crush 浅合并）。
    - 验收：三档行为与 Crush 等价；exit 2 覆盖 JSON 的规则正确。
  - **M5.2 双 adapter 共用用例集**✅（2026-09-06，本提交）：契约测试参数化（`tests/contract_adapters.rs`），同一用例集驱动 Crush / ClaudeCode / zcode 三 adapter（M5.3 并入后扩为三）。
    - 验收：共用用例集多跑全绿。
  - **M5.3 zcode adapter**✅ 实现完成（2026-09-06，本提交；**实机 hook 触发验证挂部署时探针**——实测需向 `~/.zcode/cli/config.json` 或插件目录写入 hook 配置，属项目外写入，需用户授权或用户自行配置后验证；验收项「插件分发实际触发」随之挂起）：复用 M5.1 的 `hookSpecificOutput` 信封做薄变体（输入键容差 + `${ZCODE_PROJECT_DIR}`/`${CLAUDE_PROJECT_DIR}` 项目目录回退链）；实现期先探针实测两处 zcode 文档未写死的事实再定型——stdin 输入载荷键名、`PermissionRequest` 能否返回三值决策（能则改挂它，否则用已验证的 `PreToolUse`）；交付形态 = zcode 插件（`hooks/hooks.json`，自动启用 hook runner），执行入口用 `type:"process"` 参数向量（Windows 免 shell 转义）。
    - 验收：三档行为与 Crush 等价；M5.2 共用用例集纳入第三 adapter 全绿；插件分发在 zcode 侧实际触发 hook 生效（含配置默认禁用的启用路径验证）。
- [ ] **P6 收尾**（M6.1–M6.3；**M6.3 含用户确认点，不与 P2–P5 连续推进**）：
  - **M6.1 Lua 引擎**：mlua `--engine lua`；**同批挂账（2026-09-06）：ctx 彻底封装**（自定义类型 + 方法化访问，不向脚本暴露裸 map/unit）与**决策值枚举化**（返回值类型收窄四变体、非法值构造期报错）——三者动的都是 `RuleEngine` 接口层，一次定型。
    - 验收：与 Rhai 同一 RuleEngine trait；默认规则 lua 版行为等价；限流同等；ctx 封装后现有词汇约定（`decision::PASS` / 可选字段空串）在 Lua 侧映射 nil 等价成立。
  - **M6.2 质量收口 + 文档**：`cargo audit` 干净；README + 使用文档。
    - 验收：audit 零告警；README 覆盖安装 / 配置 / 三运行模式。
  - **M6.3 mdor 侧退役**（Open Questions 1，待用户确认节奏）：删 crush-guard 目录、回滚 `[project.scripts]`。
    - 验收：mdor 侧无 crush-guard 残留；本仓库为唯一实现。
  - （**P6 后体验专项候选**，2026-09-06 登记）**编辑器支持**：taplo JSON schema（rules.toml 全键含 script_allow）/ `crush-tether script-stubs` 生成补全桩（EmmyLua 注解喂 lua-language-server；Rhai 走工作区文件索引）/ SchemaStore 发布。纯开发期工具，不进运行时路径，不触碰零内置策略。

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

1. mdor 侧退役节奏：crush-guard 目录与 `[project.scripts]` 何时删除（本仓库已是唯一实现，待用户确认后执行）。

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
