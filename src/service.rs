//! P4 服务层（M4.1）：命名端点传输 + serve 单实例 + hook connect-or-spawn。
//!
//! 语义单一事实源：`doc/design.md`「serve 模式协议（命名端点，一项目一实例）」：
//!
//! - **端点名** `crush-tether-<hash(canonical(project_dir), engine标签)>`；
//!   一项目一 serve，同项目所有 agent/会话共用。
//! - **独占 bind = 单实例裁定**：serve 启动第一动作即独占创建端点，同一
//!   syscall 同步裁定唯一性——成功 = 本项目唯一服务；失败 = 已存在 → 本进程
//!   静默退出（输者转 connect 重试，非报错退出）。Windows 侧经
//!   `FILE_FLAG_FIRST_PIPE_INSTANCE`（interprocess 默认不回收名字）、Unix
//!   abstract socket 经 `EADDRINUSE` 实现。
//! - **协议**：JSON 行，`{id, op:"check", command}` / `{id, op:"ping"}` →
//!   `{id, verdict:{decision, reason} | null, error?}`；连接生命周期 =
//!   一次请求，无长连接池。
//! - **v1 串行 accept**：`accept → 读 → 判 → 写` 单循环；「连接计数」退化为
//!   `last_activity` 时间戳；空闲超 `--idle-exit`（默认 30s）由 watchdog
//!   线程整秒醒一次退出（无 busy-loop）。
//! - **安全**：Windows 管道默认 DACL（当前用户）/ Unix socket 0600；同用户
//!   伪造请求最多把危险命令转人工确认，无可放大面。
//! - **降级**：hook 连不上（含 spawn + ~200ms 有界重试失败）→ 本进程降级
//!   check，绝不无裁决放行；`CRUSH_TETHER_DISABLE_SERVE=1` 强制走降级路径
//!   （用户逃生口 + 测试钩子）。
//! - **热重载（M4.2）**：notify 监听配置目录 + 600ms debounce（编辑器
//!   temp-rename 连发事件聚成一次），整段重编译 → `Arc` 原子换指针（在途
//!   请求继续用旧快照，无半更新）；重载失败保留旧快照 + stderr 告警；监听
//!   失效降级逐请求 stat（mtime + size + 内容 hash 三重校验），正确性不损。

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::lookup::RuleLookup;
use crate::model::{Decision, Verdict};
use crate::script::RuleEngine;

/// 端点名：`crush-tether-<16hex(hash(canonical(project), engine))>`。
/// DefaultHasher::new() 定种（SipHash 固定 key），跨进程稳定。
pub fn endpoint_name(project: &Path, engine: &str) -> String {
    let canon = project
        .canonicalize()
        .unwrap_or_else(|_| project.to_path_buf());
    let mut h = DefaultHasher::new();
    canon.to_string_lossy().hash(&mut h);
    engine.hash(&mut h);
    format!("crush-tether-{:016x}", h.finish())
}

// ── 协议 DTO ─────────────────────────────────────────────────────────────

/// 请求行：`{id, op:"check", command, agent}` / `{id, op:"ping"}`。
#[derive(Debug, Serialize, Deserialize)]
pub struct RequestLine {
    pub id: u64,
    pub op: String,
    #[serde(default)]
    pub command: String,
    #[serde(default)]
    pub agent: String,
}

/// 响应行：`verdict = None` 表示无裁决（ping 应答）。
#[derive(Debug, Serialize, Deserialize)]
pub struct ResponseLine {
    pub id: u64,
    #[serde(default)]
    pub verdict: Option<VerdictDto>,
    #[serde(default)]
    pub error: Option<String>,
}

/// 裁决传输形态（decision ∈ allow/confirm/deny）。
#[derive(Debug, Serialize, Deserialize)]
pub struct VerdictDto {
    pub decision: String,
    #[serde(default)]
    pub reason: Option<String>,
}

fn parse_decision(s: &str) -> Option<Decision> {
    match s {
        "allow" => Some(Decision::Allow),
        "confirm" => Some(Decision::Confirm),
        "deny" => Some(Decision::Deny),
        _ => None,
    }
}

// ── RuleSet：可复用的「查表 + 脚本 + 定稿点」装配 ─────────────────────────

