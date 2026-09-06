//! rules.toml 查表引擎（草案 v1）：知识库归一 + 双表三桶 + 多命中合成。
//!
//! 语义单一事实源：`doc/design.md`「rules.toml 结构（草案）」查表顺序条：
//!
//! 0. 归一先行：查表前按知识库把命令改写到规范形（`pip3` → `pip`、
//!    `npm exec foo` → `npx foo`、`--force` → `-f`）；只做名字改写，绝不做
//!    语义变换（参数值/顺序/结构不动，路径逃逸检查仍用原始参数）。
//! 1. `[global].allow` 命中 → 整命令 allow（豁免路径逃逸检查；两表皆现时
//!    `[global]` 优先——全局放行是更强的承诺）。
//! 2. 命令节优先：同层内裸列表词条被命令节遮蔽；跨层 `[global]` 优先。
//! 3. 节内多维度命中（sub / flag 各自合法命中不同桶）按 `precedence` 有序
//!    合成出唯一裁决（D-04：`git show --output=x` 中 `show` 命中 allow.sub、
//!    `--output` 命中 confirm.flag → confirm）。
//! 4. `[local]` 的一切 allow 命中带路径逃逸检查（local 的承诺是「效果不出
//!    项目」）；`[global]` allow 豁免。
//! 5. 未命中 → 节内 `default` → 顶层 `default` → confirm（fail-safe，恒链尾）。
//!
//! 知识库两侧规范形化：命令侧逐词元归一（takes_value 的值词元不参与 flag
//! 匹配），配置侧在构造时展开到规范形——`same_flag` 等价类单边配置双边生效。
//! 无知识库时一切按字面查表（「删光 = 判定不受影响」）。

use std::collections::BTreeMap;
use std::path::Path;

use crate::cmd_parse::{SimpleCommand, path_escapes};
use crate::config::{Dims, MergedCommand, MergedRules, MergedScope};
use crate::knowledge::{CanonMaps, KnowledgeBase};
use crate::model::{Decision, Verdict};

/// 查表裁决器：持有合并后生效规则的规范形副本与知识库规范形映射。
pub struct RuleLookup {
    rules: MergedRules,
    precedence: [Decision; 3],
    canon: CanonMaps,
}

/// 单命令查表结果：裁决 + 归一链（kb 日志字段；空 = 归一未生效）。
pub struct Classification {
    pub verdict: Verdict,
    pub kb_chain: Vec<String>,
}

/// 归一后的命令名字面（仅供查表；原始参数语义不变）。
struct Normalized {
    bin: String,
    /// 规范后的子命令；子命令别名命中时被目标 bin 吸收（None）。
    sub: Option<String>,
    /// flag 候选（规范形；takes_value 的值词元已剥除）。
    flags: Vec<String>,
    /// 归一链（发生改写才从首词元记起；空 = 归一未生效）。
    chain: Vec<String>,
}

impl RuleLookup {
    /// `precedence` 已由解析校验（三桶排列）或合并回落默认序，恒为 3 项。
    pub fn new(rules: MergedRules, kb: Option<&KnowledgeBase>) -> Self {
        let canon = kb
            .map(KnowledgeBase::canon_maps)
            .unwrap_or_else(CanonMaps::empty);
        let precedence = {
            let p = &rules.precedence;
            [p[0], p[1], p[2]]
        };
        RuleLookup {
            rules: canonicalize_rules(rules, &canon),
            precedence,
            canon,
        }
    }

    /// 查表裁决单条简单命令。
    pub fn classify(&self, cmd: &SimpleCommand, project: &Path) -> Verdict {
        self.classify_traced(cmd, project).verdict
    }

    /// script_allow 声明集（脚本引擎对账与定稿点作用域判据的数据源）。
    pub fn script_allow(&self) -> &crate::config::merge::ScriptAllowDecls {
        &self.rules.script_allow
    }

