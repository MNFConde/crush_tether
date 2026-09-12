//! suggest 命令（M8.6 权限建议，D-10 定位 = 用户打磨配置文件的手段）：
//! 读裁决日志与执行记录交叉推断「用户反复人工批准且执行成功的命令」，
//! stdout 输出可粘贴进 rules.toml 的建议块 + 摘要 + 跳过清单。**零写入**
//! ——人工审阅后自行粘贴（git diff 即回滚面与审计面）；查表无「学习条
//! 目」特例，写入即普通规则。
//!
//! 交叉推断（保守三条件，缺一不成为候选）：
//! 1. 裁决 = confirm（deny 永不学习）；
//! 2. 执行成功（`executions.jsonl` `success == true`；判不出 = null 不算）；
//! 3. 窗口内同触发原因重复 ≥ threshold（默认 3）。
//!
//! 关联：主键 `session_id + tool_use_id`（decisions/executions 两表都记）；
//! 兜底 = 命令原文相等 + 时间窗（execution 晚于 decision 且 ≤10min）。
//!
//! 建议生成（保守收窄）：
//! - entry cause（查表 confirm 条目）：反推 `allow.sub` / `allow.flag` /
//!   bin 级 allow 条目；
//! - script cause：跳过（脚本规则无 TOML 对应物，须人工改脚本）；
//! - whole cause：仅当示例命令可收窄为 bin+sub 且余参全 flag 时建议；
//!   否则跳过（自由参数）。
//! - 跳过类 bin（即便反复批准也不建议）：知识库 `may_write` + 网络/不可
//!   逆硬清单（curl/wget/rm/pip 等）。

use std::collections::HashMap;
use std::path::Path;

/// 硬编码跳过清单：网络下载类与不可逆类 bin（知识库 `may_write` 之外的
/// 兜底；设计定稿四类跳过之二三）。即便反复批准也不建议入 allow。
const SKIP_BINS: &[&str] = &["curl", "wget", "rm", "pip", "pip3", "sudo", "dd"];

/// 兜底关联的时间窗（execution 晚于 decision 且在此秒数内视为同次执行）。
const FALLBACK_WINDOW_SECS: u64 = 600;

/// suggest 运行选项（CLI `--threshold` / `--window` / `--format`）。
pub struct SuggestOptions {
    /// 候选门槛：窗口内重复次数（默认 3）。
    pub threshold: usize,
    /// 统计窗口天数（默认 30）。
    pub window_days: u64,
    /// 输出形态：`toml`（建议块+摘要）或 `table`（仅人读表）。
    pub format: String,
}

struct ExecRow {
    ts_epoch: u64,
    session_id: Option<String>,
    tool_use_id: Option<String>,
    command: String,
    success: Option<bool>,
}

/// RFC3339 UTC（`YYYY-MM-DDTHH:MM:SSZ`）→ epoch 秒（日志 ts 字典序即时间
/// 序，但窗口运算需要数值）。解析失败返回 None（该行不参与窗口过滤时
/// 保守保留 → 用 `u64::MAX` 兜底语义由调用方决定；此处 None = 丢弃）。
fn parse_rfc3339(s: &str) -> Option<u64> {
    let b = s.as_bytes();
    if b.len() != 20
        || b[4] != b'-'
        || b[7] != b'-'
        || (b[10] != b'T' && b[10] != b' ')
        || b[13] != b':'
        || b[16] != b':'
        || (b[19] != b'Z')
    {
        return None;
    }
    let num = |r: std::ops::Range<usize>| -> Option<u64> { s[r].parse::<u64>().ok() };
    let (y, mo, d) = (num(0..4)?, num(5..7)?, num(8..10)?);
    let (hh, mm, ss) = (num(11..13)?, num(14..16)?, num(17..19)?);
    // days_from_civil（Hinnant 逆变换）。
    let y = y as i64 - if mo <= 2 { 1 } else { 0 };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (mo as i64 + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d as i64 - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468;
    Some((days * 86_400 + hh as i64 * 3_600 + mm as i64 * 60 + ss as i64) as u64)
}

fn read_jsonl(path: &Path) -> Vec<serde_json::Value> {
    let Ok(s) = std::fs::read_to_string(path) else {
        return Vec::new();
    };
    s.lines()
        .filter_map(|l| serde_json::from_str::<serde_json::Value>(l).ok())
        .collect()
}

fn jstr<'a>(v: &'a serde_json::Value, key: &str) -> Option<&'a str> {
    v.get(key).and_then(serde_json::Value::as_str)
}