/// 装配完成的规则快照（M4.2 起被 `Arc` 包裹原子换指针）。
pub struct RuleSet {
    lookup: RuleLookup,
    script: Option<crate::script::RhaiEngine>,
    /// 知识库 main 是否在位（日志 `kb` 字段：[] = 删光自证）。
    pub kb_present: bool,
    /// 项目层 rules.toml 的 lint 告警（`type:"load"` 事件行内容）。
    pub lint_warnings: Vec<crate::lint::Lint>,
}

/// 单命令裁决的溯源信息（日志 source/normalized/script 字段数据源）。
#[derive(Debug, Default, Clone)]
pub struct DecisionTrace {
    pub source: Option<crate::lookup::EntrySource>,
    pub normalized: Option<String>,
    pub script_changed: bool,
}

impl RuleSet {
    /// 加载配置（三层发现 + 引导生成 + 显式覆盖）与脚本层；任一损坏返回
    /// `Err(完整告警消息)`——调用方按 fail-safe confirm 处理（D-03：损坏 ≠
    /// 缺失，绝不静默回落）。
    pub fn load(
        project: &Path,
        _engine_label: &str,
        config_arg: Option<&str>,
    ) -> Result<RuleSet, String> {
        let home = crate::config::home_dir();
        let found = crate::config::discover_layers(Some(project), home.as_deref());
        // 三层皆缺 → 引导生成默认包（生成动作是管线引导步骤，不经规则链，
        // design.md「零内置策略与默认配置生成」）。任一层损坏（found 为 Err）
        // 时不生成：损坏 ≠ 缺失（D-03），fail-safe confirm、原文件不动。
        let found = match found {
            Ok(l) if l.all_absent() && crate::config::explicit_path(config_arg).is_none() => {
                match crate::config::seed::seed_defaults_if_absent(project) {
                    Ok(_) => {
                        eprintln!(
                            "crush-tether: no config found; seeded defaults in {}",
                            project.join(".crush-tether").display()
                        );
                        crate::config::discover_layers(Some(project), home.as_deref())
                    }
                    Err(e) => {
                        eprintln!(
                            "crush-tether: seeding default config failed: {e}; continuing \
                             without config (fail-safe confirm)"
                        );
                        Ok(l)
                    }
                }
            }
            other => other,
        };
        let kb = found.as_ref().ok().and_then(|l| l.knowledge.as_ref());
        let lookup = if let Some(path) = crate::config::explicit_path(config_arg) {
            match crate::config::load_file(&path) {
                Ok(f) => RuleLookup::new(
                    crate::config::merge(crate::config::Layers {
                        global: None,
                        user: None,
                        project: Some(&f),
                    }),
                    kb,
                ),
                Err(e) => {
                    return Err(format!(
                        "crush-tether: explicit config {} failed to load: {e}; fail-safe \
                         confirm",
                        path.display()
                    ));
                }
            }
        } else {
            match &found {
                Ok(l) => RuleLookup::new(
                    crate::config::merge(crate::config::Layers::from_found(l)),
                    kb,
                ),
                Err(e) => {
                    return Err(format!(
                        "crush-tether: config failed to load: {e}; fail-safe confirm"
                    ));
                }
            }
        };

        // 脚本层：项目 rules.rhai（缺失 = 无脚本层，TOML 自足）。编译/加载
        // 失败（含 script_allow 对账拒载）必须告警 + fail-safe confirm。
        let script = match crate::script::load_project_script(
            project,
            kb.map(|k| Arc::new(k.clone())),
            lookup.script_allow().clone(),
        ) {
            Ok(s) => s,
            Err(e) => {
                return Err(format!(
                    "crush-tether: rules.rhai failed to load: {e}; fail-safe confirm"
                ));
            }
        };
        // lint（M2.5 双层）：项目层 rules.toml + 脚本提取集；只告警不拒绝，
        // 告警进 serve 的 `type:"load"` 事件行。
        let lint_warnings = match &found {
            Ok(l) => match (&l.project, &script) {
                (Some(f), Some(s)) => crate::lint::lint_file(f, kb, s.allow_literals()),
                (Some(f), None) => crate::lint::lint_file(f, kb, &[]),
                _ => Vec::new(),
            },
            Err(_) => Vec::new(),
        };
        Ok(RuleSet {
            lookup,
            script,
            kb_present: kb.is_some(),
            lint_warnings,
        })
    }

