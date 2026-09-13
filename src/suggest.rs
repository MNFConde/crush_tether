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
//!
//! 建议判定以类型化形态（[`Suggestion`]/[`SkipReason`]）供两个消费方共
//! 用（M10.1）：本命令（中文文案）与 repl/explain 调试提示
//! （[`suggestion_line`]，英文文案、单信号——无重复门槛/执行成功条件，
//! 定位是「放行面参考」而非学习结论）。

use std::collections::{BTreeMap, HashMap};
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

/// 建议产出（类型化）：判定与文案分离——suggest 命令渲染中文跳过原因，
/// repl/explain 调试提示渲染英文行（M10.1）。
pub(crate) enum Suggestion {
    /// 可粘贴条目：`section` = rules.toml 表头路径（`local.terraform` /
    /// `local`；裸桶统一落 `[local]`，逃逸出口落 `[global]`——被命中条目
    /// 的作用域未入溯源键，默认保守落 local，逃逸语义另见 [`SkipReason`]）。
    Allow {
        /// rules.toml 表头路径。
        section: String,
        /// 表内条目行（如 `allow.sub = ["plan"]`）。
        entry: String,
    },
    /// 不建议 + 结构化原因。
    Skip(SkipReason),
}

/// 不建议的结构化原因（两个消费方各自渲染文案）。
pub(crate) enum SkipReason {
    /// 危险类别（硬清单 / 知识库 may_write / irreversible）。
    Dangerous(String),
    /// 脚本规则触发（无 TOML 对应物）。
    ScriptRule,
    /// 含自由参数且无安全 bin+sub 可收窄。
    FreeArgs,
    /// 溯源条目非 confirm 桶。
    NotConfirmEntry,
    /// allow 桶命中被写逃逸降级 confirm（M7.0）——放行出口是 [global]
    /// （豁免逃逸检查），不是镜像 allow 条目。
    EscapedWrite,
    /// 溯源键不完整或类别未知 / 空命令。
    Malformed,
}

/// 从候选 cause 生成类型化建议：判定逻辑唯一权威，suggest 命令与调试
/// 提示各取所需。
fn build_suggestion_typed(
    skip_fact: &dyn Fn(&str) -> bool,
    kind: &str,
    key: &str,
    sample: &str,
) -> Suggestion {
    match kind {
        "script" => Suggestion::Skip(SkipReason::ScriptRule),
        "entry" => {
            let parts: Vec<&str> = key.split('\u{1f}').collect();
            let [_layer, entry, token] = parts.as_slice() else {
                return Suggestion::Skip(SkipReason::Malformed);
            };
            // 写逃逸降级（M7.0）：confirm 裁决 + allow 桶溯源 = allow 命中
            // 被写逃逸降级——建议出口是 [global]，不是同位 allow（那会继续
            // 被逃逸检查降级，形成"批了还是问"的死循环）。
            if *entry == "allow" || entry.ends_with(".allow.sub") || entry.ends_with(".allow.flag")
            {
                return Suggestion::Skip(SkipReason::EscapedWrite);
            }
            if *entry == "confirm" {
                // 裸列表整命令命中 → 建议 bin 级 allow。
                if skip_fact(token) {
                    return Suggestion::Skip(SkipReason::Dangerous((*token).to_string()));
                }
                return Suggestion::Allow {
                    section: "local".to_string(),
                    entry: format!("allow = [\"{token}\"]"),
                };
            }
            if let Some(bin) = entry.strip_suffix(".confirm.sub") {
                if skip_fact(bin) {
                    return Suggestion::Skip(SkipReason::Dangerous(bin.to_string()));
                }
                return Suggestion::Allow {
                    section: format!("local.{bin}"),
                    entry: format!("allow.sub = [\"{token}\"]"),
                };
            }
            if let Some(bin) = entry.strip_suffix(".confirm.flag") {
                if skip_fact(bin) {
                    return Suggestion::Skip(SkipReason::Dangerous(bin.to_string()));
                }
                return Suggestion::Allow {
                    section: format!("local.{bin}"),
                    entry: format!("allow.flag = [\"{token}\"]"),
                };
            }
            Suggestion::Skip(SkipReason::NotConfirmEntry)
        }
        "whole" => {
            // 兜底 cause：仅当示例可收窄为 bin+sub 且余参全 flag 时建议。
            // sub 须词形似子命令（字母数字开头，仅字母数字/-/_）——`jq .`
            // 的 `.`、`cat f.txt` 的路径等不是子命令，收窄即跳过（M10.1
            // 收紧：调试提示与 suggest 命令同口径）。
            let words: Vec<&str> = sample.split_whitespace().collect();
            let Some(bin) = words.first() else {
                return Suggestion::Skip(SkipReason::Malformed);
            };
            if skip_fact(bin) {
                return Suggestion::Skip(SkipReason::Dangerous((*bin).to_string()));
            }
            match words.get(1) {
                Some(sub)
                    if plausible_sub(sub) && words[2..].iter().all(|w| w.starts_with('-')) =>
                {
                    Suggestion::Allow {
                        section: format!("local.{bin}"),
                        entry: format!("allow.sub = [\"{sub}\"]"),
                    }
                }
                _ => Suggestion::Skip(SkipReason::FreeArgs),
            }
        }
        _ => Suggestion::Skip(SkipReason::Malformed),
    }
}

