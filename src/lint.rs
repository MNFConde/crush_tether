//! 配置 lint（双层，M2.5）：只告警不拒绝加载（design.md「lint 规则集」）。
//!
//! - **结构类**（无需知识库）：同 token 多桶、同 bin 裸列表与命令节并存、
//!   被 precedence 压死的死词条。
//! - **语义类**（读知识库）：allow `may_write` 命令建议、别名归一后永不命中
//!   的等价冗余、`same_flag` 等价类跨桶冲突、未知子命令拼写提示。
//!
//! lint 对象是**单份文件**（「同文件」语义）；分层合并后的跨层检查由效力
//! 顺序语义天然裁决，不在 lint 范围。拼写提示为配置内互查（槽位封闭集没有
//! 「合法子命令清单」槽位，D-06；知识库有该 bin 的 sub 条目时一并参与比对）。

use std::collections::BTreeMap;

use crate::config::{BucketSpec, DEFAULT_PRECEDENCE, ListField, RulesFile};
use crate::knowledge::{CanonMaps, KnowledgeBase};
use crate::model::Decision;

/// 告警级别：结构错误/死配置 = 告警；改进建议 = 提示。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LintSeverity {
    Warning,
    Suggestion,
}

/// 一条 lint 结果。
#[derive(Debug, Clone)]
pub struct Lint {
    pub severity: LintSeverity,
    /// 机器可读代号（load 事件行/测试断言用）。
    pub code: &'static str,
    pub message: String,
}

/// 一个待检槽位：所属桶 + 维度名 + 该桶该维度下的原始词条。
type Slot<'a> = (Decision, &'a str, Vec<String>);

