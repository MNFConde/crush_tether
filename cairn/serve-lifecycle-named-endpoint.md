---
type: project_topic
status: active
summary: serve 生命周期与命名端点单实例的定稿结论：使用驱动生命周期、一项目一实例、独占 bind 裁定角色、内核事件感知连接。
tags: [crush_tether, architecture, ipc]
contains: [decision, lesson]
created: 2026-09-04
updated: 2026-09-04
related: [doc/design.md]
authoring_mode: ai_generated
---
# serve 生命周期与命名端点单实例

## 形成背景

设计 serve 常驻模式时讨论「能否随 agent 进程启动/关闭」，发现精确耦合不可行，逐轮推演出使用驱动生命周期与命名端点方案。定稿正文见 `doc/design.md`「运行模式与配置热重载（定稿）」。

## 当前结论

- **生命周期绑定使用而非进程**（sccache 模式）：hook 进程 connect-or-spawn（首条命令 ≈ 随 agent 启动）；serve 在途归零 + idle 超 grace 自动退出（≈ 随 agent 关闭，延迟 ≤ grace）。残留代价：≤ 一个 grace 时长的 <5MB 零 CPU 进程，可忽略。
- **精确「随 agent 生灭」不可行的原因**：Crush 仅支持 PreToolUse（无会话事件）；ClaudeCode SessionEnd 在 crash/kill 时不触发（孤儿进程）；父子进程信号需按平台 API 探测 agent pid（pidfd / OpenProcess + 启动时间校验防 PID 复用），复杂度远超收益；多会话共用 serve 时「随某一 agent 关闭而关闭」本身是错误语义。
- **一项目一 serve**：端点名 = `hash(canonical(project_dir), engine 标签)`；配置/热重载/裁决域按项目天然隔离，进程内无需多项目缓存与逐出；同项目所有 agent/会话共用同一 serve（裁决与 agent 无关）。
- **单实例 = 独占 bind 一个 syscall 裁定两件事**：唯一性（bind 失败 = 已有服务）与角色（输者不是报错退出，而是静默转 connect 去用服务）——connect-or-spawn 自愈能力由此而来。并发冷启动惊群同步消解，无锁无 pidfile。
- **连接感知全靠内核事件**：连接 = accept / ConnectNamedPipe 完成；断开 = read 得 EOF / ERROR_BROKEN_PIPE。本机端点无 TCP 式半开连接（同机进程死 = 内核关 fd = 对端立即 EOF），无需心跳。进程崩溃 = fd 全关 = 计数天然归零，无陈旧状态清理逻辑。
- **连接计数退化为时间戳（v1）**：连接生命周期 = 一次请求（hook 是短命进程），v1 串行 accept 只需 `last_activity` 时间戳，退出条件 = 空闲超 grace；poll timeout 设为距退出 deadline 的剩余时间，到点醒一次即退出，其余零唤醒。

## 经验与教训

- **教训：传输层选型前先核实宿主进程的 fd 继承行为**。初稿「bash 客户端壳 + 进程替换持 fd」方案在评审时才发现 Crush（Go 实现）子进程仅传 std 三件套（fd 全 CLOEXEC），且每个 PreToolUse hook 都是全新 bash，跨调用共享 fd 的前提不成立。本机命名端点不受此限。
- **Windows named pipe 客户端标准模式**：`CreateFile` 得 `ERROR_PIPE_BUSY` → `WaitNamedPipe` 重试后重连。
- **崩溃残留端点分平台**：Windows 管道与 Linux abstract socket 活在内核命名空间，进程死即消失，天然免疫；文件系统 socket（macOS）会留死文件，需「bind 失败但 connect ECONNREFUSED → unlink + rebind」有界重试。
- **安全评估：伪造请求不可放大**：端点 ACL 限当前用户；同用户伪造最多把危险命令转 confirm（安全侧），无可放大面。

## 决策记录

| 决策 | 结论 | 状态 |
|---|---|---|
| serve 生命周期 | 使用驱动（connect-or-spawn + idle 退出），不耦合 agent 进程 | 定稿 |
| ClaudeCode SessionEnd 主动回收 | 仅加速回收，不覆盖 crash，不做正确性依赖 | 备选 |
| 客户端壳 + bash 进程替换持 fd | fd 全 CLOEXEC + hook 每次全新 bash，前提不成立 | 已否决 |
| serve 传输 | 本机命名端点（pipe / unix socket，优先 abstract namespace），否决 localhost TCP 与 stdout 行协议 | 定稿 |
| 实例粒度 | 一项目一 serve（端点名 hash(项目根, engine)）；全局单 serve + 请求路由 + LRU 逐出 | 当前 / 备选 |
| engine 维度入端点名 | `--engine rhai|lua` 为 CLI 参数期间，端点名必须带 engine 标签；或 engine 挪进项目配置 | 定稿（待二选一） |
