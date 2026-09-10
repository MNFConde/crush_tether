# 测试用例与 CI 设计

> **定位**:本文档是本项目**测试用例与 CI 设计的唯一承载**——测试方法、CI workflow 设计、agent 差异与规避、CI 覆盖边界、测试事项分类法、挂账规划、探针工具设计。确定性测试**结果**一律在 [agent-compat-matrix.md](agent-compat-matrix.md)(只记结论,不记方法)。两文档分工:矩阵 =「测出了什么」,本文档 =「怎么测、还缺什么」。排查过程、错误结论与更正史在 cairn/(LOG、ROADMAP、[agent-hook-testing](../cairn/agent-hook-testing.md))。

## 1. 测试方法

### 原理

mock LLM 后端驱动 agent 完成一轮固定 tool_use(`echo mock-hook-test`),注册的探针 hook 对该调用落盘 dump——**dump 出现 = hook 被拉起且裁决流转**(正向硬断言)。hook 链路是 agent 本地行为、与 LLM 无关,故 mock 驱动零凭证零费用。探针四角色与控制文件切实验见本文档 §6。

### 三层触发([.github/workflows/agent-matrix.yml](../.github/workflows/agent-matrix.yml))

- **push/PR**:pinned smoke(ubuntu 双 job + windows 三 job)+ `paths` 过滤 agent 耦合面(`src/channel/`、`plugin/`、探针/mock 脚本)——红灯归因「我方破坏」
- **weekly cron**:latest smoke——上游破坏性变更哨兵(红灯 = 上游信号)
- **workflow_dispatch**:手动指定版本回溯/排查
- **zcode windows job 配方**:extras bucket manifest 解析版本(pinned = `ZCODE_VERSION_PINNED`,latest = cron/dispatch)→ CDN 直链下载 NSIS 安装器 → 7z 两步解包取 `resources/glm/zcode.cjs`(只需 node,不装 App)→ `cargo install --path .` 供插件轨 hook 命令 → 插件无人值守装配四件套 + `enabledPlugins` 置真 → mock 驱动 `-p` → 断言 `decisions.jsonl` 落 allow 裁决
- **明确不做**:交互 TUI 自动化(脆弱,维护成本远超每版本 5 分钟人工);agent SDK headless API(不走同一 hooks 路径);zcode config 轨 CI 化(信任门 headless 不可首授)
- **后置**:协议回放(dump 样本驱动引擎)、bot commit 矩阵机器层(测完自动改表提交,`[skip ci]`)、zcode ubuntu job(等 Linux 公测)

### 本地复现

`uv run --directory script mock_llm.py [--port 8787] [--log 请求日志.jsonl]` 起后端;hook 注册 = `script/hook_probe.py` 四角色(crush 走项目 `crush.json` 的 providers+hooks;claude 走 `--settings` 文件 env+hooks);判定 = dump 行数与内容。三坑与判定准则见 §2。

### 人工批 1 清单(~5 分钟,发版触发)

