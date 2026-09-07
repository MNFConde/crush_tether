---
type: project_topic
status: active
summary: 插件分发形态分析（2026-09-08 登记未定稿）：失效模式 #2 的对策空间——三形态取舍（捆绑+wrapper 为建议目标形态/拆平台否/bootstrap 后备）、平台坑本机实测、双官方 marketplace schema 实查（无平台字段、Claude 支持 ref/sha 钉版）、装载守卫三轴模型（hooks.json/wrapper/adapter）、分发两期分解与 scoop=Releases 等价机制。已定稿：开发测试推荐 cargo install --path .（M7 前置二进制可达路线，2026-09-08 实机三档验证闭环）；zcode 插件安装启用机制与工作区 hook 审核门实查见文末两条。
tags: [crush_tether, distribution, plugin, hook, marketplace]
contains: [decision, lesson, pattern]
created: 2026-09-08
updated: 2026-09-08
related: [doc/design.md, cairn/ROADMAP.md, cairn/serve-lifecycle-named-endpoint.md]
authoring_mode: ai_generated
---

# 插件分发形态与装载守卫

> 设计取舍底稿在 `doc/design.md`「插件分发形态与装载守卫（分析登记）」；本文沉淀**可复用的生态机制事实、决策与教训**。

## 生态机制事实（跨会话可复用，2026-09-08 实查）

- **marketplace schema（双官方清单实查）**：zcode 与 Claude Code 的插件条目均无 `os`/`arch`/`platform` 字段、无 install 钩子阶段——「按平台分发」「安装时按平台下载」在 schema 层**不存在**（对照：VS Code 扩展市场支持平台定向包，本生态不支持）；按平台只剩拆插件条目一途。
- **版本钉版**：Claude marketplace 条目支持 `ref`（tag/branch）与 `sha`（commit 锁定）；zcode 官方市场为 zip artifact + sha256 经 CDN 交付（本机缓存目录无 `.git` 与之互证）；zcode github 源是否支持 ref/sha 未验证，动工前验一次。
- **安装落盘形态**：zcode 插件缓存为拷贝/解包（非保留 git 克隆），但 Unix 执行位实测存活（官方插件 `.sh` 落盘 755）——「执行位丢失」坑基本消。
- **macOS quarantine**：该标记由浏览器/LaunchServices 下载链路设置，CLI 网络栈与本地拷贝不打标——git/市场安装路线低风险。
- **scoop manifest 机制**：url 必须 http(s) 可取 + hash，无实用本地路径安装 → scoop 路线机制上**等价 GitHub Releases 路线**；自建 bucket 零审批分发（仓库 PUBLIC 即够），manifest 模板（checkver 打 Releases API + autoupdate 重写 url）现成可抄。
- **Windows PATH 机制**：PATH 就是注册表环境变量（用户级 `HKCU\Environment`/系统级 Session Manager\Environment）；安装器在「安装动作执行的那一刻」写入并广播 WM_SETTINGCHANGE，此后仅新进程生效（旧终端不刷新）；能写环境变量就能写 PATH（同一机制）；脚本写 PATH 勿用 `setx`（1024 字符截断坑），用注册表 API。
- **cargo install 落点（本机）**：scoop 版 rustup 的 `.cargo\bin` 位于 persist 目录（`Scoop\persist\rustup-msvc`），`apps\...\current` 是指向它的链接——rustup 经 scoop 升级不丢已装二进制。
- **zcode 插件安装与启用（实装链，2026-09-08）**：marketplace add + 安装只有 UI 路径（Discover `+` 接受本地目录；CLI `plugins` 子命令仅 list/enable、无 install/marketplace 管理）；本地目录型注册格式 = `known_marketplaces.json` 条目 `source: {"source":"directory","path":…}`；安装记录在 `installed_plugins.json`（id = `name@marketplace`，缓存按版本目录拷贝）；`enabledPlugins` 显式条目优先于插件默认启用——磁盘态可能与 `plugins list` 渲染不一致（渲染按合并默认），以读盘为准，`zcode plugins enable <id>` 可归一。
- **zcode 工作区 hook 审核门**：工作区 `hooks` 配置须用户在 UI 批准后才启用（重启后提示「N 个工作区 Hook 待审核，本会话暂未启用」）——排查「配置轨 hook 不生效」先查审核门再怀疑载荷格式；插件轨（插件内 `hooks/hooks.json`）不经审核门，安装启用即生效。

## 决策

- **2026-09-08 用户确认**：开发测试推荐 `cargo install --path .`（二进制入 cargo bin 即 PATH 可达；升级 `--force`、卸载 `cargo uninstall`）。M7 前置的二进制可达以此为准，README 构建节已注明。
- 分发形态选型（捆绑+wrapper / 拆平台 / bootstrap / scoop+Releases 管线）**全部后置正式分发期拍板**，登记≠开工；触发条件建议 = 出现首个非 cargo 用户或仓库对外宣传。

## 教训

- **判分发形态的统一标尺**：看「二进制缺失这一失败落在谁身上、响不响」——失败面落自己可测代码（wrapper）优于落用户操作（选平台）或网络环境（bootstrap）。
- **响亮失败需要必然在场的可执行者**：agent hook 侧「进程没起来 = fail-open」，而二进制自己恰是可能缺席（被杀软隔离等）的东西，指望不上；结构性结论 = 插件内捆绑一个哑 wrapper（找到转发/缺席 exit 2+指引），它与捆绑与否正交，wrapper-only 插件 + PATH 二进制即可先行杀掉失效模式 #2。
- **wrapper 保持哑**：永不解析信封、永不参与裁决——业务逻辑进 wrapper = 在 Rust 引擎之外造第二套不可测引擎。它与 adapter 的分工：wrapper 管装载轴，adapter 管协议轴，hooks.json 管接线轴，三者拼图构成对外兼容面；唯一 agent 耦合点是 wrapper 的解释器契约（各 agent 如何 spawn `command`），属 M7.3 探针项。
