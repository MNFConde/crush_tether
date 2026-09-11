# AGENTS.md

crush_tether —— Crush 命令级 bash 权限门（Rust 实现 crush-guard 独立化/重写）。当前状态：**P0–P6 全部收官（M6.3 完成 2026-09-06）+ M7 前置/M7.0/M7.1/M7.2 完成（2026-09-08）**（分类核心 + 配置 v1 全链：三层发现/字段级继承合并/双表三桶查表/知识库归一/双层 lint/默认包生成；脚本层双引擎：Rhai 默认 + Lua（mlua），ctx 封装与决策枚举化定型（M6.1），script_allow 受控放行五件套；serve 服务化：命名端点/connect-or-spawn/热重载/JSONL 裁决日志（D-07）；三 adapter：Crush/ClaudeCode/zcode，正式 zcode 插件 `plugin/` 入库且实机三档验证闭环（二进制经 `cargo install --path .` 进 PATH；插件经 M7 前置实机验证后已按用户决定卸载，2026-09-09）；M7.0 写目标感知逃逸检查（读源豁免/写入不出项目，知识库 `write_position` 槽位）；M7.1 规则测试工具四件套（explain/--batch/--cases/repl，用户自助调试规则）；M7.2 探针 python 参考实现（`script/hook_probe.py`）两处实机验收闭环（四角色 4/4）+ 退役清单执行（node 探针/配置轨测试注册/探针 marketplace 数据全清）；README 已落地）。后续：P7 余 **M7.3 实测收口（2026-09-09；锚点 0 全轴闭环、两契约节实测定稿；headless hooks 定性两次更正后定案「claude 条件性加载（灰度翻动）/ crush run 无条件正常执行——此前「run 不执行」经源码插桩排查证实为 mock 工具参数缺陷（缺 description 被 fantasy 静默拒绝）造成的实验假象」，hook 执行判定以 dump 物理副作用为准、日志计数与 tool result 回执均不可尽信；产出 `doc/agent-compat-matrix.md`；CI workflow 已落地且首跑全绿（`agent-matrix.yml`：两侧 headless 正向断言 + cron 上游哨兵，mock 固化 `script/mock_llm.py`）；zcode 插件已重装回用（3.11.2 实测：updated_input 全替换采纳、确认模式 allow 跳过原生弹窗、计划模式 hook 照常评估且分类器短路 ask，zcode 人工测试流程固化于 doc/test-and-ci.md）；2026-09-10 zcode headless 定性（四更）：App 内嵌 CLI `-p`（0.16.5，双版本轨道）headless 形态证实——更正「无 headless」旧结论，插件轨 hook headless 拉起实证、config 轨工作区信任门 headless 不可首授（`hooks.events.<Event>` 形状 + capable host 信任持久化），入 CI 卡发行渠道（npm 无官方包）→ 2026-09-10 深夜矩阵增 windows 三 job：**zcode 首次入 CI**（extras bucket manifest 解析版本 + CDN 直链 7z 解包取内嵌 CLI + 插件无人值守装配走插件轨，spike 验证 + CI 二跑五 job 全绿；Linux zcode 公测后补 ubuntu job），挂账清单在 doc/test-and-ci.md §5；design.md 增 zcode 契约节）；2026-09-11 测试设计独立成档 `doc/test-and-ci.md`（矩阵瘦身只留结果，doc/ 快照 archive_doc_v2）+ 五 job 冒烟串联化（探针转发真引擎，decisions.jsonl 断言升级）+ 无头场景组 deny/fail-open/rewrite 首测（fail-open 形态分化定性：claude 无头拒绝/crush·zcode 放行；zcode spawn hook 精简 env，探针须绝对路径解释器；场景件 `script/ci_scenario.sh` + mock `--cmd` 双实例）；剩余 = claude 权限管道假说 + exit2+JSON 并发（降级可选）+ zcode headless 余项（ask 收场/超时/模式旗标）+ 批 2 清单，见 [cairn/ROADMAP.md](cairn/ROADMAP.md) M7.3 条）**+ 可选加固（搁置）；CI 双 job + agent 兼容矩阵 workflow 均已落地（`.github/workflows/ci.yml`、`agent-matrix.yml`）。待用户拍板：mdor 是否实挂（M7.2 退役清单已执行，2026-09-08）。格式规范见 [doc/design.md](doc/design.md)，决策论证见 [doc/decisions.md](doc/decisions.md)。