    /// 完整单命令管线：解析拉平 → 查表 → 脚本 → 定稿点 → 组合裁决。
    pub fn decide(&self, command: &str, project: &Path) -> Verdict {
        self.decide_trace(command, project).0
    }

    /// 裁决 + 溯源（日志 source/normalized/script 字段）。
    pub fn decide_trace(&self, command: &str, project: &Path) -> (Verdict, DecisionTrace) {
        use std::cell::RefCell;
        let trace = RefCell::new(DecisionTrace::default());
        let verdict =
            crate::engine::decide_with(command, project, &|cmd, project, pipe_to_shell| {
                let c0 = self.lookup.classify_traced(cmd, project);
                let v0 = c0.verdict;
                {
                    let mut t = trace.borrow_mut();
                    if t.source.is_none() {
                        t.source = c0.source.clone();
                    }
                    if t.normalized.is_none() && !c0.kb_chain.is_empty() {
                        t.normalized = Some(c0.kb_chain.join(" -> "));
                    }
                }
                let (decision, reason) = match &self.script {
                    Some(script) => {
                        match script.evaluate(cmd, v0.decision, project, pipe_to_shell) {
                            // 定稿点：deny 终审 + allow 激活作用域化逃逸检查的唯一出口。
                            Ok(outcome) => {
                                let activate =
                                    matches!(outcome, crate::script::ScriptOutcome::Activate(_));
                                let (d, r) = crate::script::finalize(
                                    v0.decision,
                                    outcome,
                                    script.decls(),
                                    cmd,
                                    project,
                                );
                                // script 字段自证：激活或改判（含 deny 终审拦截）留痕。
                                if activate || d != v0.decision {
                                    trace.borrow_mut().script_changed = true;
                                }
                                (d, r)
                            }
                            Err(e) => {
                                eprintln!(
                                    "crush-tether: script evaluation failed: {e}; fail-safe confirm"
                                );
                                (
                                    Decision::Confirm,
                                    Some("script evaluation failed; fail-safe".into()),
                                )
                            }
                        }
                    }
                    None => (v0.decision, None),
                };
                let script_changed = trace.borrow().script_changed;
                match (reason, decision != v0.decision) {
                    (Some(reason), true) => Verdict {
                        decision,
                        reason: Some(reason),
                    },
                    (Some(reason), false) => Verdict {
                        decision,
                        reason: v0.reason.or(Some(reason)),
                    },
                    (None, _) => Verdict {
                        decision,
                        reason: if script_changed {
                            Some("adjusted by rules.rhai".into())
                        } else {
                            v0.reason
                        },
                    },
                }
            });
        (verdict, trace.into_inner())
    }
}

// ── 裁决日志（M4.3，ADR-07：默认开）───────────────────────────────────────

/// 日志开关（默认开；`CRUSH_TETHER_LOG=0|off|false` 关闭）。
pub fn log_enabled() -> bool {
    match std::env::var("CRUSH_TETHER_LOG") {
        Ok(v) => !matches!(v.as_str(), "0" | "off" | "false"),
        Err(_) => true,
    }
}

/// UTC RFC3339 时间戳（std 无本地时区能力；人读视图由 log 子命令渲染，挂账）。
fn rfc3339_utc() -> String {
    let d = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    let secs = d.as_secs();
    let days = secs / 86_400;
    let rem = secs % 86_400;
    let (hh, mm, ss) = (rem / 3_600, rem % 3_600 / 60, rem % 60);
    // civil_from_days：epoch 起的天数 → 年月日（Hinnant 算法）。
    let z = days as i64 + 719_468;
    let era = z.div_euclid(146_097);
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let dom = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = if month <= 2 { y + 1 } else { y };
    format!("{year:04}-{month:02}-{dom:02}T{hh:02}:{mm:02}:{ss:02}Z")
}

/// 追加一行 JSONL（写入失败静默：日志永不影响裁决路径）。
fn log_jsonl(project: &Path, value: &serde_json::Value) {
    if !log_enabled() {
        return;
    }
    let path = project.join(".crush-tether").join("decisions.jsonl");
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
    {
        use std::io::Write as _;
        let mut line = value.to_string();
        line.push('\n');
        let _ = f.write_all(line.as_bytes());
    }
}