    /// 裁决 + 归一链（P4 裁决日志 `kb` 字段的数据源）。
    pub fn classify_traced(&self, cmd: &SimpleCommand, project: &Path) -> Classification {
        let Some(bin0) = cmd.bin() else {
            return Classification {
                verdict: Verdict::confirm("empty command"),
                kb_chain: Vec::new(),
            };
        };
        let norm = self.normalize(bin0, cmd);
        let verdict = self.lookup(&norm, cmd, project);
        Classification {
            verdict,
            kb_chain: norm.chain,
        }
    }

    /// 归一：链式改写到规范形（加载期已防环；只改名字，不动参数）。
    fn normalize(&self, bin0: &str, cmd: &SimpleCommand) -> Normalized {
        let mut chain: Vec<String> = Vec::new();
        let mut bin = bin0.to_string();
        let mut sub = cmd.args().first().cloned();
        loop {
            // 子命令别名优先（bin+子命令 → 目标 bin，子命令槽位被吸收），
            // 其次命令别名（子命令原样保留）。
            let hop = if let Some(s) = sub.clone()
                && let Some(t) = self.canon.sub_alias.get(&(bin.clone(), s))
            {
                (Some(t.clone()), true)
            } else if let Some(t) = self.canon.bin.get(&bin) {
                (Some(t.clone()), false)
            } else {
                (None, false)
            };
            let (Some(next), sub_absorbed) = hop else {
                break;
            };
            if chain.is_empty() {
                chain.push(bin.clone());
            }
            if sub_absorbed {
                sub = None;
            }
            bin = next;
            chain.push(bin.clone());
        }
        let args = cmd.args().get(1..).unwrap_or(&[]);
        let flags = self.flag_bases(&bin, args);
        Normalized {
            bin,
            sub,
            flags,
            chain,
        }
    }

    /// flag 候选词元：剥值（`--output=x` / `-o x` / `-oX`）、规范形化。
    /// takes_value 的分离值词元不作为 flag 候选（归一不丢值，也不误判值）。
    fn flag_bases(&self, bin: &str, args: &[String]) -> Vec<String> {
        let mut out = Vec::new();
        let mut skip_next = false;
        for a in args {
            if skip_next {
                skip_next = false;
                continue;
            }
            let base = a.split('=').next().unwrap_or(a);
            // 短 flag 粘连值：-oX 的 flag 名是 -o。
            let mut probe = base.to_string();
            let mut sticky = false;
            if base.len() > 2 && base.starts_with('-') && !base.starts_with("--") {
                probe = base[..2].to_string();
                sticky = true;
            }
            let canon = self.canon.canon_flag(bin, &probe);
            out.push(canon.clone());
            if !a.contains('=') && !sticky && self.canon.flag_takes_value(bin, &canon) {
                skip_next = true;
            }
        }
        out
    }

    fn lookup(&self, norm: &Normalized, cmd: &SimpleCommand, project: &Path) -> Verdict {
        // [global].allow 整命令豁免（含逃逸豁免；两表皆现时 global 优先）。
        if self.rules.global.allow.contains(&norm.bin) {
            return Verdict::allow();
        }

        // 命令节优先（跨层 global 节优先于 local 节）；同层裸列表被节遮蔽。
        if let Some((section, is_global)) = self
            .rules
            .global
            .commands
            .get(&norm.bin)
            .map(|s| (s, true))
            .or_else(|| self.rules.local.commands.get(&norm.bin).map(|s| (s, false)))
        {
            return self.classify_section(norm, section, is_global, cmd, project);
        }

        // 头部裸列表（整命令入桶语法糖）：global 表先于 local 表；同表内按
        // precedence 取第一个命中桶（global.allow 豁免已在上方处理）。
        for scope in [&self.rules.global, &self.rules.local] {
            for decision in self.precedence {
                let bucket = match decision {
                    Decision::Allow => &scope.allow,
                    Decision::Confirm => &scope.confirm,
                    Decision::Deny => &scope.deny,
                };
                if !bucket.contains(&norm.bin) {
                    continue;
                }
                return match decision {
                    Decision::Deny => Verdict::deny(format!("{} blocked (deny list)", norm.bin)),
                    Decision::Confirm => Verdict::confirm(format!(
                        "{} requires confirmation (confirm list)",
                        norm.bin
                    )),
                    Decision::Allow => {
                        // [local] 的承诺是「效果不出项目」：allow 命中带逃逸检查
                        //（用原始参数判，归一不改参数语义）。
                        if cmd.args().iter().any(|w| path_escapes(w, project)) {
                            return Verdict::confirm("path escapes repository");
                        }
                        Verdict::allow()
                    }
                };
            }
        }
        self.default_verdict(&norm.bin)
    }