> 本项目使用 Project Cairn 组织项目知识：Cairn 全套规则（初始化配置/阅读顺序/文档职责/冲突仲裁/知识库消费反射/知识沉淀规则）见 `cairn/AGENTS.md`。
> 本机装有 project-cairn skill 且仓库存在 `cairn/` 时生效；否则视为不适用，跳过。

## 阅读顺序

1. 先读本文件（AGENTS.md）。
2. 若存在 `cairn/AGENTS.md`，先读其中 Cairn 规则（含 ROADMAP / LOG 的阅读顺序）。
3. 按需阅读相关 `cairn/` 知识专题文档与 `doc/` 规划文档。

## 文档协作规则

- 改动前判断用户要「讨论/建议」还是「直接改文档」；说「先看看/先评估」时先给分析，别直接重写正式文档。
- 纠正过往判断时追加更正说明，不静默覆盖。
- 未经确认的判断不写成既成事实。
- **引用不得指向 git 未追踪文件**（2026-09-09 起）：doc/ 与 cairn/ 的指针、证据与内容载体一律落在 git 追踪目标（正文、`doc/`、`script/` 等）；`tmp/` 等临时目录只许存放过程产物，不许作为记录的内容载体或指针目标——需持久的内容当场写入正式文档，临时件随任务结束清理。
- 与用户交流一律使用中文。
- **文档一律用中文撰写**：`doc/`、`cairn/`、根 `AGENTS.md`、`README` 及任何项目文档均以中文书写；YAML frontmatter key、`{{PLACEHOLDER}}` 令牌、文件名保持英文模板原样（见 `cairn/AGENTS.md` 中 Cairn 规则）。
- **设计文档统一放 `doc/design.md`**（工程资产，不入 `cairn/`）；cairn 按 skill 要求存知识结论/教训；项目内过程约定入本文件，过长则用指针。

## 协作约定

- 与用户交流一律使用中文。
- 指令若与仓库文档（`doc/`）或既有约定不符，先指出冲突点、说明取舍，再执行。
- 项目状态发生变化（如核心逻辑落地、crate 结构建立、工具链变更）时，同步更新本文件对应状态描述，避免误导后续会话。
- 提交格式规范见 `@.agents/rules/commit.md`，仅在准备 commit 时读取；提交相关经验/坑须登记于该文件「六」节。
- 目录级专属约束见 `doc/AGENTS.md`（doc/ 写作约定 + 决策记录规则）与 `script/AGENTS.md`（script/ 目录约定 + 临时脚本三次法则），读取对应目录下文件时自动生效。

## 质量门禁

- `cargo fmt --check` → `cargo clippy -- -D warnings` → `cargo test` → `cargo audit`（需 `cargo install cargo-audit --locked`）。
- CI（`.github/workflows/ci.yml`）：ubuntu job 跑全门禁含 check-links（audit 经 taiki-e/install-action 装二进制）；windows-latest job 专跑 test 覆盖 Windows 专属行为面。
- 单测重点在核心逻辑（平台无关）；skill 相关约定见下节「项目特化约束（skill 使用）」。

## 项目特化约束（skill 使用）

- **实现前参照**：实现 Rust 代码前按需参照环境中对应 skill 的模式——`rust-best-practices`（惯例与 API 用法）、`rust-testing`（测试组织与模式）、`rust-async-patterns`（涉及异步时）——第一次就写成惯例形态，不等提交前审查返工。
- **提交前审查**：提交前用环境中对应语言的 skill 审查改动，按审查结果修正后再提交（自质量门禁条目移入本节，2026-09-06）。

## 工具链钉版

- `rust-toolchain.toml` 钉版（minimal profile）；本地 MSVC 钉版；勿随手升级。
- 版本约束只钉在根 `Cargo.toml` 一处；升级后必须 `cargo test` + `cargo audit`。

## 架构约定（改代码前先读 doc/design.md）

- 目标结构：**单 crate + `src/lib.rs` + `src/main.rs` 双入口**（库装 `model`/`engine`/`config`/`cmd_parse`/`channel` 分类逻辑，`main.rs` 仅做装配）；核心逻辑无第二消费方，故**不拆 workspace** 分 `core`/`cli` 两 crate。
- 三档分类语义（allow / confirm / deny）与判定表（DESTRUCTIVE/READONLY/GIT_*/写 flag/路径逃逸/管道 sink）见 `doc/design.md`，是纯语义可平移的。
- **零内置策略（2026-09-04 定稿）**：二进制为纯引擎，不内嵌任何规则数据；默认策略由生成到项目侧的外部 `rules.toml` + `rules.rhai` 提供（三层皆缺才生成），见 `doc/design.md`「零内置策略与默认配置生成（定稿）」。