/// 从候选 cause（聚合计数）生成建议：Some((bin, 建议条目描述)) 或 None + 跳过原因。
fn build_suggestion(
    kb_may_write: &dyn Fn(&str) -> bool,
    kind: &str,
    key: &str,
    sample: &str,
) -> Result<(String, String), String> {
    match kind {
        "script" => Err("脚本规则触发（无 TOML 对应物；如需放行请改脚本/规则）".into()),
        "entry" => {
            let parts: Vec<&str> = key.split('\u{1f}').collect();
            let Some((layer, entry, token)) = (match parts.as_slice() {
                [l, e, t] => Some((*l, *e, *t)),
                _ => None,
            }) else {
                return Err("溯源键不完整".into());
            };
            let _ = layer;
            if entry == "confirm" {
                // 裸列表整命令命中 → 建议 bin 级 allow。
                if skip_bin(kb_may_write, token) {
                    return Err(format!("`{token}` 属危险类别（may_write/网络/不可逆）"));
                }
                Ok((String::new(), format!("allow = [\"{token}\"]")))
            } else if let Some(bin) = entry.strip_suffix(".confirm.sub") {
                if skip_bin(kb_may_write, bin) {
                    return Err(format!("`{bin}` 属危险类别（may_write/网络/不可逆）"));
                }
                Ok((bin.to_string(), format!("allow.sub = [\"{token}\"]")))
            } else if let Some(bin) = entry.strip_suffix(".confirm.flag") {
                if skip_bin(kb_may_write, bin) {
                    return Err(format!("`{bin}` 属危险类别（may_write/网络/不可逆）"));
                }
                Ok((bin.to_string(), format!("allow.flag = [\"{token}\"]")))
            } else {
                Err("溯源条目非 confirm 桶（不值得放行）".into())
            }
        }
        "whole" => {
            // 兜底 cause：仅当示例可收窄为 bin+sub 且余参全 flag 时建议。
            let words: Vec<&str> = sample.split_whitespace().collect();
            let Some(bin) = words.first() else {
                return Err("空命令".into());
            };
            if skip_bin(kb_may_write, bin) {
                return Err(format!("`{bin}` 属危险类别（may_write/网络/不可逆）"));
            }
            match words.get(1) {
                Some(sub)
                    if !sub.starts_with('-') && words[2..].iter().all(|w| w.starts_with('-')) =>
                {
                    Ok((bin.to_string(), format!("allow.sub = [\"{sub}\"]")))
                }
                _ => Err("含自由参数且无安全子命令可收窄".into()),
            }
        }
        _ => Err("未知原因类别".into()),
    }
}

fn skip_bin(kb_may_write: &dyn Fn(&str) -> bool, bin: &str) -> bool {
    SKIP_BINS.contains(&bin) || kb_may_write(bin)
}