/// 子命令词形：非空、字母数字开头、仅字母数字/-/_（排除 `.`、路径、
/// `--flag` 等不可作子命令的词元）。
fn plausible_sub(t: &str) -> bool {
    let mut chars = t.chars();
    matches!(chars.next(), Some(c) if c.is_ascii_alphanumeric())
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

/// suggest 命令的中文文案渲染（保留既有输出措辞）。
fn skip_reason_zh(r: SkipReason) -> String {
    match r {
        SkipReason::Dangerous(b) => format!("`{b}` 属危险类别（may_write/网络/不可逆）"),
        SkipReason::ScriptRule => "脚本规则触发（无 TOML 对应物；如需放行请改脚本/规则）".into(),
        SkipReason::FreeArgs => "含自由参数且无安全子命令可收窄".into(),
        SkipReason::NotConfirmEntry => "溯源条目非 confirm 桶（不值得放行）".into(),
        SkipReason::EscapedWrite => {
            "allow 命中被写逃逸降级（如确属有意，移入 [global] 豁免逃逸检查）".into()
        }
        SkipReason::Malformed => "溯源键不完整或未知类别".into(),
    }
}

/// 项目侧跳过判据（suggest 命令与 repl/explain 调试提示共用，M10.1）：
/// 硬清单 ∨ 知识库 may_write/irreversible。知识库缺席 = 只用硬清单。
pub fn kb_skip_fact(project: &Path) -> impl Fn(&str) -> bool {
    let kb = std::fs::read_to_string(project.join(".crush-tether").join("knowledge.toml"))
        .ok()
        .and_then(|t| crate::knowledge::KnowledgeBase::parse_toml(&t).ok());
    move |bin| {
        SKIP_BINS.contains(&bin)
            || kb
                .as_ref()
                .and_then(|k| k.bins.get(bin))
                .is_some_and(|e| e.may_write.unwrap_or(false) || e.irreversible.unwrap_or(false))
    }
}

/// allow 桶形态的查表溯源（裸 `allow` / 命令节 `*.allow.sub|flag`）。
fn table_source_is_allow(src: &Option<crate::lookup::EntrySource>) -> bool {
    src.as_ref().is_some_and(|s| {
        s.entry == "allow" || s.entry.ends_with(".allow.sub") || s.entry.ends_with(".allow.flag")
    })
}

/// repl/explain 调试提示行（M10.1 放行面参考）：confirm 裁决追加一行
/// 「怎么放行」或「为何不建议」；allow/deny 不打行。定位是**单信号参考**
/// （无 suggest 的重复门槛与执行成功条件），输出纯信息、零写入；跳过
/// 判据与 suggest 命令同源（[`kb_skip_fact`]）——不建议的口径完全一致。
pub(crate) fn suggestion_line(
    c: &crate::service::CommandExplain,
    skip_fact: &dyn Fn(&str) -> bool,
) -> Option<String> {
    if c.final_decision != crate::model::Decision::Confirm {
        return None;
    }
    // 逃逸降级优先直判（活路径有 write_escape 真值，不必借 cause 反推）。
    if c.write_escape && table_source_is_allow(&c.table_source) {
        return Some(format!(
            "suggestion: none — write target is outside the project; if intended, \
             move `{}` to [global] allow (escape-exempt)",
            c.bin
        ));
    }
    let (kind, key) = crate::service::cause_of_explain(c);
    match build_suggestion_typed(skip_fact, kind, &key, &c.raw) {
        Suggestion::Allow { section, entry } => Some(format!(
            "suggestion: [{section}] {entry}   (paste into rules.toml; review first)"
        )),
        Suggestion::Skip(reason) => {
            let en = match &reason {
                SkipReason::Dangerous(b) => {
                    format!("`{b}` is in the dangerous skip list (may_write/network/irreversible)")
                }
                SkipReason::ScriptRule => {
                    "script rule triggered (edit the script; no TOML counterpart)".to_string()
                }
                SkipReason::FreeArgs => "no safe bin+sub narrowing (free-form args)".to_string(),
                SkipReason::NotConfirmEntry => {
                    "source entry is not a confirm-bucket hit".to_string()
                }
                SkipReason::EscapedWrite => "write target is outside the project; if intended, \
                     move to [global] allow (escape-exempt)"
                    .to_string(),
                SkipReason::Malformed => "incomplete trace key".to_string(),
            };
            Some(format!("suggestion: none — {en}"))
        }
    }
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

    // 永久跳过判据（M10.1 起共用 [`kb_skip_fact`]：硬清单 + may_write +
    // 命令级 irreversible）。
    let skip_fact = kb_skip_fact(project);

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
    let mut suggestions: Vec<(String, String, usize)> = Vec::new(); // (表头路径, 条目, 次数)
    let mut skipped: Vec<(String, usize, String)> = Vec::new(); // (概要, 次数, 原因)
    for ((kind, key), n, sample) in &candidates {
        match build_suggestion_typed(&skip_fact, kind, key, sample) {
            Suggestion::Allow { section, entry } => suggestions.push((section, entry, *n)),
            Suggestion::Skip(reason) => skipped.push((sample.clone(), *n, skip_reason_zh(reason))),
        }
    }

    // 输出。
    let table = |out: &mut String| {
        out.push_str(&format!(
            "{:<6}  {:<7}  {:<6}  {}\n",
            "count", "kind", "bin", "cause / suggestion"
        ));
        for ((kind, key), n, sample) in &candidates {
            let suggestion = match build_suggestion_typed(&skip_fact, kind, key, sample) {
                Suggestion::Allow { section, entry } => format!("[{section}] {entry}"),
                Suggestion::Skip(reason) => format!("SKIP: {}", skip_reason_zh(reason)),
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
        // 按表头路径分组输出（M10.1：section 统一为 local / local.git /
        // global…，裸桶也带 [local] 表头——粘贴即用）。
        let mut by_section: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for (section, entry, _) in &suggestions {
            by_section
                .entry(section.clone())
                .or_default()
                .push(entry.clone());
        }
        for (section, entries) in &by_section {
            out.push_str(&format!("\n[{section}]\n"));
            for e in entries {
                out.push_str(&format!("{e}\n"));
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lookup::EntrySource;
    use crate::model::Decision;
    use crate::service::CommandExplain;

    /// 最小 CommandExplain（调试提示用例只关心 source/script_rule/raw/
    /// final_decision/write_escape/bin）。
    fn cx(
        raw: &str,
        bin: &str,
        source: Option<EntrySource>,
        script_rule: Option<&str>,
        final_decision: Decision,
        write_escape: bool,
    ) -> CommandExplain {
        CommandExplain {
            raw: raw.to_string(),
            bin: bin.to_string(),
            table_decision: Decision::Confirm,
            table_source: source,
            normalized: None,
            writes_redirect: false,
            redirect_targets: Vec::new(),
            write_scan: Vec::new(),
            write_escape,
            script_changed: false,
            script_layer: None,
            script_rule: script_rule.map(String::from),
            final_decision,
            reason: None,
        }
    }

    /// 单测判据：仅硬清单（不读文件系统）。
    fn hard_list_skip(bin: &str) -> bool {
        SKIP_BINS.contains(&bin)
    }

    fn src(layer: &'static str, entry: &str, token: &str) -> Option<EntrySource> {
        Some(EntrySource {
            layer,
            entry: entry.to_string(),
            token: token.to_string(),
        })
    }

    #[test]
    fn default_confirm_with_safe_narrowing_suggests_allow_sub() {
        let c = cx(
            "terraform plan",
            "terraform",
            None,
            None,
            Decision::Confirm,
            false,
        );
        let line = suggestion_line(&c, &hard_list_skip).expect("confirm 必有行");
        assert!(
            line.contains("suggestion: [local.terraform] allow.sub = [\"plan\"]"),
            "{line}"
        );
        assert!(
            line.contains("(paste into rules.toml; review first)"),
            "{line}"
        );
    }

    #[test]
    fn dangerous_bin_suggests_nothing_with_reason() {
        let c = cx(
            "rm tmp.txt",
            "rm",
            src("project", "confirm", "rm"),
            None,
            Decision::Confirm,
            false,
        );
        let line = suggestion_line(&c, &hard_list_skip).expect("confirm 必有行");
        assert!(
            line.contains(
                "suggestion: none — `rm` is in the dangerous skip list \
                 (may_write/network/irreversible)"
            ),
            "{line}"
        );
    }

    #[test]
    fn free_args_skip_when_no_safe_narrowing() {
        // default 兜底（entry=default → whole）+ 首参非 flag → 收窄失败。
        let c = cx(
            "jq .",
            "jq",
            src("project", "default", "jq"),
            None,
            Decision::Confirm,
            false,
        );
        let line = suggestion_line(&c, &hard_list_skip).expect("confirm 必有行");
        assert!(
            line.contains("suggestion: none — no safe bin+sub narrowing (free-form args)"),
            "{line}"
        );
    }

    #[test]
    fn script_rule_trigger_prints_script_reason() {
        let c = cx(
            "git branch -d x",
            "git",
            src("script", "script", "git"),
            Some("two_state:-d"),
            Decision::Confirm,
            false,
        );
        let line = suggestion_line(&c, &hard_list_skip).expect("confirm 必有行");
        assert!(
            line.contains(
                "suggestion: none — script rule triggered (edit the script; \
                 no TOML counterpart)"
            ),
            "{line}"
        );
    }

    #[test]
    fn escape_downgrade_points_to_global_allow() {
        // allow 命中（source 仍指 allow 桶）+ 写逃逸 → [global] 出口提示。
        let c = cx(
            "touch ../outside.txt",
            "touch",
            src("project", "allow", "touch"),
            None,
            Decision::Confirm,
            true,
        );
        let line = suggestion_line(&c, &hard_list_skip).expect("confirm 必有行");
        assert!(
            line.contains(
                "suggestion: none — write target is outside the project; if intended, \
                 move `touch` to [global] allow (escape-exempt)"
            ),
            "{line}"
        );
    }

    #[test]
    fn allow_and_denial_have_no_suggestion_line() {
        let ok = cx(
            "cp a b",
            "cp",
            src("project", "allow", "cp"),
            None,
            Decision::Allow,
            false,
        );
        assert!(suggestion_line(&ok, &hard_list_skip).is_none());
        let no = cx(
            "git push",
            "git",
            src("project", "git.deny.sub", "push"),
            None,
            Decision::Deny,
            false,
        );
        assert!(suggestion_line(&no, &hard_list_skip).is_none());
    }

    #[test]
    fn confirm_flag_entry_suggests_allow_flag() {
        let c = cx(
            "git log --output=x",
            "git",
            src("project", "git.confirm.flag", "--output"),
            None,
            Decision::Confirm,
            false,
        );
        let line = suggestion_line(&c, &hard_list_skip).expect("confirm 必有行");
        assert!(
            line.contains("suggestion: [local.git] allow.flag = [\"--output\"]"),
            "{line}"
        );
    }

    #[test]
    fn head_list_confirm_suggests_local_section_entry() {
        let c = cx(
            "jq --args x",
            "jq",
            src("project", "confirm", "jq"),
            None,
            Decision::Confirm,
            false,
        );
        let line = suggestion_line(&c, &hard_list_skip).expect("confirm 必有行");
        assert!(
            line.contains("suggestion: [local] allow = [\"jq\"]"),
            "{line}"
        );
    }

    #[test]
    fn kb_irreversible_fact_participates_in_skip() {
        // 硬清单之外的 bin 由知识库事实（may_write/irreversible）触发跳过。
        let c = cx(
            "mytool do-x",
            "mytool",
            None,
            None,
            Decision::Confirm,
            false,
        );
        let line = suggestion_line(&c, &|b: &str| b == "mytool").expect("confirm 必有行");
        assert!(
            line.contains(
                "suggestion: none — `mytool` is in the dangerous skip list \
                 (may_write/network/irreversible)"
            ),
            "{line}"
        );
    }
}