/// 裁决日志记录（design.md「日志」示例字段全集）。
pub fn log_verdict(
    project: &Path,
    mode: &str,
    agent: &str,
    command: &str,
    verdict: &Verdict,
    trace: &DecisionTrace,
    kb_present: bool,
) {
    let source = trace.source.as_ref().map(|s| {
        let file = match s.layer {
            "project" => ".crush-tether/rules.toml",
            "user" => "~/.config/crush-tether/rules.toml",
            _ => "",
        };
        serde_json::json!({
            "layer": s.layer,
            "file": if file.is_empty() {
                serde_json::Value::Null
            } else {
                serde_json::json!(file)
            },
            "entry": s.entry,
            "match": s.token,
        })
    });
    let rec = serde_json::json!({
        "ts": rfc3339_utc(),
        "mode": mode,
        "agent": agent,
        "command": command,
        "decision": verdict.decision.to_string(),
        "reason": verdict.reason,
        "source": source,
        "kb": if kb_present { serde_json::json!(["main"]) } else { serde_json::json!([]) },
        "normalized": trace.normalized,
        "script": {
            "file": if trace.script_changed { serde_json::json!("rules.rhai") } else { serde_json::Value::Null },
            "rule": serde_json::Value::Null,
        },
    });
    log_jsonl(project, &rec);
}

/// `type:"load"` 事件行：serve 冷启动/热重载留痕，含 lint 告警。
pub fn log_load_event(project: &Path, ruleset: Option<&RuleSet>) {
    let (lint, kb) = match ruleset {
        Some(rs) => (
            rs.lint_warnings
                .iter()
                .map(|w| serde_json::json!({"code": w.code, "message": w.message}))
                .collect::<Vec<_>>(),
            rs.kb_present,
        ),
        None => (Vec::new(), false),
    };
    let rec = serde_json::json!({
        "ts": rfc3339_utc(),
        "type": "load",
        "kb": if kb { serde_json::json!(["main"]) } else { serde_json::json!([]) },
        "lint": lint,
    });
    log_jsonl(project, &rec);
}

// ── 传输层 ───────────────────────────────────────────────────────────────

fn ns_name<'a>(name: &'a str) -> std::io::Result<interprocess::local_socket::Name<'a>> {
    use interprocess::local_socket::{GenericNamespaced, ToNsName};
    name.to_ns_name::<GenericNamespaced>()
}

/// 独占创建端点（单实例裁定）。`Err` = 已存在（或不可绑定）。
fn bind(name: &str) -> std::io::Result<interprocess::local_socket::Listener> {
    use interprocess::local_socket::ListenerOptions;
    let opts = ListenerOptions::new().name(ns_name(name)?);
    #[cfg(unix)]
    let opts = opts.mode(0o600); // ACL：仅当前用户可连
    // Windows：默认 DACL 即当前用户；reclaim_name=false（默认）→ 首次创建
    // 带 FILE_FLAG_FIRST_PIPE_INSTANCE，二次创建失败 = 输者。
    opts.create_sync()
}

fn connect(name: &str) -> std::io::Result<interprocess::local_socket::Stream> {
    use interprocess::local_socket::traits::Stream as _;
    interprocess::local_socket::Stream::connect(ns_name(name)?)
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

// ── serve 角色 ───────────────────────────────────────────────────────────

// v1 串行 accept：裁决只发生在主线程，规则快照以本地所有权持有，
// 重载即整体替换（无半更新）。并发版（epoll/IOCP）升级点：换
// Arc<RwLock<Arc<RuleSet>>> 原子换指针——rhai Engine 非 Send，届时启用
// rhai sync feature（普通注释：非 item 文档）。
/// debounce 窗口（design.md「配置加载与热重载」：600ms）。
const RELOAD_DEBOUNCE: Duration = Duration::from_millis(600);

/// serve 消费的配置文件全集（热重载监听与 stat 降级校验的同一清单）。
fn config_files(project: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    if let Some(home) = crate::config::home_dir() {
        files.push(home.join(".config").join("crush-tether").join("rules.toml"));
    }
    for f in ["rules.toml", "rules.rhai", "knowledge.toml"] {
        files.push(project.join(".crush-tether").join(f));
    }
    files
}

fn watch_dirs(project: &Path) -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    let pd = project.join(".crush-tether");
    if pd.is_dir() {
        dirs.push(pd);
    }
    if let Some(home) = crate::config::home_dir() {
        let ud = home.join(".config").join("crush-tether");
        if ud.is_dir() {
            dirs.push(ud);
        }
    }
    dirs
}