/// 对一份 `rules.toml` 做双层检查；知识库缺席时自动降级为纯结构检查。
pub fn lint_file(file: &RulesFile, kb: Option<&KnowledgeBase>) -> Vec<Lint> {
    let mut out = Vec::new();
    let precedence = file
        .precedence
        .clone()
        .unwrap_or_else(|| DEFAULT_PRECEDENCE.to_vec());
    let rank = |d: Decision| -> usize {
        precedence
            .iter()
            .position(|x| *x == d)
            .unwrap_or(usize::MAX)
    };
    let canon = kb
        .map(KnowledgeBase::canon_maps)
        .unwrap_or_else(CanonMaps::empty);

    for (scope_label, scope) in [("[local]", &file.local), ("[global]", &file.global)] {
        let head: Vec<Slot<'_>> = vec![
            (
                Decision::Allow,
                "allow",
                tokens(scope.buckets.allow.as_ref()),
            ),
            (
                Decision::Confirm,
                "confirm",
                tokens(scope.buckets.confirm.as_ref()),
            ),
            (Decision::Deny, "deny", tokens(scope.buckets.deny.as_ref())),
        ];

        // 结构类 1/3：同 token 多桶 + 被 precedence 压死的死词条（头部）。
        lint_multi_bucket(&mut out, &head, scope_label, "", "list", &rank);

        // 结构类 2：同 bin 裸列表与命令节并存（节遮蔽裸列表）。
        for bin in scope.commands.keys() {
            let in_head = head
                .iter()
                .any(|(_, _, toks)| toks.iter().any(|t| t == bin));
            if in_head {
                out.push(Lint {
                    severity: LintSeverity::Warning,
                    code: "head-list-and-section",
                    message: format!(
                        "`{bin}` appears both in {scope_label} bare lists and as a \
                         command section; the section shadows the bare-list entry"
                    ),
                });
            }
        }

        // 语义类：命令级（头部 allow may_write；别名等价冗余）。
        for (decision, bucket, toks) in &head {
            for t in toks {
                if *decision == Decision::Allow && may_write_bin(kb, t) {
                    out.push(suggestion(
                        "allow-may-write",
                        format!(
                            "`{t}` is known to possibly write ({scope_label}.{bucket}); \
                             allowing it permits arbitrary writes"
                        ),
                    ));
                }
                if let Some(c) = non_self_canon_bin(&canon, t)
                    && toks.contains(&c)
                {
                    out.push(Lint {
                        severity: LintSeverity::Warning,
                        code: "alias-redundant",
                        message: format!(
                            "`{t}` normalizes to `{c}` which is also listed here; \
                             this entry can never match"
                        ),
                    });
                }
            }
        }

        // 命令节：结构类（多桶/死词条）+ 语义类（flag 跨桶冲突/may_write/拼写）。
        for (bin, section) in &scope.commands {
            let slots: Vec<Slot<'_>> = vec![
                (
                    Decision::Allow,
                    "sub",
                    dims_tokens(section.allow.as_ref(), true),
                ),
                (
                    Decision::Allow,
                    "flag",
                    dims_tokens(section.allow.as_ref(), false),
                ),
                (
                    Decision::Confirm,
                    "sub",
                    dims_tokens(section.confirm.as_ref(), true),
                ),
                (
                    Decision::Confirm,
                    "flag",
                    dims_tokens(section.confirm.as_ref(), false),
                ),
                (
                    Decision::Deny,
                    "sub",
                    dims_tokens(section.deny.as_ref(), true),
                ),
                (
                    Decision::Deny,
                    "flag",
                    dims_tokens(section.deny.as_ref(), false),
                ),
            ];
            for dim in ["sub", "flag"] {
                let group: Vec<Slot<'_>> = slots
                    .iter()
                    .filter(|(_, d, _)| *d == dim)
                    .cloned()
                    .collect();
                lint_multi_bucket(&mut out, &group, scope_label, bin, dim, &rank);
            }

            // 语义类：same_flag 等价类跨桶冲突（归一后规范形只落一个桶，
            // 其余桶词条永不命中且多半违背作者本意）。
            lint_same_flag_cross_bucket(&mut out, &canon, bin, &slots);

            // 语义类：节内 allow.sub 的 may_write 建议。
            for (decision, dim, toks) in &slots {
                if *decision != Decision::Allow || *dim != "sub" {
                    continue;
                }
                for t in toks {
                    if may_write_sub(kb, bin, t) {
                        out.push(suggestion(
                            "allow-may-write",
                            format!(
                                "`{bin} {t}` is known to possibly write \
                                 ({scope_label}.{bin}.allow.sub)"
                            ),
                        ));
                    }
                }
            }

            // 语义类：未知子命令拼写提示（配置内互查 + 知识库 sub 条目）。
            lint_sub_typos(&mut out, kb, scope_label, bin, &slots);
        }
    }
    out
}

