# 测试用例与 CI 设计

> **定位**:本文档是本项目**测试用例与 CI 设计的唯一承载**——测试方法、CI workflow 设计、agent 差异与规避、CI 覆盖边界、测试事项分类法、挂账规划、探针工具设计。确定性测试**结果**一律在 [agent-compat-matrix.md](agent-compat-matrix.md)(只记结论,不记方法)。两文档分工:矩阵 =「测出了什么」,本文档 =「怎么测、还缺什么」。排查过程、错误结论与更正史在 cairn/(LOG、ROADMAP、[agent-hook-testing](../cairn/agent-hook-testing.md))。

## 1. 测试方法

### 原理

mock LLM 后端驱动 agent 完成一轮固定 tool_use(命令可配,`--cmd`),注册的探针 hook 对该调用落盘 dump——**dump 出现 = hook 被拉起且裁决流转**(正向硬断言)。hook 链路是 agent 本地行为、与 LLM 无关,故 mock 驱动零凭证零费用。探针四角色与控制文件切实验见本文档 §6。

### 串联冒烟(五 job 同构)

每 job 的正向断言走**真引擎全链**:探针 bash 角色转发 `crush-tether hook`(claude/crush)或正式插件直挂引擎(zcode),断言 `.crush-tether/decisions.jsonl` 落 `echo mock-hook-test → allow` = 「hook 拉起 + 引擎裁决 + agent 采纳」。守护对象是**契约两侧的漂移**(手写 mock 信封 vs 引擎真实产物),每 agent 一条即足,不铺场景。

### 场景组(deny / failopen / rewrite / ask / timeout / exitjson)

探针 perm 角色直回控制文件信封(**引擎不在场**),断言物理副作用 `ci-exec.txt`(场景 mock `--cmd` 发带写命令 `echo mock-hook-test > ci-exec.txt`;冒烟与场景各一个 mock 实例,8787/8788——冒烟命令须引擎默认规则放行[带写命令实测裁 confirm],场景命令只求留痕,互不牵制):

| 场景 | 控制文件 | 断言(按 agent 分化;2026-09-11/09-12 定性) |
|---|---|---|
| deny | `perm-out.txt` 回 deny 信封 | 三家一致:文件**不存在**(工具被阻断) |
| fail-open | `perm-exit.txt`=3(无信封) | **claude:文件不存在**(无头把 hook 失联兜底为拒绝,`permission_denials` 回执);**crush/zcode:文件存在**(放行,与交互一致) |
| rewrite | `perm-out.txt` 回 allow+改写信封 | 三家一致:文件内容含 `rewritten-marker`(改写命令执行) |
| ask | `perm-out.txt` 回 ask 信封(crush=空输出=无意见) | **claude:不存在**(无头拒绝,2026-09-12);**crush:存在**(run 原生权限自动接受,2026-09-12 新定性);**zcode:不存在**(2026-09-11 定性,场景批回归固化) |
| timeout | `delay.txt`=40s 压过探针 timeout 30s + allow 信封 | **claude:不存在**(无头超时=拒绝,2026-09-12——交互 ~32s 放行的形态分化);**crush:存在**(34s,复证 2026-09-09 33s);**zcode 不入环**(未定性,挂账 §5) |
| exitjson | deny 信封 + `perm-exit.txt`=2 并发(B4) | 三家一致:文件不存在(两通道同向 deny;上游聚合 halt>deny>allow 任一解释下一致) |