/// 整段重编译 + 整体替换；失败保留旧快照 + stderr 告警，绝不半更新。
fn reload(ruleset: &mut Option<RuleSet>, project: &Path, engine: &str) {
    match RuleSet::load(project, engine, None) {
        Ok(rs) => *ruleset = Some(rs),
        Err(msg) => {
            eprintln!("crush-tether: hot reload failed; keeping previous snapshot: {msg}");
        }
    }
}

/// 配置指纹：mtime + size + 内容 hash 三重校验（监听失效的降级判据）。
fn config_fingerprint(project: &Path) -> u64 {
    let mut h = DefaultHasher::new();
    for f in config_files(project) {
        match std::fs::metadata(&f) {
            Ok(md) => {
                f.hash(&mut h);
                md.len().hash(&mut h);
                if let Ok(m) = md.modified() {
                    m.hash(&mut h);
                }
                if let Ok(bytes) = std::fs::read(&f) {
                    bytes.hash(&mut h);
                }
            }
            // 文件消失也是状态变化。
            Err(_) => u8::MAX.hash(&mut h),
        }
    }
    h.finish()
}

/// 事件监听线程：notify 监听 + 600ms debounce → 发一次重载信号。信号由
/// serve 主线程在请求间隙消费（RuleSet 含 rhai Engine、非 Send，重编译
/// 必须留在主线程）。监听建不起来或事件通道断开 → alive 清零，serve
/// 逐请求降级 stat 校验。
fn spawn_watcher(project: PathBuf, alive: Arc<AtomicU64>) -> std::sync::mpsc::Receiver<()> {
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        use notify::Watcher as _;
        let (tx_ev, rx_ev) = std::sync::mpsc::channel();
        let mut watcher = match notify::recommended_watcher(tx_ev) {
            Ok(w) => w,
            Err(_) => {
                alive.store(0, Ordering::Relaxed);
                return;
            }
        };
        let dirs = watch_dirs(&project);
        if dirs.is_empty() {
            alive.store(0, Ordering::Relaxed);
            return;
        }
        for d in &dirs {
            if watcher
                .watch(d, notify::RecursiveMode::NonRecursive)
                .is_err()
            {
                alive.store(0, Ordering::Relaxed);
                return;
            }
        }
        while rx_ev.recv().is_ok() {
            // debounce：等写入稳定，并排空连发事件后再发一次信号。
            std::thread::sleep(RELOAD_DEBOUNCE);
            while rx_ev.try_recv().is_ok() {}
            if tx.send(()).is_err() {
                break; // serve 已结束
            }
        }
        // 事件通道断开（watcher 失效）→ 降级 stat 校验。
        alive.store(0, Ordering::Relaxed);
    });
    rx
}