/// suggest 主入口。返回进程 exit code（恒 0，除非 IO 异常——学习面绝不
/// 报错打扰裁决路径）。
pub fn run(project: &Path, opts: &SuggestOptions) -> i32 {
    let exec_path = project.join(".crush-tether").join("executions.jsonl");
    if !exec_path.exists() {
        println!("no execution records found for this project;");
        println!("learning signal requires an agent that emits PostToolUse events");
        println!("(claude: full, zcode: degraded, crush: unavailable) and runs with");
        println!("the hook installed. nothing to suggest.");
        return 0;
    }

    // 知识库 may_write 查询（知识库缺席 = 只用硬清单）。
    let kb = std::fs::read_to_string(project.join(".crush-tether").join("knowledge.toml"))
        .ok()
        .and_then(|t| crate::knowledge::KnowledgeBase::parse_toml(&t).ok());
    let kb_may_write = |bin: &str| -> bool {
        kb.as_ref()
            .and_then(|k| k.bins.get(bin))
            .and_then(|e| e.may_write)
            .unwrap_or(false)
    };

    // 窗口下限。
    let now = now_epoch();
    let cutoff = now.saturating_sub(opts.window_days * 86_400);

    // 执行记录（窗口内）。
    let execs: Vec<ExecRow> = read_jsonl(&exec_path)
        .into_iter()
        .filter_map(|v| {
            let ts = jstr(&v, "ts")?;
            Some(ExecRow {
                ts_epoch: parse_rfc3339(ts)?,
                session_id: jstr(&v, "session_id").map(String::from),
                tool_use_id: jstr(&v, "tool_use_id").map(String::from),
                command: jstr(&v, "command").unwrap_or_default().to_string(),
                success: v.get("success").and_then(serde_json::Value::as_bool),
            })
        })
        .filter(|e| e.ts_epoch >= cutoff)
        .collect();

    // 主键索引 + 兜底索引（command → 成功 execution 列表）。
    let mut by_key: HashMap<(String, String), &ExecRow> = HashMap::new();
    let mut by_cmd: HashMap<String, Vec<&ExecRow>> = HashMap::new();
    for e in &execs {
        if e.success != Some(true) {
            continue;
        }
        if let (Some(s), Some(t)) = (&e.session_id, &e.tool_use_id) {
            by_key.insert((s.clone(), t.clone()), e);
        }
        by_cmd.entry(e.command.clone()).or_default().push(e);
    }
    for v in by_cmd.values_mut() {
        v.sort_by_key(|e| e.ts_epoch);
    }

    // confirm 裁决行 → 关联执行成功 → cause 计数。
    let dec_path = project.join(".crush-tether").join("decisions.jsonl");
    let mut counts: HashMap<(String, String), (usize, String)> = HashMap::new();
    for v in read_jsonl(&dec_path) {
        if jstr(&v, "decision") != Some("confirm") {
            continue; // deny/allow 永不学习。
        }
        let ts = match jstr(&v, "ts").and_then(parse_rfc3339) {
            Some(t) if t >= cutoff => t,
            _ => continue,
        };
        let command = jstr(&v, "command").unwrap_or_default().to_string();
        // 关联：主键优先，兜底命令原文 + 时间窗。
        let matched = match (jstr(&v, "session_id"), jstr(&v, "tool_use_id")) {
            (Some(s), Some(t)) => by_key.contains_key(&(s.to_string(), t.to_string())),
            _ => by_cmd.get(&command).is_some_and(|list| {
                list.iter()
                    .any(|e| e.ts_epoch >= ts && e.ts_epoch <= ts + FALLBACK_WINDOW_SECS)
            }),
        };
        if !matched {
            continue;
        }
        // cause 提取（与裁决时 causes_of_components 同构，从日志字段重建）。
        let script_rule = v
            .pointer("/script/rule")
            .and_then(serde_json::Value::as_str)
            .map(String::from);
        let source: Option<(String, String, String)> =
            v.get("source").filter(|s| !s.is_null()).and_then(|s| {
                Some((
                    jstr(s, "layer")?.to_string(),
                    jstr(s, "entry")?.to_string(),
                    jstr(s, "match")?.to_string(),
                ))
            });
        let (kind, key) = cause_of(&script_rule, &source, &command);
        let e = counts
            .entry((kind.to_string(), key))
            .or_insert((0, command));
        e.0 += 1;
    }

    // 候选 = 计数 ≥ threshold。
    let mut candidates: Vec<((String, String), usize, String)> = counts
        .iter()
        .filter(|(_, (n, _))| *n >= opts.threshold)
        .map(|(k, (n, sample))| (k.clone(), *n, sample.clone()))
        .collect();
    candidates.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));

    // 生成建议 / 跳过清单。
    let mut suggestions: Vec<(String, String, usize)> = Vec::new(); // (bin, 条目, 次数)
    let mut skipped: Vec<(String, usize, String)> = Vec::new(); // (概要, 次数, 原因)
    for ((kind, key), n, sample) in &candidates {
        match build_suggestion(&kb_may_write, kind, key, sample) {
            Ok((bin, entry)) => suggestions.push((bin, entry, *n)),
            Err(reason) => skipped.push((sample.clone(), *n, reason.to_string())),
        }
    }

    // 输出。
    let table = |out: &mut String| {
        out.push_str(&format!(
            "{:<6}  {:<7}  {:<6}  {}\n",
            "count", "kind", "bin", "cause / suggestion"
        ));
        for ((kind, key), n, sample) in &candidates {
            let suggestion = match build_suggestion(&kb_may_write, kind, key, sample) {
                Ok((bin, e)) => format!("{bin} {e}"),
                Err(reason) => format!("SKIP: {reason}"),
            };
            out.push_str(&format!(
                "{:<6}  {:<7}  {:<6}  {}\n",
                n,
                kind,
                sample.split_whitespace().next().unwrap_or("?"),
                suggestion
            ));
        }
    };
    let mut out = String::new();
    if suggestions.is_empty() {
        out.push_str(&format!(
            "no suggestions (threshold = {} in the last {} day(s));\n",
            opts.threshold, opts.window_days
        ));
        if !skipped.is_empty() {
            out.push_str("some repeated approvals were skipped (see below).\n");
        }
    } else if opts.format == "toml" {
        out.push_str(&format!(
            "# crush-tether suggest: rules derived from {} repeated approvals\n",
            suggestions.len()
        ));
        out.push_str("# paste into .crush-tether/rules.toml (git diff = rollback & audit);\n");
        out.push_str(
            "# review each entry: repeated approval does not always mean safe to allow.\n",
        );
        // 按 bin 分组输出。
        let mut by_bin: HashMap<String, Vec<String>> = HashMap::new();
        for (bin, entry, _) in &suggestions {
            by_bin.entry(bin.clone()).or_default().push(entry.clone());
        }
        let mut bins: Vec<&String> = by_bin.keys().collect();
        bins.sort();
        for bin in bins {
            if bin.is_empty() {
                for e in &by_bin[bin] {
                    out.push_str(&format!("{e}\n"));
                }
            } else {
                out.push_str(&format!("\n[local.{bin}]\n"));
                for e in &by_bin[bin] {
                    out.push_str(&format!("{e}\n"));
                }
            }
        }
        out.push('\n');
    }
    if !candidates.is_empty() {
        table(&mut out);
    }
    if !skipped.is_empty() {
        out.push_str("\nskip list (repeatedly approved, but not suggested):\n");
        for (sample, n, reason) in &skipped {
            out.push_str(&format!("  {sample}  (x{n})  {reason}\n"));
        }
    }
    print!("{out}");
    0
}

fn now_epoch() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// 日志字段 → cause（与裁决时 [`crate::service::causes_of_components`]
/// 同构；从 decisions.jsonl 行重建）。
fn cause_of(
    script_rule: &Option<String>,
    source: &Option<(String, String, String)>,
    command: &str,
) -> (String, String) {
    if let Some(r) = script_rule {
        return ("script".into(), r.clone());
    }
    if let Some((layer, entry, token)) = source
        && layer != "script"
        && layer != "default"
        && entry != "default"
        && !entry.ends_with(".default")
    {
        return ("entry".into(), format!("{layer}\u{1f}{entry}\u{1f}{token}"));
    }
    ("whole".into(), command.to_string())
}
