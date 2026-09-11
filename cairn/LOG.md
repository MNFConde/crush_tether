# Project Cairn Log

This file records substantive progress in reverse-chronological order — newest entry at the top, right below this line. Keep each entry short — summary and pointer only; conclusions settle into `cairn/<topic>.md`.

## 2026-09-11 · 测试设计独立成档 + 五 job 串联化 + 无头场景组首测(fail-open 形态分化定性)

- **文档重组**:矩阵瘦身只留结果(§3/§4/§5/§6.3 迁出),新建 `doc/test-and-ci.md` 承载测试方法/CI 设计/差异规避/覆盖边界/分类法/挂账/探针设计(design.md 探针节同迁);doc/ 重组按约定快照 archive_doc_v2。
- **CI 升级**:五 job 冒烟**串联化**(claude/crush 探针换 bash 角色转发真引擎,断言升级 decisions.jsonl 落 allow,补契约漂移守护);新增场景组 deny/fail-open/rewrite(探针 perm 直回信封,引擎不在场,物理副作用断言 `ci-exec.txt`;双 mock 实例——冒烟命令须引擎放行[带写实测裁 confirm]、场景命令只求留痕);mock 加 `--cmd`。
- **fail-open 形态分化(核心定性)**:claude 交互放行/无头**拒绝**(`permission_denials` 回执,两次复现);crush/zcode 无头与交互一致放行——「能力×形态」二维教训的又一实例。deny/rewrite 三家无头与交互一致。
- **附带定性**:zcode 无头对 confirm=拒绝(无人批准保守拒);**zcode spawn hook 精简 env**——`uv run` 静默失效(uv 找不到管理 python,spawn 可达性用空参快跑二分定位),hook 须绝对路径解释器;排障中插件装配增量切换+备份复原(user 环境含 9 官方插件,不可整写)。
- Details: `doc/test-and-ci.md`(§1 场景组/人工流程、§2 差异 12-13、§3 覆盖边界+归因树、§4 分类法定稿)、`doc/agent-compat-matrix.md`(§1.2 fail-open 行/§2)、`script/ci_scenario.sh`、`script/mock_llm.py`。


- **spike 先行**：本机清空插件存储 → 机械重建四件套（`known_marketplaces.json` directory 源 + `marketplaces/` 镜像 + `installed_plugins.json` + `cache/<mkt>/<plugin>/<ver>/` 拷贝）+ `enabledPlugins` 置真 → `plugins list` 识别（hooks: 1）→ headless `-p` hook 拉起、`decisions.jsonl` 落 allow——**无 transaction/digest 校验障碍**，无人值守装配路径坐实；测后备份完整复原。
- **workflow**：agent-matrix.yml 增 claude-win / crush-win / zcode-win 三 job（`shell: bash` + `python` 命令名 + curl 探活替换 `/dev/tcp` + `cygpath -m` 原生路径）。zcode 配方 = extras bucket manifest 解析版本（cron latest 哨兵的白送版本发现+hash）→ CDN 直链 NSIS → 7z 两步解包取 `zcode.cjs`（node 22 驱动，不装 App）→ `cargo install --path .` → 无人值守装配 → mock 驱动 `-p` → 断言 decisions.jsonl。
- **首跑 4/5 绿**：zcode/claude-win 一次过；crush-win 挂 `crush.exe` exit 127——zip 资产带版本嵌套目录，find 归位 PATH 根修复（68d1055）；**二跑五 job 全绿**。
- Linux zcode 内测中，公测后补 ubuntu job（windows 配方可平移）；cron latest 哨兵首触待观察（每周一 UTC）。
- Details: `.github/workflows/agent-matrix.yml`（头注释即配方）、`doc/agent-compat-matrix.md`（注6/§2/§3/§5）、`cairn/ROADMAP.md` M7.3。

## 2026-09-10 深夜 · zcode headless 形态证实（更正「无 headless」）+ 插件轨 headless 实证 + 入 CI 卡点查明

- **更正**：矩阵旧结论「zcode 无 headless 形态」不成立——App 内嵌 CLI（`resources/glm/zcode.cjs`，版本轨道 0.16.5）有 `-p/--prompt` 非交互一次性形态；此前结论成因 = 入口不在 PATH、只探了 `zcode` 命令不存在。
- **实证链**：provider 配置（`provider.<id>.options` 端点/密钥 + `model.main` 仅接受 `"provider/model"` 字符串）指向 mock_llm → `-p` 全回合跑通（mock 见工具结果回 spike done）；修 mock SSE `input` 须为对象（zcode AI SDK 严格校验，claude/crush 宽容）；**插件轨 hook 在 headless 拉起**（`decisions.jsonl` 落 allow 裁决），config 轨则被工作区信任门挡死（headless 无 capable host 不可首授，信任按 工作区+声明 digest 持久化）。
- **入 CI 卡点**：非能力而是发行——npm 无官方包（`zcode-app-cli`/`zcode-acp-server` 均第三方），本机仅证 win32-x64 内嵌 bundle；前提清单入矩阵 §5。
- 测试后已复原 `~/.zcode/cli/config.json`（插件停用 + model 段移除），mock 进程已停，tmp 产物已清。
- Details: `doc/agent-compat-matrix.md`（注6/§1.2 headless 行/§2/§3 步骤2/§4 行9-11/§5）、`cairn/agent-hook-testing.md`（更正三）、`cairn/ROADMAP.md`（M7.3）。

## 2026-09-10 · zcode 模式交叉两案定论 + 测试流程固化（M7.3 人工待补清零）