/// serve 主循环：独占 bind → 装配规则快照 → 串行 accept。输者静默退出
/// （ExitCode 0，无输出）。
pub fn serve_main(project: PathBuf, engine: String, idle_exit: Duration) -> std::process::ExitCode {
    let name = endpoint_name(&project, &engine);
    let listener = match bind(&name) {
        Ok(l) => l,
        // 已有实例（或绑定失败）→ 本进程是惊群输者：静默退出，由 hook 客户端
        // 重试连接赢家。
        Err(_) => return std::process::ExitCode::from(0),
    };

    // 规则快照：加载失败保持存活、逐请求 fail-safe confirm（绝不放行）。
    // 冷启动 load 事件行留痕（ADR-07）。
    let mut ruleset: Option<RuleSet> = match RuleSet::load(&project, &engine, None) {
        Ok(rs) => Some(rs),
        Err(msg) => {
            eprintln!("{msg}");
            None
        }
    };
    log_load_event(&project, ruleset.as_ref());

    // 热重载：notify 监听 + debounce（watcher 线程只发信号；重载在主线程
    // 的请求间隙执行——RuleSet 含 rhai Engine、非 Send）；alive 清零时
    // 逐请求 stat 降级。
    let watcher_alive = Arc::new(AtomicU64::new(1)); // 1 = 监听在位；0 = 降级 stat
    let reload_sig = spawn_watcher(project.clone(), watcher_alive.clone());

    let last_activity = Arc::new(AtomicU64::new(now_ms()));

    // watchdog：整秒醒一次，空闲超 grace 即退出（在途请求 ≤5s 有界，不会
    // 超过 grace；接受但静默的连接同样被 grace 回收）。
    {
        let last = last_activity.clone();
        let idle_ms = idle_exit.as_millis() as u64;
        std::thread::spawn(move || {
            loop {
                std::thread::sleep(Duration::from_secs(1));
                if now_ms().saturating_sub(last.load(Ordering::Relaxed)) >= idle_ms {
                    std::process::exit(0);
                }
            }
        });
    }

    // v1 串行 accept：accept → 读 → 判 → 写。
    loop {
        use interprocess::local_socket::traits::Listener as _;
        match listener.accept() {
            Ok(stream) => {
                last_activity.store(now_ms(), Ordering::Relaxed);
                // 热重载：消费 debounce 后的重载信号，主线程整段重编译 +
                // 原子换指针（串行设计无在途并发请求，天然无半更新）。
                let mut dirty = false;
                while reload_sig.try_recv().is_ok() {
                    dirty = true;
                }
                if dirty {
                    reload(&mut ruleset, &project, &engine);
                }
                // 监听失效降级：逐请求 stat 三重校验，指纹变化才整段重载。
                if watcher_alive.load(Ordering::Relaxed) == 0 {
                    static LAST_FP: AtomicU64 = AtomicU64::new(0);
                    let fp = config_fingerprint(&project);
                    if LAST_FP.load(Ordering::Relaxed) != 0 && LAST_FP.load(Ordering::Relaxed) != fp
                    {
                        reload(&mut ruleset, &project, &engine);
                    }
                    LAST_FP.store(fp, Ordering::Relaxed);
                }
                handle_connection(&stream, ruleset.as_ref(), &project, &last_activity);
            }
            Err(_) => {
                // 瞬时 accept 错误：短暂退避，避免错误风暴变 busy-loop。
                std::thread::sleep(Duration::from_millis(50));
            }
        }
    }
}

/// 单连接处理：一次请求一连接。规则快照缺失 → fail-safe confirm 应答。
fn handle_connection(
    stream: &interprocess::local_socket::Stream,
    ruleset: Option<&RuleSet>,
    project: &Path,
    last_activity: &AtomicU64,
) {
    let respond = |mut stream: &interprocess::local_socket::Stream,
                   resp: &ResponseLine|
     -> std::io::Result<()> {
        let mut line = serde_json::to_string(resp)?;
        line.push('\n');
        stream.write_all(line.as_bytes())
    };
    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    if reader.read_line(&mut line).is_err() || line.trim().is_empty() {
        return;
    }
    let req = match serde_json::from_str::<RequestLine>(line.trim()) {
        Ok(r) => r,
        Err(_) => {
            let _ = respond(
                stream,
                &ResponseLine {
                    id: 0,
                    verdict: None,
                    error: Some("malformed request".into()),
                },
            );
            return;
        }
    };
    let verdict = match req.op.as_str() {
        "ping" => None,
        "check" => match ruleset {
            Some(rs) => {
                let (v, trace) = rs.decide_trace(&req.command, project);
                // serve 单点写裁决日志（ADR-07）。
                log_verdict(
                    project,
                    "serve",
                    if req.agent.is_empty() {
                        "unknown"
                    } else {
                        &req.agent
                    },
                    &req.command,
                    &v,
                    &trace,
                    rs.kb_present,
                );
                Some(VerdictDto {
                    decision: v.decision.to_string(),
                    reason: v.reason,
                })
            }
            // 规则快照缺失：fail-safe confirm（绝不放行）。
            None => Some(VerdictDto {
                decision: "confirm".into(),
                reason: Some("serve rule set unavailable; fail-safe".into()),
            }),
        },
        other => {
            let _ = respond(
                stream,
                &ResponseLine {
                    id: req.id,
                    verdict: None,
                    error: Some(format!("unknown op `{other}`")),
                },
            );
            last_activity.store(now_ms(), Ordering::Relaxed);
            return;
        }
    };
    last_activity.store(now_ms(), Ordering::Relaxed);
    let _ = respond(
        stream,
        &ResponseLine {
            id: req.id,
            verdict,
            error: None,
        },
    );
}