脚本 `script/ci_scenario.sh`(setup/assert 两段式);信封按 agent 分支(claude/zcode = `hookSpecificOutput`,`updatedInput` 全替换须含 schema 全字段;crush = 顶层 `decision`/`updated_input` 浅合并)。首跑红灯语义 = 上游采纳语义漂移或我方信封笔误,人工判读后回填[矩阵 §2](agent-compat-matrix.md#2-版本测试结果记录)。

### 三层触发([.github/workflows/agent-matrix.yml](../.github/workflows/agent-matrix.yml))

- **push/PR**:pinned smoke(ubuntu 双 job + windows 三 job)+ `paths` 过滤 agent 耦合面(`src/channel/`、`plugin/`、探针/mock 脚本)——红灯归因「我方破坏」
- **weekly cron**:latest smoke——上游破坏性变更哨兵(红灯 = 上游信号)
- **workflow_dispatch**:手动指定版本回溯/排查
- **zcode windows job 配方**:extras bucket manifest 解析版本(pinned = `ZCODE_VERSION_PINNED`,latest = cron/dispatch)→ CDN 直链下载 NSIS 安装器 → 7z 两步解包取 `resources/glm/zcode.cjs`(只需 node,不装 App)→ `cargo install --path .` 供插件轨 hook 命令 → 插件无人值守装配四件套 + `enabledPlugins` 置真 → mock 驱动 `-p` → 断言 `decisions.jsonl` 落 allow 裁决
- **明确不做**:交互 TUI 自动化(2026-09-11 深化论证:技术路径 = tmux/pexpect 驱动 TUI 字符画面匹配弹窗文案,脆弱性高——上游改提示语/终端宽度/主题即红灯,时序轮询天然 flaky,且会稀释哨兵红灯归因(pinned 红=我方破坏/cron 红=上游信号);三 agent 三套栈(crush/claude TUI + zcode Electron GUI 互不通用);价值密度低——交互语义大版本才动、人工批测 5 分钟/版本,CI 化省的时间抵不过修断言的时间。折中:若未来真实痛点出现,可只取「弹窗出现+批准/拒绝两路+物理副作用」子集挂 workflow_dispatch 手动触发,不进 push/cron;agent SDK headless API(不走同一 hooks 路径);zcode config 轨 CI 化(信任门 headless 不可首授)
- **后置**:协议回放(dump 样本驱动引擎)、bot commit 矩阵机器层(测完自动改表提交,`[skip ci]`)、zcode ubuntu job(等 Linux 公测)

### 本地复现

`uv run --directory script mock_llm.py [--port 8787] [--cmd 命令] [--log 请求日志.jsonl]` 起后端;hook 注册 = `script/hook_probe.py` 四角色(crush 走项目 `crush.json` 的 providers+hooks;claude 走 `--settings` 文件 env+hooks);判定 = dump 行数与内容 + decisions.jsonl 增量。三坑与判定准则见 §2。

本机(Windows)与 CI 的形态差异:本机无 `python` 命令,claude/crush 的 hook command 写 `uv run --directory <script> python …` 可跑(两者经 **shell** 执行,有完整 PATH);**zcode 插件轨 hook 由 zcode 直接 spawn(精简 env),`uv` 类 wrapper 静默失效,须绝对路径解释器**(§2 差异 12)。CI 里 claude/crush 用 `python3`/`python`,zcode 探针插件用运行时探测的 `sys.executable`。

### 新版本人工交互测试流程(发版触发,三 agent 共通前置)

前置:`cargo install --path .` 更新引擎;探针注册指向 `script/hook_probe.py`,probe-dir 用仓库内 `.hook-probe/`;判定三件套 = 探针 dump(物理副作用唯一判据,§2 差异 7)+ `.crush-tether/decisions.jsonl` 增量 + 工作区文件变化;测完回填[矩阵 §2](agent-compat-matrix.md#2-版本测试结果记录)一行流水。

**claude-code(交互,~5 分钟)**:注册四角色(bash/perm/post/fail,matcher `Bash|PowerShell`)→ 交互会话五连:`echo hi`(allow 直通)→ `curl --version`(ask 弹窗,批准后执行)→ `sudo --version`(deny 阻断)→ 控制文件切改写信封跑 echo(`updatedInput` 全替换)→ 控制文件切 exit 3 跑任一命令(fail-open:应放行 + UI 明示 non-blocking)。

**crush(交互,~5 分钟)**:同五连,差异点:ask = 无意见走原生权限提示;deny = exit 2 + stderr 或 JSON;改写 = 浅合并(TUI 标记 `Rewrote Output`);**先确认命令不在 banned commands 黑名单**(curl/sudo 被模型层劝退不触达 hook 层,§2 差异 6——deny 实验换黑名单外命令)。

**zcode(七步)**:① 前置:引擎在 PATH;插件安装 = Plugin Management → Discover → `+` → 本地目录选仓库 `plugin/` → 安装 → **重启会话**(无需信任门);② headless 冒烟(每版本建议):`node <App安装目录>/resources/glm/zcode.cjs -p "<驱动 Bash 的指令>"`,模型后端写 `~/.zcode/cli/config.json` 的 `provider.<id>.options` + `model.main`(只接受 `"provider/model"` 字符串),hook 走插件轨读 decisions.jsonl 增量;③ 武装判定:跑命令后查 decisions.jsonl 增量(`type:"load"` 为加载留痕);④ 三档:`echo hi` / `curl --version` / `sudo --version`;⑤ `updated_input`:停用插件 + 探针 config 轨(`hooks.enabled` + `events.PreToolUse`)→ 批准信任门 → 控制文件切信封(Claude 式/crush 式对照)→ 测后**退役 config 轨**;⑥ 模式交叉(确认/计划,人在场):确认模式 allow 跳过弹窗/ask 弹窗/deny 不弹,反证实验(点拒绝 → agent 收 Denied)钉死人工性;计划模式 hook 照常评估、只读分类器短路 ask、agent 层硬约束先于 hook;⑦ 版本记录:App 版与内嵌 CLI 版双轨分记。**注意**:本机注册探针若用 `uv run` 会被 zcode 插件轨精简 env 静默失效,须绝对路径 python.exe(§2 差异 12)。

**无头格分工**:fail-open 无头格只有 claude 与交互不同(拒)——已由 CI 场景组自动兜底,人工只测交互格;ask 无头收场与超时无头尚未定性(§5 挂账),人工测到可顺手补记。

## 2. agent 差异与规避

| # | 差异 | 规避 |
|---|---|---|
| 1 | crush:工具调用缺 schema 必填字段(如 `description`)→ fantasy 校验**静默拒绝**(error tool result、无日志,伪装成对话正常) | mock 工具参数含全部必填字段 |
| 2 | claude(Windows):shell 工具按运行环境选(`PowerShell`/`Bash`),工具列表无 `Bash` 时盲发被拒(`No such tool available`) | mock 从请求 tools 列表自适应选名 + matcher `"Bash\|PowerShell"` |
| 3 | claude:hook 加载随 GrowthBook 灰度翻动(冷窗口静默缺席) | `DISABLE_GROWTHBOOK=1` 钉死(官方 env;hooks 内置默认=开,实测) |
| 4 | crush OpenAI-compat:`stream=true` 必须回 SSE 分块,回 JSON 得 unexpected EOF | mock 完整实现分块(role → tool_calls → finish) |
| 5 | claude:settings env 三级优先级(shell < 用户级 `env` 块 < `--settings`),shell 注入会被覆盖 | 实验端点注入走 `--settings` |
| 6 | crush:banned commands 模型层预拦截(curl/sudo 在工具调用前被劝退,不触达 hook 层) | 测试用黑名单外命令 |
| 7 | **判定准则(通用)**:mock 驱动下「有 tool result」≠「工具执行过」(校验拒绝/工具缺席都会回 error result);hook 执行以**探针 dump 物理副作用**判定;日志计数(`Registered 0 hooks`)与回执均不可尽信 | 探针 dump 为唯一判据 |
| 8 | zcode 探针 config 轨:hook 在命令**执行前**读控制文件(天然差一拍),且 `updatedInput` 全替换会把同调用内的写文件操作一并废掉 | 改控制文件用**非 Bash 工具**(hook matcher 只匹配 Bash);每轮只发纯探测命令 |
| 9 | zcode:非交互入口不在 PATH——CLI 是 App 内嵌 bundle(`resources/glm/zcode.cjs`),版本轨道与 App 版本号独立(App 3.11.2 / CLI 0.16.5) | `node <app>/resources/glm/zcode.cjs -p "<指令>"`;双版本号分别记录 |
| 10 | zcode config 轨:事件声明在 `hooks.events.<Event>` 下(非插件 envelope 的 `hooks.<Event>`,写错仅日志 `config.file.invalid` 提示);项目 hooks 需工作区信任,信任由 **capable host**(Desktop App UI)授予、按 工作区+声明 digest 持久化于 `~/.zcode/security/workspace-hook-trust-v1.json`——headless **不可首授** | headless/CI 一律走**插件轨**(免信任,实测 hook 可拉起);config 轨仅交互会话用 |
| 11 | zcode:Anthropic SSE `content_block_start` 的 `input` 字段严格校验(须为对象),mock 回空串即整回合失败(`AI_TypeValidationError`) | mock 固化版已修正为 `"input": {}`(claude/crush 对空串宽容) |
| 12 | zcode 插件轨 spawn hook 给**精简 env**:`uv run` 等 wrapper/PATH 依赖的 command 静默失效(uv 找不到其管理的 python,连首行诊断都写不出,2026-09-11 实测);hook 失效后 zcode 无头权限兜底拒绝工具(行为与 claude 无头 fail-open 同构,极易误判为「语义拒绝」) | 插件 hook command 用**绝对路径解释器**(CI 探针插件用运行时 `sys.executable`);排障先验「spawn 可达性」(command 换 `--version` 类空参快跑)再疑语义 |
| 13 | claude:fail-open 兜底按**会话形态分化**——交互放行(UI 明示 non-blocking)/ 无头拒绝(`permission_denials` 回执,2026-09-11 实测);crush/zcode 无头与交互一致放行 | CI 场景断言按 agent 分化(failopen:claude=拒,crush/zcode=放行,`ci_scenario.sh` 已内置);矩阵 §1.2 该格分形态记录 |

## 3. CI 覆盖边界(谁测什么)

- **CI 机测面(push/PR/cron 全部 job)**:①串联冒烟——headless 下真引擎全链走通,断言 `decisions.jsonl` 落 `echo mock-hook-test → allow`(五 job 同构,守护契约漂移);②场景组——deny/fail-open/updated_input 三场景探针信封 + 物理副作用断言(`ci-exec.txt`,failopen 按 agent 分化),判定准则见 §2 差异 7
- **不在 CI 内**:confirm 弹窗(人工性本质不可机测)、ask 无头收场与超时(未定性,§5)、模式交叉、zcode config 轨(信任门 headless 不可首授)——由人工流程/专项实验维护
- 真实引擎 deny 全链(预写 rules.toml 触发引擎 deny → agent 阻断)可以机测,未立项,待拍板

### 红灯归因决策树

1. **先看触发器**:pinned(push/PR)红 = 我方破坏;cron(latest)红 = 上游信号——用 `workflow_dispatch` 指定旧 pinned 对照跑一轮,复现 = 我方(或环境),消失 = 上游版本引入
2. **再看挂的 step**:`Install engine` 挂 = cargo/工具链问题;冒烟断言挂 = 链路破(hook 没拉起/引擎裁决异常/信封漂移);场景组挂 = 信封笔误或上游采纳语义漂移
3. **最后看 artifact**:hook 没触发 → `dump.jsonl`(角色标记齐全否);裁决语义疑问 → `decisions.jsonl`(`reason` 字段);工具名/模型回合问题 → `mock-requests.jsonl` / `scenario-requests.jsonl`(请求轮次与 tools 列表)

### 维护口径

- **零 secrets**:全 mock 驱动,任何 runner/fork 原生可跑,排查时排除凭证因素
- **时长预算**:串联化+场景组后 claude/crush job 约 +3 分钟;zcode job(7z 解包 + cargo install + 五次 headless)逼近 25 分钟 timeout,超限先拆分或上调
- **扩展指引**:新增场景 = `ci_scenario.sh` 加分支(信封+断言)+ workflow 循环串加一个词,零注册改动;新增 agent = 复制最接近的 job 模板改注册面与 headless 命令;zcode 场景/冒烟的双装配切换在 zcode-windows job 的场景 step 内完成(CI 环境一次性,无需复原)

## 4. 测试事项分类法

每个测试事项(能力/行为/探测)按四个正交标签编目,沿生命周期单向推进:

**维度三标签**(划分标准 = 与权限控制的直接相关性):
- **协议**:权限控制直接相关的 hook 契约面——信封/退出码/事件/注册发现
- **agent 行为**:agent 内部复杂行为、契约之外的观测面;**兼任兜底**——不好归类的事项(非单纯权限相关、选项混杂其它、也非模式×hook 交叉)一律入此,同质事项聚集时再考虑增设新标签
- **模式×hook**:agent 内置权限模式 × hook 规则的交互面(标签用短名,全称在此)

**手段四标签**:引擎单测(cargo test,裁决逻辑)/ 探针实验(mock 信封,**首跑即能力探寻**)/ 串联冒烟(真引擎×真 agent,守护契约漂移)/ 人工交互(弹窗、信任门、交互语义)。

**知识状态**:未定性 / 已定性。**固化产物**:矩阵行([agent-compat-matrix.md](agent-compat-matrix.md) §1/本文档 §2)/ CI 断言(workflow + 本文档 §3)/ 前提条件(决定他项可否入 CI)/ cron 哨兵。

**生命周期四态(单向推进)**:待探(B:探针/人工首跑,产定性)→ 已定性(矩阵行落地)→ 已固化(CI 断言落地)→ 守护中(矩阵 §3.2 复核分层 + 哨兵)。弹窗维度、信任门首授止步于「已定性 + 人工守护」。

**混合行规则**:协议+agent 行为各半的事项(headless 加载、confirm、超时、项目级门槛),定性按主维度记矩阵;固化时协议半边走探针断言、agent 行为半边止步人工守护。

**§1.2 十四行维度归组**(定稿):协议 9 行(交互加载/allow/deny/updated_input/fail-open/halt/PermissionRequest/用户选择回传/PostToolUse);协议+agent 行为混合 4 行(headless 加载/confirm/超时/项目级门槛);agent 行为 1 行(模型预拦截);模式×hook 1 行(引用行,主体矩阵 §1.3)。

**落点映射**(新事项按此入账):定性 → 矩阵 §1 与本文档 §2;回归断言 → workflow + 本文档 §3;挂账 → 本文档 §5;方法论 → 本节。

## 5. 测试规划与挂账

> 已收口的项不再留痕于本节(测完即删,过程史在 cairn)。新挂账按 §4 分类法打标签。

| 事项 | 维度 | 手段 | 固化目标 | 说明 |
|---|---|---|---|---|
| claude-code **交互 + 全放行形态**(``--dangerously-skip-permissions`` / `allowedTools:["*"]`)下 hook 是否仍被评估 | agent 行为 | 人工交互 | [矩阵 §1.2](agent-compat-matrix.md) | 社区「权限管道跳过」假说(zcode 侧已有同构结论:hook 评估先于原生权限并可覆盖 yolo) |
| exit 2 与 JSON 回包并发时的覆盖规则 | 协议 | 探针实验 | design.md 契约节核对 | **上游聚合语义引用(halt > deny > allow),非我方行为面**,仅可选抽查以验证 design.md 契约节引用的准确性 |
| claude-code 非默认 permission_mode(plan/bypassPermissions 交互)× hook 交叉 | 模式×hook | 人工交互 | [矩阵 §1.3](agent-compat-matrix.md#13-原生模式-hook-交叉按-agent) | 未测;无头等价形态可场景化(`-p --permission-mode …`,B2 预研 2026-09-12) |
| crush 原生确认/计划模式 × hook 交叉 | 模式×hook | 人工交互 | [矩阵 §1.3](agent-compat-matrix.md#13-原生模式-hook-交叉按-agent) | yolo 语义有源码级核对;仅 `--yolo` 一档,无头交叉可场景化(`crush run --yolo`,B2 预研) |
| zcode headless 超时收场(hook timeout 杀进程后工具命运) | 协议+agent 行为 | 探针实验 | CI 场景组+ [矩阵 §1.2](agent-compat-matrix.md) | 未定性未入 CI 环(2026-09-12):探针 timeoutMs 30s+delay 40s 配方就绪;本机实弹因 App 运行中不动 live 配置而缓——CI 首跑或 App 关闭后补;ask/exitjson 已入环(assert 有定性/语义支撑) |
| zcode ubuntu job | agent 行为 | 串联冒烟 | workflow | Linux 版内测中,公测后补(发行渠道落地即可平移 windows job 配方);同批换接 wrapper .sh(hooks.json 现指 .cmd,M8.2) |
| wrapper 本机 zcode 0.2.0 实弹 | agent 行为 | 插件重装+headless | [矩阵 §1.2](agent-compat-matrix.md) | 0.2.0 wrapper 已落地(M8.2,CI 三 job 覆盖),本机 cache 拷贝须重装生效,现网 0.1.0 行为不变;随用户下次插件重装一并验 |
| zcode cron latest 哨兵的首个自动触发尚待观察 | 协议 | 串联冒烟 | [矩阵 §1.1/§2](agent-compat-matrix.md) | 每周一 UTC |

## 6. 探针与工具设计

部署验收与新 agent 接入对照表都要实测「hook 真的被拉起来、裁决真的被采纳」,探针是与 agent 无关的标准方法。**方法论语言无关**——示例实现用过的解释器只是当时本机可用工具的选择,不是依赖:

1. **dump wrapper**:hook 注册一个 wrapper 进程(任何解释器/语言均可),把 stdin 载荷原样落盘(JSONL 追加),再原样转发给 `crush-tether hook --agent <slug>` 并回传其 stdout 与退出码——观测到的是 agent 真实下发的载荷与我方引擎的原话裁决;
2. **控制文件切模式**:wrapper 每次调用读小控制文件决定行为(如 perm 回包内容 / 退出码覆写 / fail 模拟退出码),换实验不改注册、不重启会话;
3. **来源标记**:多注册来源(配置文件轨 vs 插件轨)各打标记进 dump,确认哪一层真的在触发(zcode 实测:插件轨触发、配置轨未生效,即靠此法区分);
4. **exit / close 双事件观测**:排查「hook 挂死」类问题先区分**进程退出**与**输出流关闭**两个时刻(node 下即 `exit` vs `close` 事件)——句柄被孙进程继承时二者分离,这是 M4.1 句柄继承洞(更正登记 20)的定位手法;
5. **idle 缩放实验**:把 serve 的 `--idle-exit` 调小,若某等待时长同步缩短,即证明持有者是 serve 的存活期——因果坐实而不靠猜。

注册面要点:zcode 插件 `hooks/hooks.json` 用 `type:"process"`(``command``/`args`/`timeoutMs` 三字段严格,勿混入 `statusMessage`)+ `${ZCODE_PLUGIN_ROOT}` 相对路径;dump/控制目录用参数传入而非写死。参考实现:`script/hook_probe.py`(M7.2 已落地,python 零第三方依赖,经 `uv run python` 调用;探针目录与引擎全参数化——`--probe-dir`/`HOOK_PROBE_DIR`、`--engine`/`CRUSH_TETHER_EXE`,用法见 `script/scripts.md`)。