    /// 命令节裁决：多维度命中按 precedence 合成。
    fn classify_section(
        &self,
        norm: &Normalized,
        section: &MergedCommand,
        is_global: bool,
        cmd: &SimpleCommand,
        project: &Path,
    ) -> Verdict {
        for decision in self.precedence {
            let dims = match decision {
                Decision::Allow => &section.allow,
                Decision::Confirm => &section.confirm,
                Decision::Deny => &section.deny,
            };
            let sub_hit = norm.sub.as_ref().and_then(|s| {
                dims.sub
                    .iter()
                    .find(|t| *t == s)
                    .map(|t| format!("{} {t}", norm.bin))
            });
            let flag_hit = norm
                .flags
                .iter()
                .find(|f| dims.flag.contains(f))
                .map(|f| format!("{} {f}", norm.bin));
            let Some(hit) = sub_hit.or(flag_hit) else {
                continue;
            };
            return match decision {
                Decision::Deny => Verdict::deny(format!("{hit} blocked")),
                Decision::Confirm => Verdict::confirm(format!("{hit} requires confirmation")),
                Decision::Allow => {
                    // [local] 的承诺是「效果不出项目」：allow 命中一律带逃逸
                    // 检查（原始参数）；[global] allow 豁免。
                    if !is_global && cmd.args().iter().any(|w| path_escapes(w, project)) {
                        return Verdict::confirm("path escapes repository");
                    }
                    Verdict::allow()
                }
            };
        }

        // 未命中：节内 default（已含跨层继承）→ 顶层 default → confirm。
        match section.default.or(self.rules.default) {
            Some(Decision::Allow) => Verdict::allow(),
            Some(Decision::Confirm) => {
                Verdict::confirm(format!("{} requires confirmation (default)", norm.bin))
            }
            Some(Decision::Deny) => Verdict::deny(format!("{} blocked (default)", norm.bin)),
            None => Verdict::confirm(format!(
                "{} requires confirmation (no default configured)",
                norm.bin
            )),
        }
    }

    fn default_verdict(&self, bin: &str) -> Verdict {
        match self.rules.default {
            Some(Decision::Allow) => Verdict::allow(),
            Some(Decision::Confirm) => {
                Verdict::confirm(format!("{bin} requires confirmation (default)"))
            }
            Some(Decision::Deny) => Verdict::deny(format!("{bin} blocked (default)")),
            None => Verdict::confirm(format!(
                "{bin} requires confirmation (no default configured)"
            )),
        }
    }
}

/// 配置侧规范形化：词条/命令节键/flag token 归一到规范形（单边配置双边生效）。
fn canonicalize_rules(rules: MergedRules, canon: &CanonMaps) -> MergedRules {
    MergedRules {
        default: rules.default,
        precedence: rules.precedence,
        script_allow: rules.script_allow.map_names(|b| canon.canon_bin(b)),
        local: canonicalize_scope(rules.local, canon),
        global: canonicalize_scope(rules.global, canon),
    }
}

