//! rules.toml 查表引擎（草案 v1）：双表三桶 + 多命中合成。
//!
//! 语义单一事实源：`doc/design.md`「rules.toml 结构（草案）」查表顺序条：
//!
//! 1. `[global].allow` 命中 → 整命令 allow（豁免路径逃逸检查；两表皆现时
//!    `[global]` 优先——全局放行是更强的承诺）。
//! 2. 命令节优先：同层内裸列表词条被命令节遮蔽；跨层 `[global]` 优先。
//! 3. 节内多维度命中（sub / flag 各自合法命中不同桶）按 `precedence` 有序
//!    合成出唯一裁决（D-04：`git show --output=x` 中 `show` 命中 allow.sub、
//!    `--output` 命中 confirm.flag → confirm）。
//! 4. `[local]` 的一切 allow 命中带路径逃逸检查（local 的承诺是「效果不出
//!    项目」）；`[global]` allow 豁免。
//! 5. 未命中 → 节内 `default` → 顶层 `default` → confirm（fail-safe，恒链尾）。

use std::path::Path;

use crate::cmd_parse::{SimpleCommand, path_escapes};
use crate::config::{MergedCommand, MergedRules};
use crate::model::{Decision, Verdict};

/// 查表裁决器：持有合并后的生效规则。
pub struct RuleLookup {
    rules: MergedRules,
    precedence: [Decision; 3],
}

impl RuleLookup {
    /// `precedence` 已由解析校验（三桶排列）或合并回落默认序，恒为 3 项。
    pub fn new(rules: MergedRules) -> Self {
        let p = &rules.precedence;
        RuleLookup {
            precedence: [p[0], p[1], p[2]],
            rules,
        }
    }

    /// 查表裁决单条简单命令。
    pub fn classify(&self, cmd: &SimpleCommand, project: &Path) -> Verdict {
        let Some(bin) = cmd.bin() else {
            return Verdict::confirm("empty command");
        };

        // [global].allow 整命令豁免（含逃逸豁免；两表皆现时 global 优先）。
        if self.rules.global.allow.iter().any(|t| t == bin) {
            return Verdict::allow();
        }

        // 命令节优先（跨层 global 节优先于 local 节）；同层裸列表被节遮蔽。
        if let Some((section, is_global)) = self
            .rules
            .global
            .commands
            .get(bin)
            .map(|s| (s, true))
            .or_else(|| self.rules.local.commands.get(bin).map(|s| (s, false)))
        {
            return self.classify_section(bin, section, is_global, cmd, project);
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
                if !bucket.iter().any(|t| t == bin) {
                    continue;
                }
                return match decision {
                    Decision::Deny => Verdict::deny(format!("{bin} blocked (deny list)")),
                    Decision::Confirm => {
                        Verdict::confirm(format!("{bin} requires confirmation (confirm list)"))
                    }
                    Decision::Allow => {
                        // [local] 的承诺是「效果不出项目」：allow 命中带逃逸检查。
                        if cmd.args().iter().any(|w| path_escapes(w, project)) {
                            return Verdict::confirm("path escapes repository");
                        }
                        Verdict::allow()
                    }
                };
            }
        }
        self.default_verdict(bin)
    }

    /// 命令节裁决：多维度命中按 precedence 合成。
    fn classify_section(
        &self,
        bin: &str,
        section: &MergedCommand,
        is_global: bool,
        cmd: &SimpleCommand,
        project: &Path,
    ) -> Verdict {
        let sub = cmd.args().first().map(String::as_str);
        let args = cmd.args().get(1..).unwrap_or(&[]);

        for decision in self.precedence {
            let dims = match decision {
                Decision::Allow => &section.allow,
                Decision::Confirm => &section.confirm,
                Decision::Deny => &section.deny,
            };
            let sub_hit = sub.and_then(|s| {
                dims.sub
                    .iter()
                    .find(|t| t.as_str() == s)
                    .map(|t| format!("{bin} {t}"))
            });
            let flag_hit = args.iter().find_map(|a| {
                let base = a.split('=').next().unwrap_or(a);
                dims.flag
                    .iter()
                    .find(|t| t.as_str() == base)
                    .map(|t| format!("{bin} {t}"))
            });
            let Some(hit) = sub_hit.or(flag_hit) else {
                continue;
            };
            return match decision {
                Decision::Deny => Verdict::deny(format!("{hit} blocked")),
                Decision::Confirm => Verdict::confirm(format!("{hit} requires confirmation")),
                Decision::Allow => {
                    // [local] 的承诺是「效果不出项目」：allow 命中一律带逃逸
                    // 检查；[global] allow 豁免。
                    if !is_global && args.iter().any(|w| path_escapes(w, project)) {
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
                Verdict::confirm(format!("{bin} requires confirmation (default)"))
            }
            Some(Decision::Deny) => Verdict::deny(format!("{bin} blocked (default)")),
            None => Verdict::confirm(format!(
                "{bin} requires confirmation (no default configured)"
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
        RuleLookup::new(merge(Layers {
            global: None,
            user: None,
            project: Some(&f),
        }))
    }

    fn lookup3(global: &str, user: &str, project: &str) -> RuleLookup {
        let g = file(global);
        let u = file(user);
        let p = file(project);
        RuleLookup::new(merge(Layers {
            global: Some(&g),
            user: Some(&u),
            project: Some(&p),
        }))
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
        let l = RuleLookup::new(merge(Layers {
            global: None,
            user: None,
            project: Some(&f),
        }));
        let v = crate::engine::decide_with("ls && sudo rm x", &project, &|c, p| l.classify(c, p));
        assert!(v.decision == Decision::Deny, "任一 deny → 组合 deny");
        let v = crate::engine::decide_with("ls && curl example.com", &project, &|c, p| {
            l.classify(c, p)
        });
        assert!(
            v.decision == Decision::Confirm,
            "allow + confirm → 组合 confirm"
        );
    }
}