- **确认模式 × hook 三值**：allow **跳过原生弹窗**（touch 变更类实证——hook 评估先于原生权限并预批准）；ask 弹窗，**反证实验钉死人工性**（弹窗点拒绝 → agent 收 Denied）；deny 不弹直接阻断。
- **计划模式 × hook**：hook 照常评估（allow/confirm 两例全中日志）；allow 只读命令放行；计划模式只读分类器**短路 ask**（不弹窗直接拦，用户证实）；第一道门在 agent 层（系统硬约束禁写、先于 hook——「hook allow 写操作 × 计划模式」不可达，如实记录）。
- **流程固化与入档**：zcode 版本入档 3.11.2（Desktop App）；六步人工测试流程固化于矩阵 §3（前置/武装判定/三档/updated_input config 轨/模式交叉/版本记录）；§1.1 zcode 升「通过（3.11.2）」，§1.2 增两行模式交叉，§4 增控制文件差一拍坑，§5 zcode 项全收口。
- Details: `doc/agent-compat-matrix.md`（§1.1/§1.2/§2/§3/§4/§5）、`cairn/ROADMAP.md`（M7.3 剩余项）。

## 2026-09-09 深夜 · zcode `updated_input` 实测定论：Claude 式全替换采纳（待补测再收一项）

- **结论**：zcode 采纳 Claude 式 `hookSpecificOutput.updatedInput`——**全替换语义实证**：探针回包把整条复合命令（含 heredoc 写文件+echo）替换成单条 `echo BBB-REWRITTEN`，控制文件因而连读三拍旧值，恰好成为三次采纳的重复证据；crush 式顶层 `updated_input` 信封不采纳（原样执行）。与 zcode 复用 ClaudeCode 信封（M5.3）一致。
- **方法注记**：控制文件切实验存在「hook 先于命令执行读文件」的天然差一拍——改控制文件须用非 Bash 工具（matcher 只匹配 Bash）；被改写的调用会连带废掉同调用内的写文件操作。此坑已体现于矩阵 §4 判定准则语境。
- **矩阵回填**：§1.1 zcode 格升「通过」；§1.2 行定论；§2 流水加行；§5 收项。config 轨探针测后即退役（`.zcode/config.json`+`hook-probe` 已删）。
- Details: `doc/agent-compat-matrix.md`。

## 2026-09-09 深夜 · 矩阵文档按「事实/过程分离」原则重构（存档 v1）

- **doc/agent-compat-matrix.md 重写**为确定性事实五节：兼容性矩阵（新增 agent × 版本二维覆盖表 + pinned 能力快照）、版本测试结果记录（日期/版本/触发器/结果流水）、测试如何进行（CI 三层 + 本地复现）、agent 差异与规避（七行表格化）、待补测（exit2+JSON 并发降级为「上游语义引用，非我方行为面」——用户裁定聚合属 agent 领域）。原「实测环境」节撤销：版本+日期已内嵌于各结论行，OS 特有项归 §4 差异表。
- **过程史迁 cairn**：新主题笔记 `cairn/agent-hook-testing.md`（三次更正史链条/判定准则/mock 三坑/灰度机制/聚合归属裁决/crush run 悬案插桩排查全记录含证据代码位置）；doc 内更正叙事删除（LOG/ROADMAP 既有条目已完整承载）。
- **约束落档**：doc/AGENTS.md 新增「确定性事实与过程史分离」写作约定（含指针不得指向未追踪文件）；重写前已按存档规则快照 `doc/archive_doc_v1/`。
- Details: `doc/agent-compat-matrix.md`（重构版）、`cairn/agent-hook-testing.md`（新主题笔记）。

## 2026-09-09 · 三 Rust skill 全库审查：代码卫生收口 + clippy 门禁加固

