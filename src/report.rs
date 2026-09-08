//! M7.1 规则测试工具的人读渲染：`explain` 报告、REPL 单条、`--batch`
//! 裁决表与 `--cases` 断言对账共用此处的输出形态。
//!
//! 定位是「人读」：调试者要一眼看到命中层级/桶/token/归一链/写效果扫描/
//! 脚本改判与最终档位——与裁决日志（JSONL，机器面）互补，不重复其字段集。

use crate::model::Decision;
use crate::service::{CommandExplain, ExplainReport};

/// 单命令溯源块（explain 与 REPL 共用）。
pub fn render_command(idx: usize, c: &CommandExplain) -> String {
    let mut out = String::new();
    out.push_str(&format!("[{}] {} => {}\n", idx, c.raw, c.final_decision));
    out.push_str(&format!(
        "    lookup: {} {}\n",
        c.table_decision,
        match &c.table_source {
            Some(s) => format!("<- {}.{} (token: {})", s.layer, s.entry, s.token),
            None => "<- (no hit)".to_string(),
        }
    ));
    if let Some(n) = &c.normalized {
        out.push_str(&format!("    normal: {}\n", n));
    }
    let write_desc = if c.redirect_targets.is_empty() && c.write_scan.is_empty() {
        "no write-effect paths".to_string()
    } else {
        format!("scan = [{}]", c.write_scan.join(", "))
    };
    out.push_str(&format!(
        "    write:  {} (escape check: {})\n",
        write_desc,
        if c.write_escape { "ESCAPES" } else { "pass" }
    ));
    out.push_str(&format!(
        "    script: {}\n",
        match (c.script_changed, c.script_layer) {
            (true, Some(l)) => format!("changed (layer: {l})"),
            (true, None) => "changed".to_string(),
            (false, _) => "-".to_string(),
        }
    ));
    if let Some(r) = &c.reason {
        out.push_str(&format!("    reason: {r}\n"));
    }
    out
}

/// `explain` 全报告。
pub fn render_report(r: &ExplainReport) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "engine: {}  kb: {}  lint: {} warning(s)\n",
        r.engine,
        if r.kb_present { "present" } else { "absent" },
        r.lint.len()
    ));
    if let Some(e) = &r.parse_error {
        out.push_str(&format!(
            "parse error: {e}\ncombined: {}\n",
            r.combined.decision
        ));
        return out;
    }
    if r.commands.is_empty() {
        out.push_str("(empty command)\n");
        return out;
    }
    for (i, c) in r.commands.iter().enumerate() {
        out.push_str(&render_command(i + 1, c));
    }
    out.push_str(&format!("combined: {}\n", r.combined.decision));
    if let Some(reason) = &r.combined.reason {
        out.push_str(&format!("combined reason: {reason}\n"));
    }
    out
}

/// `check --batch` 的单行裁决表条目（DECISION 左对齐 8 列 + 命令 + 原因）。
pub fn render_batch_line(command: &str, v: &crate::model::Verdict) -> String {
    let reason = v.reason.as_deref().unwrap_or("");
    if reason.is_empty() {
        format!("{:<8} {}", decision_tag(v.decision), command)
    } else {
        format!("{:<8} {}  ; {}", decision_tag(v.decision), command, reason)
    }
}

fn decision_tag(d: Decision) -> &'static str {
    match d {
        Decision::Allow => "ALLOW",
        Decision::Confirm => "CONFIRM",
        Decision::Deny => "DENY",
    }
}

/// `check --cases` 的单条对账行。
pub fn render_case_line(pass: bool, command: &str, expect: Decision, actual: Decision) -> String {
    if pass {
        format!("PASS  {:<8} {}", decision_tag(actual), command)
    } else {
        format!(
            "FAIL  {:<8} {}  (expected {})",
            decision_tag(actual),
            command,
            expect
        )
    }
}