fn canonicalize_scope(scope: MergedScope, canon: &CanonMaps) -> MergedScope {
    let mut commands = BTreeMap::new();
    for (key, cmd) in scope.commands {
        let canon_key = canon.canon_bin(&key);
        let cmd = canonicalize_command(cmd, &canon_key, canon);
        let merged = match commands.remove(&canon_key) {
            // 同一规范形的两个别名节并存（lint 警告对象）：字段级折叠保序。
            Some(existing) => fold_command(existing, cmd),
            None => cmd,
        };
        commands.insert(canon_key, merged);
    }
    MergedScope {
        allow: canon_unique(&scope.allow, canon),
        confirm: canon_unique(&scope.confirm, canon),
        deny: canon_unique(&scope.deny, canon),
        // 声明词条同走规范形（alias_of 声明 → 规范 bin 名对账）。
        script_allow: canon_unique(&scope.script_allow, canon),
        commands,
    }
}

fn canonicalize_command(cmd: MergedCommand, bin: &str, canon: &CanonMaps) -> MergedCommand {
    let dims = |d: Dims| Dims {
        sub: d.sub,
        flag: d
            .flag
            .iter()
            .map(|t| canon.canon_flag(bin, t))
            .collect::<Vec<_>>()
            .into_iter()
            .fold(Vec::new(), extend_unique),
    };
    MergedCommand {
        allow: dims(cmd.allow),
        confirm: dims(cmd.confirm),
        deny: dims(cmd.deny),
        default: cmd.default,
        script_allow: cmd.script_allow,
    }
}

/// 折叠同一规范形命令节的两个字段副本（保持出现序、去重）。
fn fold_command(mut a: MergedCommand, b: MergedCommand) -> MergedCommand {
    for (dst, src) in [
        (&mut a.allow, b.allow),
        (&mut a.confirm, b.confirm),
        (&mut a.deny, b.deny),
    ] {
        dst.sub = src
            .sub
            .iter()
            .chain(dst.sub.iter())
            .cloned()
            .fold(Vec::new(), extend_unique);
        dst.flag = src
            .flag
            .iter()
            .chain(dst.flag.iter())
            .cloned()
            .fold(Vec::new(), extend_unique);
    }
    a.default = a.default.or(b.default);
    a
}

fn canon_unique(tokens: &[String], canon: &CanonMaps) -> Vec<String> {
    tokens
        .iter()
        .map(|t| canon.canon_bin(t))
        .collect::<Vec<_>>()
        .into_iter()
        .fold(Vec::new(), extend_unique)
}