预设注册与控制文件 → 交互会话照单跑:`echo hi`(allow 直通)→ `curl --version`(confirm 弹窗批准后执行)→ `sudo --version`(deny 阻断)→ 控制文件切 exit 3 后任一命令(fail-open 放行)→ 读 dump 断言回填[矩阵 §2](agent-compat-matrix.md#2-版本测试结果记录)。

### zcode 人工测试流程(无 CI 自动化,每版本照此走)

1. **前置**:`crush-tether` 在 PATH(`cargo install --path .`,更新引擎后重装);插件安装 = Plugin Management → Discover → `+` → 本地目录选仓库 `plugin/` → 安装 crush-tether → **重启会话**(hook 自动武装,无需信任门)
2. **headless 冒烟**(可自动化,每版本建议加做):`node <App安装目录>/resources/glm/zcode.cjs -p "<一句驱动 Bash 的指令>"`;模型后端写 `~/.zcode/cli/config.json` 的 `provider` + `model` 键(`model.main` 只接受 `"provider/model"` 字符串,端点与密钥在 `provider.<id>.options` 下);hook 验证走**插件轨**(读 `.crush-tether/decisions.jsonl` 增量断言);config 轨信任门 headless 不可首授,仅交互会话可用(见 §2)
3. **武装判定**:跑任意命令后查 `.crush-tether/decisions.jsonl` 增量——每 hook 触发记一条裁决;`type:"load"` 行为配置加载留痕
4. **三档**:`echo hi`(allow)→ `curl --version`(ask)→ `sudo --version`(deny),读日志断言
5. **`updated_input`**:插件停用 + 探针 config 轨(`.zcode/config.json` 写 `hooks.enabled: true` + `events.PreToolUse` perm 角色,指向 `script/hook_probe.py`)→ 交互会话批准信任门武装 → 控制文件 `perm-out.txt` 切信封(Claude 式 `updatedInput` / crush 式对照)→ 看执行输出是否被改写 → **测后退役 config 轨**(防与插件双轨叠跑)
6. **模式交叉**(确认模式/计划模式,人在场看弹窗):
   - 确认模式:跑 allow/ask/deny 三类,观察弹窗——allow 应跳过弹窗、ask 应弹、deny 不弹直接挡;**区分「人工批准 vs 自动放行」用反证实验**:弹窗上点拒绝,agent 侧收到 Denied 即弹窗为真
   - 计划模式:hook 照常评估(日志增量可证);计划模式只读分类器会**短路 ask**(不弹窗直接拦);agent 层被系统硬约束禁写,「hook allow 写操作」不可达
7. **版本记录**:ZCode Desktop App 版本随测随记入[矩阵 §1.1/§2](agent-compat-matrix.md)(当前 3.11.2,内嵌 CLI 版本轨道独立以 `zcode.cjs version` 为准,当前 0.16.5)

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

## 3. CI 覆盖边界(谁测什么)

- **CI 机测面(push/PR/cron 全部 job)**:headless 下 hook 被拉起 + allow 裁决流转——mock 单发固定 `echo`、引擎默认规则 allow,断言探针 dump(claude/crush)或 `decisions.jsonl`(zcode)
- **不在 CI 内**:confirm 弹窗(人工性本质不可机测)、fail-open、`updated_input`、超时、模式交叉、zcode config 轨(信任门 headless 不可首授)——由人工批 1 / 专项实验维护
- deny 路径**可以**机测(规则侧把 `echo` 设 deny 即可无 UI 断言),未立项,待拍板

## 4. 测试事项分类法

(定稿随首批 CI 场景固化落笔;骨架:维度三标签[协议/agent 行为/模式×hook] × 手段[引擎单测/探针实验/串联冒烟/人工交互] × 知识状态[未定性/已定性] × 固化产物[矩阵行/CI 断言/前提条件/哨兵],生命周期 待探→已定性→已固化→守护中。)

## 5. 测试规划与挂账

> 已收口的项不再留痕于本节(测完即删,过程史在 cairn)。新挂账按 §4 分类法打标签。

| 事项 | 维度 | 手段 | 固化目标 | 说明 |
|---|---|---|---|---|
| claude-code **交互 + 全放行形态**(``--dangerously-skip-permissions`` / `allowedTools:["*"]`)下 hook 是否仍被评估 | agent 行为 | 人工交互 | [矩阵 §1.2](agent-compat-matrix.md) | 社区「权限管道跳过」假说(zcode 侧已有同构结论:hook 评估先于原生权限并可覆盖 yolo) |
| exit 2 与 JSON 回包并发时的覆盖规则 | 协议 | 探针实验 | design.md 契约节核对 | **上游聚合语义引用(halt > deny > allow),非我方行为面**,仅可选抽查以验证 design.md 契约节引用的准确性 |
| claude-code 非默认 permission_mode(plan/bypassPermissions 交互)× hook 交叉 | 模式×hook | 人工交互 | [矩阵 §1.3](agent-compat-matrix.md#13-原生模式-hook-交叉按-agent) | 未测 |
| crush 原生确认/计划模式 × hook 交叉 | 模式×hook | 人工交互 | [矩阵 §1.3](agent-compat-matrix.md#13-原生模式-hook-交叉按-agent) | yolo 语义已有源码级核对,模式交叉未实测 |
| zcode headless 全轴:三档语义、`updated_input`、模式交叉在 headless 形态下的表现 | 协议 | 探针实验 | CI 场景组 + [矩阵 §1.2](agent-compat-matrix.md) | 现 mock 只发固定 `echo`,deny/confirm 需扩展 mock 或换规则 |
| zcode ubuntu job | agent 行为 | 串联冒烟 | workflow | Linux 版内测中,公测后补(发行渠道落地即可平移 windows job 配方) |
| zcode cron latest 哨兵的首个自动触发尚待观察 | 协议 | 串联冒烟 | [矩阵 §1.1/§2](agent-compat-matrix.md) | 每周一 UTC |

## 6. 探针与工具设计

部署验收与新 agent 接入对照表都要实测「hook 真的被拉起来、裁决真的被采纳」,探针是与 agent 无关的标准方法。**方法论语言无关**——示例实现用过的解释器只是当时本机可用工具的选择,不是依赖:

1. **dump wrapper**:hook 注册一个 wrapper 进程(任何解释器/语言均可),把 stdin 载荷原样落盘(JSONL 追加),再原样转发给 `crush-tether hook --agent <slug>` 并回传其 stdout 与退出码——观测到的是 agent 真实下发的载荷与我方引擎的原话裁决;
2. **控制文件切模式**:wrapper 每次调用读小控制文件决定行为(如 perm 回包内容 / 退出码覆写 / fail 模拟退出码),换实验不改注册、不重启会话;
3. **来源标记**:多注册来源(配置文件轨 vs 插件轨)各打标记进 dump,确认哪一层真的在触发(zcode 实测:插件轨触发、配置轨未生效,即靠此法区分);
4. **exit / close 双事件观测**:排查「hook 挂死」类问题先区分**进程退出**与**输出流关闭**两个时刻(node 下即 `exit` vs `close` 事件)——句柄被孙进程继承时二者分离,这是 M4.1 句柄继承洞(更正登记 20)的定位手法;
5. **idle 缩放实验**:把 serve 的 `--idle-exit` 调小,若某等待时长同步缩短,即证明持有者是 serve 的存活期——因果坐实而不靠猜。

注册面要点:zcode 插件 `hooks/hooks.json` 用 `type:"process"`(``command``/`args`/`timeoutMs` 三字段严格,勿混入 `statusMessage`)+ `${ZCODE_PLUGIN_ROOT}` 相对路径;dump/控制目录用参数传入而非写死。参考实现:`script/hook_probe.py`(M7.2 已落地,python 零第三方依赖,经 `uv run python` 调用;探针目录与引擎全参数化——`--probe-dir`/`HOOK_PROBE_DIR`、`--engine`/`CRUSH_TETHER_EXE`,用法见 `script/scripts.md`)。