/// 同 token 多桶（告警生效桶）+ 非 precedence 首位桶的死词条。
fn lint_multi_bucket(
    out: &mut Vec<Lint>,
    group: &[Slot<'_>],
    scope_label: &str,
    bin: &str,
    dim: &str,
    rank: &impl Fn(Decision) -> usize,
) {
    let mut by_token: BTreeMap<&String, Vec<Decision>> = BTreeMap::new();
    for (decision, _, toks) in group {
        for t in toks {
            by_token.entry(t).or_default().push(*decision);
        }
    }
    for (token, buckets) in &by_token {
        if buckets.len() < 2 {
            continue;
        }
        let where_ = if bin.is_empty() {
            format!("{scope_label}.{dim}")
        } else {
            format!("{scope_label}.{bin}.{dim}")
        };
        let effective = *buckets
            .iter()
            .min_by_key(|d| rank(**d))
            .expect("at least two buckets");
        let listed = buckets
            .iter()
            .map(|d| d.to_string())
            .collect::<Vec<_>>()
            .join("/");
        out.push(Lint {
            severity: LintSeverity::Warning,
            code: "multi-bucket",
            message: format!(
                "`{token}` appears in multiple buckets of {where_} ({listed}); \
                 effective: {effective} (precedence)"
            ),
        });
        for d in buckets {
            if *d != effective {
                out.push(Lint {
                    severity: LintSeverity::Warning,
                    code: "dead-entry",
                    message: format!(
                        "`{token}` in {where_} of {bucket} is dead: shadowed by \
                         `{effective}` via precedence",
                        bucket = d
                    ),
                });
            }
        }
    }
}

/// same_flag 等价类跨桶冲突：同一规范形出现在多个桶 → 冲突告警。
fn lint_same_flag_cross_bucket(
    out: &mut Vec<Lint>,
    canon: &CanonMaps,
    bin: &str,
    slots: &[Slot<'_>],
) {
    let mut by_canon: BTreeMap<String, Vec<Decision>> = BTreeMap::new();
    for (decision, dim, toks) in slots {
        if *dim != "flag" {
            continue;
        }
        for t in toks {
            by_canon
                .entry(canon.canon_flag(bin, t))
                .or_default()
                .push(*decision);
        }
    }
    for (canonical, mut buckets) in by_canon {
        buckets.sort_by_key(|d| d.to_string());
        buckets.dedup();
        if buckets.len() < 2 {
            continue;
        }
        let listed = buckets
            .iter()
            .map(|d| d.to_string())
            .collect::<Vec<_>>()
            .join("/");
        out.push(Lint {
            severity: LintSeverity::Warning,
            code: "same-flag-cross-bucket",
            message: format!(
                "same_flag class of `{canonical}` in [{bin}] spans multiple buckets \
                 ({listed}); only the effective one can ever match after normalization"
            ),
        });
    }
}

/// 未知子命令拼写提示：配置内互查（编辑距离 ≤2）+ 知识库 sub 条目比对。
fn lint_sub_typos(
    out: &mut Vec<Lint>,
    kb: Option<&KnowledgeBase>,
    scope_label: &str,
    bin: &str,
    slots: &[Slot<'_>],
) {
    let mut subs: Vec<&String> = slots
        .iter()
        .filter(|(_, d, _)| *d == "sub")
        .flat_map(|(_, _, toks)| toks.iter())
        .collect();
    subs.sort();
    subs.dedup();

    // 配置内互查：同一节内的子命令词条彼此比对（stauts ≈ status）。
    for (i, a) in subs.iter().enumerate() {
        for b in &subs[i + 1..] {
            let d = edit_distance(a, b);
            if (1..=2).contains(&d) {
                out.push(suggestion(
                    "subcommand-typo",
                    format!("`{a}` in {scope_label}.{bin} looks like a typo of `{b}`"),
                ));
            }
        }
    }

    // 知识库比对：配置词条不在该 bin 的已知 sub 中，但与某个已知 sub 近似。
    let kb_subs: Option<&BTreeMap<String, crate::knowledge::SubEntry>> =
        kb.and_then(|k| k.bins.get(bin)).map(|e| &e.subs);
    if let Some(known) = kb_subs {
        for t in &subs {
            if known.contains_key(t.as_str()) {
                continue;
            }
            if let Some(hit) = known.keys().find(|k| {
                let d = edit_distance(t, k);
                (1..=2).contains(&d)
            }) {
                out.push(suggestion(
                    "subcommand-typo",
                    format!("`{t}` is not a known subcommand of `{bin}`; did you mean `{hit}`?"),
                ));
            }
        }
    }
}

/// 列表值词条（Delta 形态取 add；remove 是删除动作，无命中语义）。
fn tokens(lf: Option<&ListField>) -> Vec<String> {
    match lf {
        None => Vec::new(),
        Some(ListField::Set(v)) => v.clone(),
        Some(ListField::Delta { add, .. }) => add.clone().unwrap_or_default(),
    }
}

fn dims_tokens(spec: Option<&BucketSpec>, sub: bool) -> Vec<String> {
    match spec {
        None => Vec::new(),
        Some(s) => {
            let field = if sub { s.sub.as_ref() } else { s.flag.as_ref() };
            tokens(field)
        }
    }
}

fn may_write_bin(kb: Option<&KnowledgeBase>, bin: &str) -> bool {
    kb.and_then(|k| k.bins.get(bin))
        .and_then(|e| e.may_write)
        .unwrap_or(false)
}

fn may_write_sub(kb: Option<&KnowledgeBase>, bin: &str, sub: &str) -> bool {
    kb.and_then(|k| k.bins.get(bin))
        .and_then(|e| e.subs.get(sub))
        .and_then(|e| e.may_write)
        .unwrap_or(false)
}

fn non_self_canon_bin(canon: &CanonMaps, t: &str) -> Option<String> {
    let c = canon.canon_bin(t);
    (c != t).then_some(c)
}

fn suggestion(code: &'static str, message: String) -> Lint {
    Lint {
        severity: LintSeverity::Suggestion,
        code,
        message,
    }
}

fn edit_distance(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    for (i, ca) in a.iter().enumerate() {
        let mut cur = vec![i + 1];
        for (j, cb) in b.iter().enumerate() {
            let cost = usize::from(ca != cb);
            cur.push((prev[j] + cost).min(prev[j + 1] + 1).min(cur[j] + 1));
        }
        prev = cur;
    }
    prev[b.len()]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(src: &str) -> RulesFile {
        RulesFile::parse_toml(src).expect("fixture parses")
    }

    fn kb(src: &str) -> KnowledgeBase {
        KnowledgeBase::parse_toml(src).expect("kb parses")
    }

    fn codes(lints: &[Lint]) -> Vec<&'static str> {
        lints.iter().map(|l| l.code).collect()
    }

    #[test]
    fn multi_bucket_warns_with_effective_and_dead_entries() {
        let f = file(concat!(
            "version = 1\n",
            "default = \"confirm\"\n",
            "[local]\n",
            "allow = [\"curl\"]\n",
            "confirm = [\"curl\"]\n",
        ));
        let lints = lint_file(&f, None);
        let c = codes(&lints);
        assert!(c.contains(&"multi-bucket"), "{lints:?}");
        assert!(c.contains(&"dead-entry"), "{lints:?}");
        let multi = lints.iter().find(|l| l.code == "multi-bucket").unwrap();
        assert!(
            multi.message.contains("effective: confirm"),
            "{}",
            multi.message
        );
    }

    #[test]
    fn section_dimension_multi_bucket_also_checked() {
        let f = file(concat!(
            "version = 1\n",
            "default = \"confirm\"\n",
            "[local.git]\n",
            "allow.sub = [\"status\"]\n",
            "deny.sub = [\"status\"]\n",
        ));
        let lints = lint_file(&f, None);
        assert!(codes(&lints).contains(&"multi-bucket"), "{lints:?}");
    }

    #[test]
    fn clean_file_has_no_structural_warnings() {
        let f = file(concat!(
            "version = 1\n",
            "default = \"confirm\"\n",
            "[local]\n",
            "allow = [\"ls\"]\n",
            "confirm = [\"curl\"]\n",
            "[local.git]\n",
            "allow.sub = [\"status\"]\n",
            "deny.sub = [\"push\"]\n",
        ));
        assert!(lint_file(&f, None).is_empty());
    }

    #[test]
    fn head_list_and_section_coexistence_warns() {
        let f = file(concat!(
            "version = 1\n",
            "default = \"confirm\"\n",
            "[local]\n",
            "confirm = [\"git\"]\n",
            "[local.git]\n",
            "allow.sub = [\"status\"]\n",
        ));
        let lints = lint_file(&f, None);
        assert!(
            codes(&lints).contains(&"head-list-and-section"),
            "{lints:?}"
        );
    }

    #[test]
    fn allow_may_write_suggests_with_kb_only() {
        let k = kb("version = 1\n[npx]\nmay_write = true");
        let f = file("version = 1\ndefault = \"confirm\"\n[local]\nallow = [\"npx\"]");
        let lints = lint_file(&f, Some(&k));
        assert!(codes(&lints).contains(&"allow-may-write"), "{lints:?}");
        // 无知识库降级：同配置纯结构检查无告警。
        assert!(lint_file(&f, None).is_empty());
    }

    #[test]
    fn alias_redundant_warns_on_never_matching_entry() {
        let k = kb("version = 1\n[pip3]\nalias_of = \"pip\"");
        let f = file("version = 1\ndefault = \"confirm\"\n[local]\nallow = [\"pip\", \"pip3\"]");
        let lints = lint_file(&f, Some(&k));
        assert!(codes(&lints).contains(&"alias-redundant"), "{lints:?}");
        // 单边配置（无冗余）不告警。
        let f2 = file("version = 1\ndefault = \"confirm\"\n[local]\nallow = [\"pip\"]");
        assert!(lint_file(&f2, Some(&k)).is_empty());
    }

    #[test]
    fn same_flag_cross_bucket_conflict_warns() {
        let k = kb("version = 1\n[git]\nflag.\"--force\" = { same_flag = \"-f\" }");
        let f = file(concat!(
            "version = 1\n",
            "default = \"confirm\"\n",
            "[local.git]\n",
            "allow.sub = [\"add\"]\n",
            "confirm.flag = [\"-f\"]\n",
            "allow.flag = [\"--force\"]\n",
        ));
        let lints = lint_file(&f, Some(&k));
        assert!(
            codes(&lints).contains(&"same-flag-cross-bucket"),
            "{lints:?}"
        );
        // 等价类同桶不冲突。
        let f2 = file(concat!(
            "version = 1\n",
            "default = \"confirm\"\n",
            "[local.git]\n",
            "confirm.flag = [\"-f\", \"--force\"]\n",
        ));
        assert!(lint_file(&f2, Some(&k)).is_empty());
    }

    #[test]
    fn subcommand_typo_hint_from_config_internal_pairing() {
        let f = file(concat!(
            "version = 1\n",
            "default = \"confirm\"\n",
            "[local.git]\n",
            "allow.sub = [\"status\"]\n",
            "deny.sub = [\"stauts\"]\n",
        ));
        let lints = lint_file(&f, None);
        let typos: Vec<_> = lints
            .iter()
            .filter(|l| l.code == "subcommand-typo")
            .collect();
        assert_eq!(typos.len(), 1, "{lints:?}");
        assert!(typos[0].message.contains("stauts") && typos[0].message.contains("status"));
    }

    #[test]
    fn subcommand_typo_hint_against_kb_known_subs() {
        let k = kb("version = 1\n[git]\nsub.config = { write_arg_count = 2 }");
        let f = file(concat!(
            "version = 1\n",
            "default = \"confirm\"\n",
            "[local.git]\n",
            "allow.sub = [\"confg\"]\n",
        ));
        let lints = lint_file(&f, Some(&k));
        let typos: Vec<_> = lints
            .iter()
            .filter(|l| l.code == "subcommand-typo")
            .collect();
        assert_eq!(typos.len(), 1, "{lints:?}");
        assert!(
            typos[0].message.contains("did you mean `config`"),
            "{}",
            typos[0].message
        );
        // 无知识库时该用例无提示（降级纯结构检查）。
        assert!(lint_file(&f, None).is_empty());
    }

    #[test]
    fn precedence_from_file_decides_effective_bucket() {
        // 调换 precedence 后生效桶与死词条随之翻转。
        let f = file(concat!(
            "version = 1\n",
            "default = \"confirm\"\n",
            "precedence = [\"allow\", \"confirm\", \"deny\"]\n",
            "[local]\n",
            "allow = [\"curl\"]\n",
            "confirm = [\"curl\"]\n",
        ));
        let lints = lint_file(&f, None);
        let multi = lints.iter().find(|l| l.code == "multi-bucket").unwrap();
        assert!(
            multi.message.contains("effective: allow"),
            "{}",
            multi.message
        );
        let dead = lints.iter().find(|l| l.code == "dead-entry").unwrap();
        assert!(dead.message.contains("of confirm"), "{}", dead.message);
    }

    #[test]
    fn delta_form_add_lists_are_linted() {
        let f = file(concat!(
            "version = 1\n",
            "default = \"confirm\"\n",
            "[local]\n",
            "allow = [\"curl\"]\n",
            "confirm = { add = [\"curl\"] }\n",
        ));
        assert!(codes(&lint_file(&f, None)).contains(&"multi-bucket"));
    }
}