- **按 rust-best-practices / rust-testing / rust-async-patterns 三 skill 通读全部 src/ 与 tests/**，整体结论：架构与测试形态健康（错误处理 thiserror 风格手工实现、fail-safe 语义一致、测试覆盖充分），本轮只收卫生债不改行为。rust-async-patterns 无适用改动（项目为 std::thread 串行 accept 设计，rhai Engine 非 Send 的选型已文档化）。
- **改动**：① `lookup.rs` classify_section flag 命中双重 find 收敛为单次 + zip/and_then 结构简化；② lookup 规范形化两处「collect→into_iter→fold」中间分配去掉；③ `lint.rs` precedence 免克隆改借用；④ `script/lua.rs` 18 处注册 `.expect()` 改 `Result` 传播（`compile_err` 助手，与同文件 register_decision_table 风格统一）；⑤ 双引擎删除与 trait 完全重复的 inherent `decls()`/`allow_literals()`；⑥ `Cargo.toml` 新增 `[lints.clippy]` redundant_clone/needless_collect=deny——**新门禁首跑即抓到 `tests/fixture/mod.rs` 一处真实冗余克隆**（kb.clone() 后即弃），门禁自证有效。
- Details: 本条即全部（无设计变更，不新增主题文档）；门禁经 fmt/clippy/test/audit 四道全绿验证。

## 2026-09-09 深夜 · M7.3 收官：agent-matrix 首跑全绿（CI 三层落地闭环）

- **首跑实质链路即通**：CI 全新未信任环境 + `DISABLE_GROWTHBOOK=1` 下 claude hooks 触发（ci-perm/ci-post 落盘、Linux 工具=Bash），crush job 挂在 tar 解包路径。两处机械修（解包后 find 定位二进制；dump 嵌套转义 JSON 断言去引号）+ setup-uv v6 / actions v7 消 Node20 警告后，814c0ee 双 job success（~1 分钟/轮）。
- **教训**：① goreleaser tar 包二进制可能不在解包根，安装用 find 定位；② dump 行 stdin 为嵌套转义 JSON，跨层断言按去引号键名匹配；③ 首跑失败 ≠ 机制失败——先看工件（artifact 证实 hook 已触发）再修断言。
- Details: `cairn/ROADMAP.md`（M7.3 剩余项二次更新）、`.github/workflows/agent-matrix.yml`。

## 2026-09-09 深夜 · M7.3 执行：CI workflow 落地 + crush 超时实测收口 + 测试资产退役

- **agent-matrix workflow 落地**（42d5b57）：`.github/workflows/agent-matrix.yml` 三层（push/PR pinned smoke + weekly cron latest 上游哨兵 + dispatch 指定版本），正向断言 dump 出现；mock 固化 `script/mock_llm.py`（双协议/SSE/工具名自适应，三坑写进脚本头），`scripts.md` 登记。双侧本地预演全过后提交；**首跑验证待 push**。
- **crush 超时实测 ✅**（待补测清单又收一项）：headless + delay 控制文件 45s > timeout 30 → 33s 后放行执行，与 claude ~32s 行为及 crush 文档语义一致——超时语义已可完全 headless 验证。
- **测试资产退役 ✅**：TestProject 清至只剩空 `.git`（`.claude/`/`.crush/`/`.crush-tether/`/crush.json/调试日志全清，顺带终止两个 15h 前残留 crush.exe 解锁 db）；tmp 478MB→276K（claude-old/clone/两插桩二进制/旧 mock 全删，保留 .go 源码证据样本与排查计划——cairn 记录仍引用）。
- Details: `cairn/ROADMAP.md`（M7.3 剩余项更新）、`script/scripts.md`（mock 登记）。

## 2026-09-09 深夜 · M7.3 追加：claude 灰度可官方 env 钉死 + Windows 工具名坑——CI 配方两侧确定化

- **官方文档结论**（`code.claude.com/docs/en/env-vars`）：`DISABLE_GROWTHBOOK=1` 禁用灰度拉取、所有 flag 落二进制内置默认值（`CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC`/`DISABLE_TELEMETRY`/`DO_NOT_TRACK` 同效）。实测带此 env 跑 `claude -p` + mock **hooks 照常全链触发**（内置默认=开）——**claude CI 哨兵升级为正向硬断言，与 crush 对齐，灰度翻动不再是 CI 不稳定源**。
- **新坑（Windows 工具名）**：claude `-p` 的 shell 工具按运行环境选择——本机 zcode 环境提供 `PowerShell` 无 `Bash`，用户终端提供 `Bash`；mock 盲发 `Bash` 被静默拒（`No such tool available: Bash`，error tool result 伪装成功，与 crush 侧 fantasy 校验拒绝同款）。修法：mock 从请求 tools 列表自适应选工具名（现固化 `script/mock_llm.py`）+ matcher 放宽 `"Bash|PowerShell"`（`TestProject/.claude/spike-settings.json` 已改）。修后连跑两发（含 GB-off）dump 各 +3 全触发。
- **CI 配方定稿**：claude = `DISABLE_GROWTHBOOK=1` + 自适应 mock + 宽 matcher + 正向断言；crush = mock + `crush run` + 正向断言。两侧零费用零灰度零凭据全确定。
- Details: `doc/agent-compat-matrix.md`（headless 节深夜追加 + CI 三层节 + 能力表 claude 格）。

## 2026-09-09 深夜 · M7.3 二次更正：crush run 一直执行 hooks，「不执行」系 mock 缺陷假象（插桩排查定案）

- **悬案告破**：按排查计划（现归档矩阵附录）编译 v0.92.0（与 scoop 同 commit 559ec80）复现时发现编译版 run **触发** hooks；2×2 对照（二进制 × `-m`）锁定真判别变量 = mock vs 真实模型；五处插桩证明 run 接线全程健康但工具从未执行；crush.db 会话记录定案根因——**mock 的 OpenAI-compat 路径返回的工具调用缺必填 `description`，fantasy 分发前按 schema 校验、缺参静默拒绝**（error tool result、无日志），mock「见 tool result 即回 spike done」伪装成功；触发过的 TUI 实验全是真实模型驱动。
- **修正结论**：`crush run` 无条件正常执行 hooks（与 TUI 同链路）；两 agent headless 统一「均可用」（claude 条件性/灰度、crush 无条件）；提 crush issue 作废（无 bug）；CI crush 哨兵恢复正向 + mock 参数必须含全部 schema 必填字段。**方法论教训 +1：mock 驱动测试中「有 tool result」≠「工具执行过」，物理副作用判定准则再次制胜。**
- 同步更正：矩阵（能力表/headless 节/CI 哨兵）、design.md（Crush 契约/387 行/claude 契约更正指针）、AGENTS.md 状态行、ROADMAP M7.3。
- Details: `doc/agent-compat-matrix.md`（headless 节二次更正）、`cairn/ROADMAP.md`（深夜二次更正段）。

## 2026-09-09 · M7.3 补充：DeepWiki 对照 + crush run 双执行路径发现

- DeepWiki 问答（「无头会触发 hook」）与我方静态分析完全同源（coordinator 共用、hookRunner 不看 interactive），且其 Notes 自认「未直接展示 run.go 完整源码」——属静态应然推断、无动态验证；我方动态证据（交互 TUI 5 条 `Hook completed` INFO vs run 零日志、dump 零记录）优先。
- **新发现：run 子命令实为双执行路径**（run.go 全文走查）：client/server 模式（`runNonInteractive`，连 `-H` server 或自动拉起）vs 本地模式（`AppWorkspace.App().RunNonInteractive`）——此前分析未纳入；server 长驻进程持有旧 config 为新排查线索。
- 源码插桩排查计划（内容已归档至 `doc/agent-compat-matrix.md` 附录「crush run 悬案插桩排查记录」；tmp 载体已删）。
- Details: `doc/agent-compat-matrix.md`（headless 节）、`cairn/ROADMAP.md`（M7.3 更正段）。

## 2026-09-09 · M7.3 更正：headless hooks 为条件性加载（受控重测推翻初判）

- **初判「headless 不加载」错误**：根因 = 误信 `Registered 0 hooks` / `Found 0 total hooks in registry` 日志计数——受控重测证明 hook 实际执行时该计数仍打印 0。**方法论沉淀：hook 执行判定以 dump 物理副作用 + `[INFO] Slow PreToolUse hooks` 行为准，日志计数仅作参考。**
- **修正结论**：claude-code `-p` hooks 为**条件性加载**（随 GrowthBook 灰度 cold↔热翻动：下午 cold 全不执行，晚间起 3/3 执行且全语义正常）；`CLAUDE_CODE_ENABLE_FUNCTION_HOOKS` 无关；`--settings` 不屏蔽（hooks 聚合 2 hooks 并存）；crush `run` 跨日复测仍不执行——固有行为，与 claude 定性分开。官方文档「矛盾」撤消。
- **crush 侧源码+日志实锤（晚间追加）**：run 下 `runner.Run` 未被调用（交互 TUI 5 条 `Hook completed` INFO / run 零 hook 日志），三种注册途径含 crushrc `hook add` builtin 全无效——0.92.0 run 路径未接线，提 issue 素材齐备；DeepWiki/源码核对补聚合全序、updated_input 语义、hooks 信任模型与上游 #3482/#3389。
- Details: `doc/agent-compat-matrix.md`（headless 节重写）、`cairn/ROADMAP.md`（M7.3 更正段）。

## 2026-09-09 · M7.3 实测收口：锚点 0 全轴闭环 + headless hooks 失效重大发现

- **两 agent 锚点 0 全轴实测闭环**（交互会话，探针 + 控制文件切实验）：三档全链（claude allow 直通/`ask` 弹窗批准/deny exit 2 阻断；crush npm confirm 走原生权限/git push deny 阻断）、`updated_input` 两侧采纳（echo 改写执行）、fail-open（claude exit 3 non-blocking）、crush exit 49 halt（引擎不用，三 agent 统一单命令阻断）、claude 超时（45s>30s 放行且改写未送达）——两契约节升实测定稿。
- **重大发现：headless（`claude -p`、`crush run`）下 hooks 整体不加载**——hook 进程根本不被拉起（非「阻断被忽略」层），跨配置路径/版本/开关一致且无告警：CI/自动化场景权限门静默缺席，部署验收必须在交互会话实测触发。物理阻断 workaround 对该层无效。
- mock 双协议 LLM 后端零凭证验证通过（CI 方案根基）；CI 三层混合设计定稿（协议回放/冒烟/负向探测/版本追踪 + 发版触发人工清单）；crush 模型层 banned 预拦截、PostToolUse 信号差异（权限学习降级依据）等全录矩阵。
- Details: `doc/agent-compat-matrix.md`（初版）、`cairn/ROADMAP.md`（M7.3 执行结果 + 剩余项）、`doc/design.md`（两契约节定稿 + 失效模式 #2 补注）。

## 2026-09-09 · M7.3 开工：方案定稿（一次性实机实测 + CI 常态回归）

- 方案要点与三层触发器全录 ROADMAP M7.3 条：mock LLM 后端驱动 agent（hook 链路 = agent 进程本地行为，与 LLM 无关——mock 固定 tool_use 响应即可让 agent 走完 hook 全链，零凭证零费用可重复，CI 矩阵成立的根基，spike 待证）；锚点 0 最新版全轴出基线、历史锚点静态不重测按需二分；push = pinned smoke + paths 过滤 agent 耦合面，cron = latest smoke + 新版自动全轴 + bot commit 矩阵机器层（scoop 模式），dispatch = 全矩阵人工回溯排查。
- 状态事实：正式插件已卸载（用户执行，重装即可补）→ zcode 两项补充（确认模式 allow 跳过弹窗、计划模式交叉）标待补测；claude-code 已升 2.1.263（锚点 1 = 2.1.195 窗口关闭，降回溯候选）；两 agent 均走第三方供应商 API（base_url+key 无登录动作）——本地实测耗供应商额度，CI 零 secrets 零消耗。
- Details: `cairn/ROADMAP.md`（M7.3 条 + Current focus）。

## 2026-09-08 · M7.2 收官：两处实机验收闭环（四角色 4/4）+ 退役清单执行

- **两处实机验收全过**：本仓库自测（新会话批准工作区 hook 审核门后 `ws-*` 实弹——bash 转发真实引擎回 `allow`、fail 默认 exit 0、post 正常 dump）；外部项目 TestProject（`.zcode/config.json` 注册落盘后会话即武装）——**四角色 4/4 全覆盖，更正前报「ext-perm 未触发」：实为报告时点早于 confirm 类命令，14:49/14:53 两次 PermissionRequest 实证触发**（requestId 载荷、静默 exit 0 后原生审批弹窗照常）；蛇形+驼峰双命名并存在外部项目再次实锤。
- **引擎侧副作用与设计吻合**：生成物落 `<项目>/.crush-tether/`——默认包三件套（rules.toml/rules.rhai/knowledge.toml，M2.6 定位）+ ADR-07 默认开的 decisions.jsonl 实时裁决（连会话自身的 powershell 进程检查都判 confirm）；serve connect-or-spawn + `--idle-exit 30` 生命周期实证（PID 空闲自退、随用随拉）；默认包 lint 告警为静态提示不影响裁决。
- **退役清单已执行**：删工作区 `.zcode/probe/`（node 探针三副本随删消灭）+ `.zcode/config.json`（配置轨测试注册，防日后与正式插件双 hook 叠跑）+ 用户级 `crush-tether-probe-marketplace` 数据目录与 cache 空壳 + TestProject 全部测试产物；探针资产收敛为入库的 `script/hook_probe.py` 参考实现。P7 仅余 M7.3 + 可选加固（搁置）。
- Details: `cairn/ROADMAP.md`（M7.2 ✅ + 退役清单执行注记）、`script/scripts.md`（探针用法）、`doc/design.md`（hook 探针方法定稿 + zcode 叠加段）。

## 2026-09-08 · M7.2 探针 python 化落地：hook_probe.py + 实机测试就绪

- `script/hook_probe.py`（零第三方依赖）落地：四角色 bash/perm/post/fail 与退役 node 探针语义 1:1；控制文件（perm-out/perm-exit/fail-exit.txt）切模式不改注册；**探针目录与引擎全参数化**（`--probe-dir`/`HOOK_PROBE_DIR` 默认 `<cwd>/.zcode/hook-probe`；`--engine`/`CRUSH_TETHER_EXE` 默认裸命令名 PATH），换项目可复用。
- **关键确认**：`uv run python` 在无 pyproject 的 CWD 走默认解释器临时环境——外部项目（含非 python 项目）可直接注册本脚本，无需自带脚本环境；非实机冒烟四角色全过（转发真实引擎 allow 回包 / perm exit 3 / fail exit 7 / post 仅 dump）。
- 调序（用户拍板）：实机验收两处（本仓库自测 + 外部项目实测）合并等用户批准；退役清单挪至测试通过后执行（node 探针保留到替代品实机验证后）；本仓库 `.zcode/config.json` 已改指 python 探针（审核门待批准）；旧 dump.jsonl 剪至 `tmp/`（用户过目，`tmp/` 入 gitignore）。
- Details: `cairn/ROADMAP.md`（M7.2 状态 + 退役清单收缩注记）、`script/scripts.md`（用法节）、`doc/design.md`（探针方法节参考实现指针）。

## 2026-09-08 · M7.0 + M7.1 完成：写目标感知逃逸检查 + 规则测试工具四件套

- **M7.0（3ce9058）**：`[local]` allow 逃逸检查从「任意参数词」收窄为「写效果路径」（重定向目标 + 知识库新槽位 `write_position="last"` 标注的写参数位），读外纯读不再误拦——落实「读取默认都通过、写入默认只能本仓库内」。关键发现：allow 桶内的写型命令**必须**标注写参数位否则写逃逸失去防护（`touch outside.txt` 回归用例钉死）——默认包补 cp/mv/touch/mkdir 条目，**存量配置不自动迁移**（本仓库自己的 knowledge.toml 已手动补）。design.md 更正登记 22 + 承诺措辞精化「写入不出项目」。
- **M7.1**：四件套落地——`explain`（全溯源：命中层/桶/token/归一链/写扫描集/脚本改判）、`check --batch`（裁决表）、`check --cases`（断言对账，规则变更回归护栏）、`repl`（调试器：空行重复上一条、每条输入重载配置 = 改规则即测、不落日志）。基建 = `RuleSet::classify_single` 单命令共享原语（工具与门禁裁决恒一致）。REPL 形态定点 = 纯 stdio（rustyline 留升级位）。
- **M7.0 四形态 explain 实跑闭环**（读外纯读/读外写内/读内写外/flag 写）——验收按裁定并入 M7.1 完成。
- 插件已按用户决定禁用（日常无门运行，M7 前置验证结论不受影响）；P7 仅余搁置项。
- Details: `cairn/ROADMAP.md`（M7.0/M7.1 ✅ 条目）、`doc/design.md`（更正登记 22 + 「规则测试工具（M7.1，定稿）」节）、`tests/rule_tools.rs`、`README.md`（运行模式）。

## 2026-09-08 · M7 前置完成：正式插件实机三档验证闭环 + hook 审核门发现

- **三档全链实测通过**（正式插件 `type:"process"` + 裸命令名，区别于探针形态）：allow（`cat`/`tail` 单命令无弹窗）/ confirm（`curl --version` 弹窗 → 用户批准 → 执行）/ deny（`sudo --version` 工具调用直接阻断）；复合命令（`echo && powershell`）按多命中合成落 confirm，语义正确。链路点：裸命令名 PATH 解析、connect-or-spawn（serve 常驻进程 + 裁决全走 `mode:"serve"`）、JSONL 裁决日志含 `type:"load"` 事件——失效模式 #2 部署项在开发机闭环。
- **安装启用机制实查**：marketplace add + 安装只有 UI 路径（本地目录型注册格式已实证）；CLI `plugins` 子命令仅 list/enable；`enabledPlugins` 显式条目优先于默认启用（曾现磁盘 false 与 list 渲染不一致，CLI enable 归 true）。
- **两个新发现**：①探针插件已不在册（用户级三处注册均无残留）→ M7.2 退役清单收缩；②zcode 工作区 hook 有审核门（待批准不启用）→ M5.3「配置轨未生效」真因解开，statusMessage 疑因作废（design.md 更正登记 21）。
- M7.2/M7.3 + 可选加固搁置（用户拍板）；M7.0/M7.1 待授权（M7.0 的 explain 验证并入 M7.1）。
- Details: `cairn/ROADMAP.md`（Current focus + M7 前置 ✅ + M7.2 收缩 + M5.3 更正指针）、`doc/design.md`（更正登记 21 + 插件分发节验证闭环）、`cairn/plugin-distribution-analysis.md`（机制实装补录）。

## 2026-09-08 · 分发形态分析登记 + M7 前置安装路线定稿（cargo install 实装）

- **M7 前置二进制可达定稿**（用户确认）：开发测试推荐 `cargo install --path .`，已实机安装（落点 scoop persist rustup `.cargo\bin`，`where` PATH 解析命中 + check 模式 stdin 信封 allow 裁决验证）；README 构建节注明推荐与失效模式 #2 风险提示。
- **插件分发形态分析登记**（未定稿，待正式分发期拍板）：三形态取舍（捆绑+wrapper 为建议目标形态/拆平台否/bootstrap 后备）、平台坑本机实测（执行位存活、quarantine 低危、杀软误报唯一真坑）、双官方 marketplace schema 实查（无平台字段/无 install 阶段；Claude 支持 ref/sha 钉版）、装载守卫三轴模型（hooks.json 接线/wrapper 装载守卫/adapter 协议）、两期分解**建议**（一期 wrapper-only 插件 + PATH 即可先行杀失效模式 #2，二期捆绑 + scoop/Releases 管线）。
- **ROADMAP Open Questions #1 勾销**：mdor 侧退役已随 M6.3 完成（13d175e），条目划掉留注。
- Details: `doc/design.md`「插件分发形态与装载守卫（分析登记）」、`cairn/plugin-distribution-analysis.md`（新建专题：生态机制事实/决策/教训）、`cairn/ROADMAP.md`（OQ#1 + M7 前置 2026-09-08 更新）、`README.md`（构建节）。

## 2026-09-07 · CI 首跑双红诊断与修复（gh 日志闭环）

- **CI run #1（10543eb）两 job 皆红**。ubuntu quality：clippy 失败于 `src/service.rs` `mode()`——interprocess 的 `mode` 是 Unix-only 扩展 trait `ListenerOptionsExt` 的方法而非固有方法，未引入作用域即失效；`#[cfg(unix)]` 块在 Windows 上整体编译掉，本地永远测不出（双 job 平台差异面的首个实锤）。
- **windows-test：失败用例 = `templates_match_design_md_examples_byte_for_byte`，非预登记的 seed 竞态 flake（更正此前预期）**。根因 = `include_str!` 嵌入工作区模板字节 + 仓库无 `.gitattributes` + CI runner `core.autocrlf=true` 干净 checkout 转 CRLF；测试只归一了 design.md 侧。本机工作区文件从未重新 checkout 故保持 LF，测不出。
- **用户裁定走 CRLF 兼容路线**（不强推 `.gitattributes`）：测试两侧归一换行符，护栏语义回归「逐行内容一致」；TOML/rhai/lua 解析器对 CRLF 本就无害，用户侧 seed 包不在任何比对范围。`.gitattributes` 留作后续真正需要字节确定性时再补。
- 教训：`include_str!` 嵌入的是工作区字节，跨平台字节断言必须两侧归一或钉死 eol；平台差异门控的代码块在本平台永远不可测，只能靠对端 job。
- Details: `src/service.rs`（bind 补 trait 引入）、`tests/seed_defaults.rs`（两侧归一）。

## 2026-09-07 · 会话审查闭环：漂移修复 + 退役清单钉死 + 可选加固登记

- **审查发现 3 处文档漂移，全部修复**：ROADMAP P6 行标题滞后（子条目已 ✅ 父行仍写「待用户确认节奏」）；AGENTS.md 状态段自相矛盾（「仅余 M6.3」紧接「M6.3 已完成」——整段重写为收官态口径，消除「M6.1/M6.2 已落地」旧括号）；design.md 失效模式 #2 实测范围精化（实测 = 「进程已启动但非 2 退出码」fail-open；「路径不存在」未测、推断同路、随 M7 前置确认；顺带更正 ROADMAP「M6.2 遗留探针插件」标签笔误）。
- **M7 前置登记于 P7 最前**（正式插件实机验证）：登记时 `crush-tether` 不在 PATH——不装 PATH 直接装正式插件会落进失效模式 #2 的 fail-open；M5.3 实测通过的是探针插件形态（node wrapper + 绝对路径），正式版 `type:"process"` + PATH 解析链路未跑过；M5.3 验收行同步加注。
- **探针退役清单钉死在 M7.2**（用户裁定：审查项 6/7 均随退役消解，不单独处理）：禁用/卸载插件 + 清缓存目录 + 删 `.zcode/probe/`（dump 删前用户过目、三副本漂移随删消灭）+ 删 `.zcode/config.json`（防配置轨日后激活双 hook）+ 清用户级注册；全部为未入库临时件，持久文档不在范围。
- **可选加固登记（带前因后果）**：Windows 句柄继承回归测试——943b205 仅人工验证，普通 CI shell 管道不可继承恰为盲区；若守护缺失，删代码全部测试仍绿、症状只在实用中重现；方案 = 可继承句柄 helper 父进程专用用例（可行但脆，非正式里程碑）。
- 流程处置（用户裁定）：check-links 触发自理；mdor 退役提交（feature/m3-reader 分支）合并时处理；临时件残留不触碰业务代码与持久文档。
- Details: `cairn/ROADMAP.md`（P6 行/Current focus 标签/P7 前置/M7.2 退役清单/可选加固/M5.3 验收注）、`AGENTS.md`（状态段重写）、`doc/design.md`（#2 行精化）。

## 2026-09-07 · CI 工作流落地（参考 mdor ci.yml 适配）

- 新增 `.github/workflows/ci.yml`：ubuntu job = 本仓库全门禁（fmt/clippy/test/check-links/audit，单 crate 无 `-p` 限定）；windows-latest job = 专跑 test（覆盖句柄继承修复、命名管道 serve/hook 等 Windows 专属行为面）。
- **适配点**：check-links 经 `astral-sh/setup-uv`；audit 改用 `taiki-e/install-action` 装二进制（比逐次 `cargo install` 快）；D-08 的 unmaintained 警告默认 warning 级不失败，无需 audit.toml（本地核实仓库与用户级均无配置、退出码 0）。
- **已知风险登记**：seed 并发测试 Windows rename 竞态偶发 flake 可能使 windows job 偶红（维持登记不修）；mlua vendored + tree-sitter 冷构建较慢（rust-cache 承接）。
- Details: `.github/workflows/ci.yml`、`AGENTS.md`「质量门禁」。

## 2026-09-07 · 登记 P7 体验与适配专项 + zcode 原生权限叠加结论

- **P7 登记（四项均待授权，ROADMAP）**：M7.0 写目标感知逃逸检查（用户策略「读默认都通过、写默认只许仓内」——现状 `[local]` 逃逸检查参数级、输入输出不分，读外翻 confirm、`[global]` 豁免整体有洞，正解 = 逃逸检查只看写效果路径）；M7.1 规则测试工具（explain / batch / 断言式用例 / **用户面 REPL**——定位是放开给用户调试自己的规则配置与脚本）；M7.2 探针 python 化（`script/hook_probe.py`，uv 管理无全局 python，方法语言无关入 design.md「hook 探针方法（定稿）」）；M7.3 多 agent 实机兼容性实测 + 特性降级矩阵（不支持的能力默认不生效、绝不报错）。
- **zcode 原生权限四档 × hook 三值叠加（实测/推断分层入 design.md）**：hook ask 覆盖 yolo（实测）；hook allow 跳过确认模式弹窗（同构语义推断未实测）；计划模式交叉未实测；原生「以后都放行」对本门 confirm 类命令结构性失效（hook ask 无状态 + 用户选择不回传）→ 权限学习候选动机补强。
- **工具链口径（教训修正）**：本机 python 由 **uv 管理**（`uv run python` 调用），PATH 上的 `python` 是商店占位别名、不存在默认 python 环境——此前「python 坏了」的表述不准确；agent 脚本杂活可用路径 = `uv run python` 或 node，且 agent 工作方式会直接撞权限门 confirm 兜底（node/go run 类任意代码执行被刻意排除在 allow 外）。
- Details: `cairn/ROADMAP.md`（P7 块 + 权限学习动机）、`doc/design.md`（zcode 叠加段 + 探针方法节 + #2 指针）、`README.md`（默认包画像）。

## 2026-09-06 · M6.3 收官：M5.3 实机探针四项闭环 + 正式插件定稿

- **探针四项观察**（新 zcode 会话 + 插件分发实测）：① stdin 载荷 = ClaudeCode 蛇形键与 zcode 驼峰键**双命名并存**（`toolInput`/`riskLevel`/`sideEffectScope`/`requestId` 等 zcode 增键），adapter 零改动可用；② `PermissionRequest` JSON 信封不被采纳（exit 2 可否决、exit 0 静默走原生确认）→ 挂点保持 `PreToolUse`（其三值 JSON 全部生效：allow 直通连 PermissionRequest 都不触发）；③ 用户最终选择**不回传**任何 hook（PostToolUse 只有执行结果）——「权限学习」候选的保守路线更稳；④ hook 错误（非 2 退出码）→ agent 侧 **fail-open 放行**（失效模式 #2 补齐：部署必查二进制可达）。
- **意外发现**：工作区配置 hook 轨（`.zcode/config.json` + enabled:true）实测未生效，插件轨正常——疑 `process` 型混入 `statusMessage` 字段被丢弃（diagnosing 指南坑 #7）；交付只依赖插件轨，坑已登记 design.md。
- **正式插件 `plugin/` 入库**：marketplace.json + crush-tether/.zcode-plugin/plugin.json + hooks/hooks.json（PreToolUse/Bash → `type:"process"` 直拉 `crush-tether hook --agent zcode`，PATH 解析，无 wrapper）；README「agent 接入」补三步安装 + 部署验收。
- **收尾待用户拍板**：探针插件（crush-tether-probe）禁用；mdor 是否实挂 crush-tether。
- Details: `doc/design.md`（zcode 契约定稿 + 失效模式表 #2 回填 + 更正登记 20）、`cairn/ROADMAP.md`（M5.3/P5/M6.3 勾选）。

## 2026-09-06 · M5.3 探针首获：serve spawn 句柄继承洞（M4.1 修复）

- **M5.3 实机探针（工作区 hook 配置轨）首测即抓到 P4 阻断级缺陷**：node 系祖先（zcode hook runner 即是）下，hook 进程 49ms 完成裁决并退出，但其 stdout/stderr 管道 EOF 迟迟不来——runner 侧表现为 hook 挂死 31s（= serve idle 期）；超时型 runner 会掐死 hook、裁决丢失。bash（MSYS）下不可复现，纯文档自测无法发现。
- **根因**：Windows 句柄继承按句柄自身 inheritable 标志复制，node/libuv 创建的管道可继承、serve 经 `Command::spawn`（bInheritHandles=TRUE）全量复制——serve 攥住 hook 的输出管道直至自身退出（`CRUSH_TETHER_IDLE_EXIT=2` 时 CLOSE 同步缩到 3s，实锤因果）。
- **修复**：`spawn_serve` spawn 前对自身三个 stdio 句柄 `SetHandleInformation` 清 `HANDLE_FLAG_INHERIT`（windows-sys 新增为直接依赖，钉 0.61）；修复后 EXIT/CLOSE 均 ~90ms。
- **教训**：Windows 跨进程 stdio 挂死的排查姿势——先分「进程退出」与「流关闭」两个事件（node `exit` vs `close`），再用 idle 期缩放实验定位持有者。
- Details: `src/service.rs` `spawn_serve`、`Cargo.toml`（windows-sys）；探针资产在 `.zcode/probe/`（gitignore）。

## 2026-09-06 · 文档批：hook 接入失效模式表 + 权限学习候选登记

- **失效模式入档**（用户指定：为后续功能做铺垫）：design.md Agent 适配层节新增「Hook 接入失效模式与保障边界（定稿）」——五条失效模式 × 三层兜底责任（A agent 侧机制 / B 我方管线内部 / C 部署验收实测），作为新 agent 接入的验收对照表；唯一待补洞 = #2（hook 二进制起不来时 agent 侧行为未验证，zcode 探针待办）。
- **权限学习候选登记**（仅候选，未立项）：裁决日志 + PostToolUse 交叉推断用户批准 → suggest 保守路线先行；安全红线三条（deny 不学习 / 最小作用域 / 来源标记）。事实底座：`PermissionRequest` 为 zcode 独有事件（ClaudeCode 无同语义、Crush 未记录），跨 agent 一致信号源 = PostToolUse。
- **事实纪律**：zcode enabled 门槛与插件自动启用引配置指南；ClaudeCode/Crush 启用门槛标注「未核实」不写成既成事实。
- Details: `doc/design.md`（Agent 适配层节新小节）、`cairn/ROADMAP.md`（P6 后候选两条）。

## 2026-09-06 · 审查后修复：lua 协程限流覆盖与文档残留清理

- **审查发现（探针实测）**：mlua `set_hook` 仅挂主线程，脚本自建协程（C 层 `coroutine.create`）完全逃逸指令预算——协程内 200 万次循环毫秒级完成、正常返回 Pass；「限流同等」验收声明对协程不成立。
- **修复**：改 `set_global_hook`（实测覆盖协程，阈值处终止循环）；新增副作用标记型协程限流测试（无墙钟断言——循环被终止则完成标志不置位）。
- **语义边界（design.md 更正登记 18）**：`coroutine.resume` 类 pcall 吞协程内错误——超预算协程被终止（DoS 已阻）但不转化为 fail-safe confirm；rhai 侧限流错误直接中断脚本，两引擎形态有差异、安全性等价（均被有界终止）。
- **文档残留**（本批 docs 提交）：更正登记 19（DSL 节原语清单如实化）；script.file 说明引擎感知；零内置节默认包按引擎生成措辞。
- **教训**：mlua hook 作用域差异 + 限流验证须覆盖并发原语（协程向量），详见 `cairn/rust-rewrite-notes.md`。
- Details: `src/script/lua.rs`、`tests/script_lua.rs`、`doc/design.md`（更正登记 18/19）。

## 2026-09-06 · M6.1+M6.2 收口：Lua 引擎落地，P0–P6 仅余 M6.3

- **M6.1 三件套一批定型**（b5c3ec8 接口层 / 3d9b7bc Lua 引擎 / 34ebf13 script_allow Lua 侧）：ctx 封装（ScriptCtx 自定义类型 + 只读 getter，**脚本侧 `ctx.bin` 属性语法保留**，用户拍板，模板零改动）+ 决策枚举化（ScriptDecision 四变体构造封闭，裸字符串返回边界统一解析，双保险保留）+ LuaEngine（mlua 0.12 lua54+vendored）实现同一 RuleEngine trait，ScriptChain 泛化 Box<dyn>。
- **沙箱与限流**：mlua 无 sandbox feature → StdLib 白名单 + new_with 安全模式 + base 危险全局消毒；限流 = 指令数 hook（20 万条）+ set_memory_limit（16MB），与 rhai 同语义。返回 nil = PASS（词汇约定 nil 等价验收点达成）。
- **script_allow Lua 侧**：机制 2/3 同语义；机制 1 = 注释剥离后保守词法扫描（防注释内 allow 误拒；字符串含 `--` 漏收不误拒）——design.md 更正登记 17。
- **引擎感知装配**：脚本文件按引擎选择 rules.rhai/rules.lua；默认包生成随引擎（default-rules.lua 四谓词与 rhai 双跑对账测试）；本层缺失但有他引擎脚本文件 → stderr 告警；端点名 hash 本已含 engine。
- **M6.2**：README 落地（中文，安装/配置/四模式/agent 接入/安全模型）；audit 口径修正 = D-08（smartstring unmaintained 经 rhai 传递依赖，用户裁定接受并名册化，监控点 = rhai 移除该依赖即升级）。
- **教训**：mlua 对象不保活 Lua state（实例字段锚定）；StdLib::ALL_SAFE 含 IO/OS 不可直接用；rhai getter 是 register_get（闭包收 &mut T）；concat! 无分隔拼接制造 `nilend`（多行脚本文本行尾带 \n）。详见 `cairn/rust-rewrite-notes.md`。
- Details: `doc/design.md`（DSL 引擎节 Lua 定型、更正登记 17、依赖钉版）、`doc/decisions.md` D-08、`README.md`、`src/script/{mod,lua}.rs`、`tests/script_lua.rs`。

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