fn extend_unique(mut acc: Vec<String>, t: String) -> Vec<String> {
    if !acc.contains(&t) {
        acc.push(t);
    }
    acc
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cmd_parse::flatten_commands;
    use crate::config::{Layers, RulesFile, merge};
    use std::path::PathBuf;

    /// 项目根固定为不存在的参考目录；逃逸用例用仓库外绝对/相对路径。
    const PROJ: &str = "D:/code/tmp/lookup-project";

    fn file(src: &str) -> RulesFile {
        RulesFile::parse_toml(src).expect("fixture parses")
    }

    fn lookup(project: &str) -> RuleLookup {
        let f = file(project);
        RuleLookup::new(
            merge(Layers {
                global: None,
                user: None,
                project: Some(&f),
            }),
            None,
        )
    }

    fn lookup3(global: &str, user: &str, project: &str) -> RuleLookup {
        let g = file(global);
        let u = file(user);
        let p = file(project);
        RuleLookup::new(
            merge(Layers {
                global: Some(&g),
                user: Some(&u),
                project: Some(&p),
            }),
            None,
        )
    }

    fn cmd(s: &str) -> SimpleCommand {
        flatten_commands(s)
            .expect("parses")
            .into_iter()
            .next()
            .expect("one simple command")
    }

    fn classify(l: &RuleLookup, s: &str) -> Verdict {
        l.classify(&cmd(s), Path::new(PROJ))
    }

    /// 基线配置：头部三桶 + git 节 + npm 节 default。
    const BASE: &str = concat!(
        "version = 1\n",
        "default = \"confirm\"\n",
        "[local]\n",
        "allow = [\"ls\", \"cat\"]\n",
        "confirm = [\"curl\"]\n",
        "deny = [\"sudo\"]\n",
        "[local.git]\n",
        "allow.sub = [\"status\", \"log\", \"show\"]\n",
        "confirm.sub = [\"reset\"]\n",
        "confirm.flag = [\"--output\", \"-o\"]\n",
        "deny.sub = [\"push\"]\n",
        "deny.flag = [\"--hard\"]\n",
        "[local.npm]\n",
        "confirm.sub = [\"publish\"]\n",
        "default = \"allow\"\n",
        "[global]\n",
        "allow = []\n",
    );

    #[test]
    fn head_buckets_decide_by_list_membership() {
        let l = lookup(BASE);
        assert!(classify(&l, "ls").decision == Decision::Allow);
        assert!(classify(&l, "curl").decision == Decision::Confirm);
        assert!(classify(&l, "sudo x").decision == Decision::Deny);
    }

    #[test]
    fn section_shadows_head_bare_list_within_scope() {
        // 头部 confirm 含 "git"，但存在 [local.git] 节 → 头部词条被遮蔽：
        // 节内命中的走节内档位，节内未命中的落 default 而非头部 confirm。
        let src = concat!(
            "version = 1\n",
            "default = \"confirm\"\n",
            "[local]\n",
            "confirm = [\"git\"]\n",
            "[local.git]\n",
            "allow.sub = [\"status\"]\n",
            "deny.sub = [\"push\"]\n",
        );
        let l = lookup(src);
        assert!(classify(&l, "git status").decision == Decision::Allow);
        assert!(classify(&l, "git push").decision == Decision::Deny);
        assert!(
            classify(&l, "git fetchall").decision == Decision::Confirm,
            "落节内 default，不被头部 confirm 捕获"
        );
    }

    #[test]
    fn multi_dimension_hits_synth_by_precedence() {
        // show 命中 allow.sub，--output 命中 confirm.flag → confirm 胜出。
        let l = lookup(BASE);
        assert!(classify(&l, "git show --output=x").decision == Decision::Confirm);
        assert!(classify(&l, "git show -o out.txt").decision == Decision::Confirm);
        assert!(classify(&l, "git show").decision == Decision::Allow);
    }

    #[test]
    fn deny_flag_beats_confirm_sub() {
        let l = lookup(BASE);
        assert!(classify(&l, "git reset").decision == Decision::Confirm);
        assert!(classify(&l, "git reset --hard").decision == Decision::Deny);
    }

    #[test]
    fn precedence_is_configurable() {
        let src = concat!(
            "version = 1\n",
            "precedence = [\"allow\", \"confirm\", \"deny\"]\n",
            "default = \"confirm\"\n",
            "[local]\n",
            "[local.git]\n",
            "allow.sub = [\"show\"]\n",
            "confirm.flag = [\"--output\"]\n",
        );
        let l = lookup(src);
        assert!(classify(&l, "git show --output=x").decision == Decision::Allow);
    }

    #[test]
    fn local_allow_hits_carry_escape_check() {
        let l = lookup(BASE);
        assert!(classify(&l, "ls").decision == Decision::Allow);
        assert!(
            classify(&l, "ls /outside/x").decision == Decision::Confirm,
            "[local] allow 命中带路径逃逸检查"
        );
        assert!(
            classify(&l, "git status /outside/x").decision == Decision::Confirm,
            "节内 allow.sub 命中同样带逃逸检查"
        );
        assert!(classify(&l, "git status").decision == Decision::Allow);
    }

    #[test]
    fn global_allow_exempts_whole_command_and_beats_local() {
        let g = "version = 1\n[global]\nallow = [\"docker\"]";
        let u = "version = 1\ndefault = \"confirm\"\n[local]\ndeny = [\"docker\"]";
        let p = "version = 1\ndefault = \"confirm\"\n[local]\ndeny = [\"docker\"]";
        let l = lookup3(g, u, p);
        assert!(
            classify(&l, "docker -v /outside/x").decision == Decision::Allow,
            "global allow 整命令豁免（含逃逸），且优先于 local deny"
        );
    }

    #[test]
    fn global_section_shadows_local_section() {
        let g = "version = 1\n[global.git]\nallow.sub = [\"push\"]";
        let p = concat!(
            "version = 1\n",
            "default = \"confirm\"\n",
            "[local.git]\n",
            "deny.sub = [\"push\"]\n",
        );
        let l = lookup3(g, "version = 1", p);
        assert!(
            classify(&l, "git push").decision == Decision::Allow,
            "同命令两节皆现时 global 节优先"
        );
    }

    #[test]
    fn section_default_overrides_top_default() {
        let l = lookup(BASE);
        assert!(classify(&l, "npm publish").decision == Decision::Confirm);
        assert!(
            classify(&l, "npm run server").decision == Decision::Allow,
            "节内 default=allow 覆盖顶层 confirm"
        );
        assert!(classify(&l, "npm unknown-thing").decision == Decision::Allow);
    }

    #[test]
    fn unmatched_bin_falls_to_top_default_then_fail_safe() {
        let l = lookup(BASE);
        assert!(classify(&l, "mystery-cli x").decision == Decision::Confirm);

        // 无 default：fail-safe confirm（不误放行）。
        let l = lookup("version = 1\n[local]\nallow = [\"ls\"]");
        assert!(classify(&l, "mystery-cli").decision == Decision::Confirm);
    }

    #[test]
    fn empty_rules_fail_safe_to_confirm() {
        let l = lookup("version = 1");
        assert!(classify(&l, "ls").decision == Decision::Confirm);
    }

    #[test]
    fn compound_command_combines_without_degradation() {
        let project = PathBuf::from(PROJ);
        let f = file(BASE);
        let l = RuleLookup::new(
            merge(Layers {
                global: None,
                user: None,
                project: Some(&f),
            }),
            None,
        );
        let v =
            crate::engine::decide_with("ls && sudo rm x", &project, &|c, p, _| l.classify(c, p));
        assert!(v.decision == Decision::Deny, "任一 deny → 组合 deny");
        let v = crate::engine::decide_with("ls && curl example.com", &project, &|c, p, _| {
            l.classify(c, p)
        });
        assert!(
            v.decision == Decision::Confirm,
            "allow + confirm → 组合 confirm"
        );
    }

    // -------------------------------------------------------------------------
    // 知识库归一（M2.4）
    // -------------------------------------------------------------------------

    const KB_MAIN: &str = concat!(
        "version = 1\n",
        "[npx]\n",
        "may_write = true\n",
        "[npm]\n",
        "sub.exec = { alias_of = \"npx\" }\n",
        "sub.x = { alias_of = \"npx\" }\n",
        "[pnpm]\n",
        "sub.dlx = { alias_of = \"npx\" }\n",
        "[pip3]\n",
        "alias_of = \"pip\"\n",
        "[git]\n",
        "flag.\"--force\" = { same_flag = \"-f\" }\n",
        "flag.\"--output\" = { same_flag = \"-o\", takes_value = true }\n",
    );

    fn lookup_kb(rules: &str, kb: &str) -> RuleLookup {
        let f = file(rules);
        let k = crate::knowledge::KnowledgeBase::parse_toml(kb).expect("kb parses");
        RuleLookup::new(
            merge(Layers {
                global: None,
                user: None,
                project: Some(&f),
            }),
            Some(&k),
        )
    }

    #[test]
    fn command_alias_rewrites_bin_for_lookup() {
        // 配置 allow pip；`pip3 --version` 归一 → pip 命中 allow；
        // 知识库缺席时同一命令落 default。
        let rules = "version = 1\ndefault = \"confirm\"\n[local]\nallow = [\"pip\"]";
        let l = lookup_kb(rules, KB_MAIN);
        let c = l.classify_traced(&cmd("pip3 --version"), Path::new(PROJ));
        assert!(c.verdict.decision == Decision::Allow);
        assert_eq!(c.kb_chain, ["pip3", "pip"], "归一链记录改写路径");

        let l_plain = lookup(rules);
        assert!(
            classify(&l_plain, "pip3 --version").decision == Decision::Confirm,
            "无知识库按字面查表"
        );
    }

    #[test]
    fn subcommand_alias_absorbs_sub_slot_into_target_bin() {
        // npm exec/x foo 与 pnpm dlx y → npx …（bin+子命令 → 目标 bin）。
        let rules = "version = 1\n[local]\nconfirm = [\"npx\"]";
        let l = lookup_kb(rules, KB_MAIN);
        assert!(classify(&l, "npm exec foo").decision == Decision::Confirm);
        assert!(classify(&l, "npm x foo").decision == Decision::Confirm);
        assert!(classify(&l, "pnpm dlx y").decision == Decision::Confirm);
        // 归一只改名字：目标 bin 无配置时按 default 走，不放大权限。
        let rules2 = "version = 1\ndefault = \"confirm\"\n[local]\nallow = [\"npm\"]";
        let l2 = lookup_kb(rules2, KB_MAIN);
        assert!(
            classify(&l2, "npm exec foo").decision == Decision::Confirm,
            "npm 的 allow 不因归一泄漏给 npx"
        );
    }

    #[test]
    fn same_flag_closure_hits_from_either_side() {
        let rules = concat!(
            "version = 1\n",
            "default = \"confirm\"\n",
            "[local.git]\n",
            "allow.sub = [\"add\"]\n",
            "confirm.flag = [\"-f\"]\n",
        );
        let l = lookup_kb(rules, KB_MAIN);
        assert!(classify(&l, "git add").decision == Decision::Allow);
        assert!(
            classify(&l, "git add --force").decision == Decision::Confirm,
            "配置只写规范形 -f，--force 经归一命中"
        );
        assert!(classify(&l, "git add -f").decision == Decision::Confirm);

        // 反向单边：配置写别名 --force，命令用 -f 也命中。
        let rules_rev = concat!(
            "version = 1\n",
            "default = \"confirm\"\n",
            "[local.git]\n",
            "allow.sub = [\"add\"]\n",
            "confirm.flag = [\"--force\"]\n",
        );
        let l_rev = lookup_kb(rules_rev, KB_MAIN);
        assert!(classify(&l_rev, "git add -f").decision == Decision::Confirm);
    }

    #[test]
    fn takes_value_flag_consumes_its_value_token() {
        // -o/--output 取值：`git show -o --hard` 的 --hard 是「值」而非 flag，
        // 不得命中 deny.flag；无 takes_value 时同命令会被误判 deny。
        let rules = concat!(
            "version = 1\n",
            "default = \"confirm\"\n",
            "[local.git]\n",
            "allow.sub = [\"show\"]\n",
            "deny.flag = [\"--hard\"]\n",
            "confirm.flag = [\"-o\"]\n",
        );
        let l = lookup_kb(rules, KB_MAIN);
        assert!(
            classify(&l, "git show -o --hard").decision == Decision::Confirm,
            "--hard 被识别为 -o 的值，合成只剩 confirm.flag 命中"
        );
        assert!(
            classify(&l, "git show --output=--hard").decision == Decision::Confirm,
            "attached 值形态同样不进 flag 匹配"
        );
        assert!(
            classify(&l, "git show -oOUT.txt").decision == Decision::Confirm,
            "粘连值形态 -oX 归一不丢值"
        );
        assert!(
            classify(&l, "git show --hard").decision == Decision::Deny,
            "裸 --hard 仍命中 deny"
        );
    }

    #[test]
    fn no_kb_means_empty_trace_and_literal_lookup() {
        let l = lookup(BASE);
        let c = l.classify_traced(&cmd("ls"), Path::new(PROJ));
        assert_eq!(c.kb_chain, Vec::<String>::new(), "kb:[] = 归一未生效");
        assert!(c.verdict.decision == Decision::Allow);
    }
}