// ── hook 客户端角色 ──────────────────────────────────────────────────────

/// 单次请求应答（一连接一请求）。
fn ask(project: &Path, engine: &str, agent: &str, command: &str) -> Option<Verdict> {
    let stream = connect(&endpoint_name(project, engine)).ok()?;
    let req = RequestLine {
        id: 1,
        op: "check".into(),
        command: command.to_string(),
        agent: agent.to_string(),
    };
    let mut line = serde_json::to_string(&req).ok()?;
    line.push('\n');
    let mut stream = stream;
    stream.write_all(line.as_bytes()).ok()?;
    let mut resp_line = String::new();
    BufReader::new(&stream).read_line(&mut resp_line).ok()?;
    let resp: ResponseLine = serde_json::from_str(resp_line.trim()).ok()?;
    let v = resp.verdict?;
    let decision = parse_decision(&v.decision)?;
    Some(Verdict {
        decision,
        reason: v.reason,
    })
}

/// 端点存活探测（测试/客户端等待就绪用）。
pub fn ping(project: &Path, engine: &str) -> bool {
    let Ok(stream) = connect(&endpoint_name(project, engine)) else {
        return false;
    };
    let Ok(mut req) = serde_json::to_string(&RequestLine {
        id: 1,
        op: "ping".into(),
        command: String::new(),
        agent: String::new(),
    }) else {
        return false;
    };
    req.push('\n');
    let mut stream = stream;
    if stream.write_all(req.as_bytes()).is_err() {
        return false;
    }
    let mut resp_line = String::new();
    if BufReader::new(&stream).read_line(&mut resp_line).is_err() {
        return false;
    }
    serde_json::from_str::<ResponseLine>(resp_line.trim())
        .map(|r| r.error.is_none())
        .unwrap_or(false)
}
/// detached spawn serve（输者语义由 serve 自身处理：静默退出 0）。
fn spawn_serve(project: &Path, engine: &str) {
    let Ok(exe) = std::env::current_exe() else {
        return;
    };
    let idle_secs = std::env::var("CRUSH_TETHER_IDLE_EXIT")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(30);
    let mut cmd = std::process::Command::new(exe);
    cmd.args([
        "serve",
        "--project",
        &project.to_string_lossy(),
        "--engine",
        engine,
        "--idle-exit",
        &idle_secs.to_string(),
    ])
    .stdin(std::process::Stdio::null())
    .stdout(std::process::Stdio::null())
    .stderr(std::process::Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const DETACHED_PROCESS: u32 = 0x0000_0008;
        const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP | CREATE_NO_WINDOW);
    }
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        cmd.process_group(0);
    }
    let _ = cmd.spawn();
}

/// hook 主路径：connect-or-spawn → 仍失败返回 `None`（调用方降级本进程
/// check，绝不无裁决放行）。`CRUSH_TETHER_DISABLE_SERVE=1` 跳过（逃生口）。
pub fn hook_decide(project: &Path, engine: &str, agent: &str, command: &str) -> Option<Verdict> {
    if std::env::var_os("CRUSH_TETHER_DISABLE_SERVE").is_some() {
        return None;
    }
    // 直连常驻 serve（µs 级）。
    if let Some(v) = ask(project, engine, agent, command) {
        return Some(v);
    }
    // 冷启动惊群：spawn serve（独占 bind 裁定唯一性，输者静默退出），有界
    // 等就绪重试 ~200ms。
    spawn_serve(project, engine);
    let t0 = Instant::now();
    while t0.elapsed() < Duration::from_millis(200) {
        std::thread::sleep(Duration::from_millis(20));
        if let Some(v) = ask(project, engine, agent, command) {
            return Some(v);
        }
    }
    None
}
