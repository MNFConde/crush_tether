//! 三层字段级继承合并（D-02）：低层 = 父类，高层 = 子类。
//!
//! - 未定义即继承，定义即覆盖（遮蔽）；效力顺序：项目 > 用户 > 全局，不粘性。
//! - 列表值双形态：数组 = 覆盖定义（本份就是全部）；inline table
//!   `{ add = [...], remove = [...] }` = 继承低层并增删。
//! - 标量（`default` / `precedence`）写值即覆盖。
//! - 命令节按字段级合并：高层节只写了 `deny.sub` 时，低层同节其余维度照常继承。

use std::collections::{BTreeMap, BTreeSet};

use crate::config::discover::FoundLayers;
use crate::config::schema::{
    BucketSpec, CommandSection, ListField, RulesFile, ScopeBuckets, ScopeTable,
};
use crate::model::Decision;

/// `precedence` 三层皆未定义时的默认序（design.md：deny > confirm > allow，
/// default 恒链尾）。
pub const DEFAULT_PRECEDENCE: [Decision; 3] = [Decision::Deny, Decision::Confirm, Decision::Allow];

/// 三层输入（低 → 高：global → user → project）；`None` = 该层缺。
pub struct Layers<'a> {
    pub global: Option<&'a RulesFile>,
    pub user: Option<&'a RulesFile>,
    pub project: Option<&'a RulesFile>,
}

impl<'a> Layers<'a> {
    /// 从发现结果构造三层。
    pub fn from_found(found: &'a FoundLayers) -> Self {
        Layers {
            global: found.global.as_ref(),
            user: found.user.as_ref(),
            project: found.project.as_ref(),
        }
    }
}

/// 合并后的生效规则：列表值已解出具体词条集（Delta 已消解），查表（M2.3）
/// 直接消费本结构。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MergedRules {
    /// 顶层兜底档（三层皆未定义 → None，查表时落 confirm fail-safe）。
    pub default: Option<Decision>,
    pub precedence: Vec<Decision>,
    /// script_allow 声明集（M4.0）。
    pub script_allow: ScriptAllowDecls,
    pub local: MergedScope,
    pub global: MergedScope,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MergedScope {
    /// 头部裸列表桶（整命令词条）。
    pub allow: Vec<String>,
    pub confirm: Vec<String>,
    pub deny: Vec<String>,
    /// 本作用域 `script_allow` 顶级列表的合并结果（D-02 链）。
    pub script_allow: Vec<String>,
    pub commands: BTreeMap<String, MergedCommand>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MergedCommand {
    pub allow: Dims,
    pub confirm: Dims,
    pub deny: Dims,
    /// 节内兜底档（含跨层继承链的最终结果）。
    pub default: Option<Decision>,
    /// 节内 `script_allow` 键（高层定义即覆盖，`false` = 显式取消）。
    pub script_allow: Option<bool>,
}

/// 命令节一个桶的两个维度词条集。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Dims {
    pub sub: Vec<String>,
    pub flag: Vec<String>,
}

/// 合并三层。全 None 输入得到「全空 + 默认 precedence」（三层皆缺的默认
/// 配置生成判定在发现层做，不在合并层）。
pub fn merge(layers: Layers<'_>) -> MergedRules {
    let low_to_high = [layers.global, layers.user, layers.project];
    // 标量：高层定义即覆盖 → 从高往低找第一个定义。
    let default = low_to_high.iter().rev().flatten().find_map(|f| f.default);
    let precedence = low_to_high
        .iter()
        .rev()
        .flatten()
        .find_map(|f| f.precedence.clone())
        .unwrap_or_else(|| DEFAULT_PRECEDENCE.to_vec());
    let local = merge_scope(&low_to_high, |f| &f.local);
    let global = merge_scope(&low_to_high, |f| &f.global);
    // script_allow 声明集：两形态汇入同一集合（顶级列表 ∪ 节键 true），
    // 附声明所在表元数据；两表皆现 → global 胜（M2.3「更强的承诺」同规）。
    let mut script_allow = ScriptAllowDecls::default();
    for b in &local.script_allow {
        script_allow.local.insert(b.clone());
    }
    for b in &global.script_allow {
        script_allow.global.insert(b.clone());
    }
    for (name, c) in &local.commands {
        if c.script_allow == Some(true) {
            script_allow.local.insert(name.clone());
        }
    }
    for (name, c) in &global.commands {
        if c.script_allow == Some(true) {
            script_allow.global.insert(name.clone());
        }
    }
    MergedRules {
        default,
        precedence,
        script_allow,
        local,
        global,
    }
}

/// script_allow 声明集（M4.0）：脚本 allow 激活的对账白名单，按 bin 名
/// 索引、附声明所在表的元数据。声明不是规则、不进查表路径。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ScriptAllowDecls {
    local: BTreeSet<String>,
    global: BTreeSet<String>,
}

/// 声明所在的作用域（定稿点作用域化逃逸检查的判据）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeclScope {
    /// local 声明：激活时对原始命令参数执行路径逃逸检查。
    Local,
    /// global 声明：豁免逃逸检查（两表皆现时 global 胜）。
    Global,
}

impl ScriptAllowDecls {
    /// 登记一条 local 声明（测试/装配辅助）。
    pub fn declare_local(&mut self, bin: &str) {
        self.local.insert(bin.to_string());
    }

    /// 登记一条 global 声明（测试/装配辅助）。
    pub fn declare_global(&mut self, bin: &str) {
        self.global.insert(bin.to_string());
    }

    /// bin 的声明作用域；未声明 = `None`（运行时 allow(name) 激活被拒）。
    pub fn scope_of(&self, bin: &str) -> Option<DeclScope> {
        if self.global.contains(bin) {
            Some(DeclScope::Global)
        } else if self.local.contains(bin) {
            Some(DeclScope::Local)
        } else {
            None
        }
    }

    /// 全部已声明 bin（lint 死声明检查用；去重后的并集视图）。
    pub fn bins(&self) -> impl Iterator<Item = &str> {
        self.global
            .iter()
            .chain(self.local.iter())
            .map(String::as_str)
    }

    pub fn is_empty(&self) -> bool {
        self.local.is_empty() && self.global.is_empty()
    }

    /// 以规范名重建声明集（查表层规范形化用：alias 名声明 → 规范 bin 名）。
    pub fn map_names(self, f: impl Fn(&str) -> String) -> Self {
        fn map_set(set: BTreeSet<String>, f: &impl Fn(&str) -> String) -> BTreeSet<String> {
            set.into_iter().map(|s| f(&s)).collect()
        }
        Self {
            local: map_set(self.local, &f),
            global: map_set(self.global, &f),
        }
    }
}

/// 沿低 → 高解一条列表链：`Set` 覆盖累计；`Delta` 基于累计增删（先 remove
/// 后 add）；链上无任何定义 → 空集。
fn resolve_chain<'a>(chain: impl Iterator<Item = Option<&'a ListField>>) -> Vec<String> {
    let mut acc: Option<Vec<String>> = None;
    for field in chain.flatten() {
        match field {
            ListField::Set(v) => acc = Some(v.clone()),
            ListField::Delta { add, remove } => {
                let mut out = acc.take().unwrap_or_default();
                if let Some(r) = remove {
                    out.retain(|t| !r.contains(t));
                }
                if let Some(a) = add {
                    for t in a {
                        if !out.contains(t) {
                            out.push(t.clone());
                        }
                    }
                }
                acc = Some(out);
            }
        }
    }
    acc.unwrap_or_default()
}

fn merge_scope(
    layers: &[Option<&RulesFile>; 3],
    get: impl Fn(&RulesFile) -> &ScopeTable,
) -> MergedScope {
    let defined: Vec<&ScopeTable> = layers.iter().flatten().map(|f| get(f)).collect();
    let head = |pick: fn(&ScopeBuckets) -> Option<&ListField>| {
        resolve_chain(defined.iter().map(|t| pick(&t.buckets)))
    };
    // 命令节并集（低 → 高收集；同节名字段级合并）。
    let mut commands = BTreeMap::new();
    let mut names: Vec<&String> = defined.iter().flat_map(|t| t.commands.keys()).collect();
    names.sort();
    names.dedup();
    for name in names {
        let secs: Vec<&CommandSection> = defined
            .iter()
            .filter_map(|t| t.commands.get(name))
            .collect();
        commands.insert(name.clone(), merge_command(&secs));
    }
    MergedScope {
        allow: head(|b| b.allow.as_ref()),
        confirm: head(|b| b.confirm.as_ref()),
        deny: head(|b| b.deny.as_ref()),
        // script_allow 顶级列表不在 ScopeBuckets（与三桶不同命名空间）。
        script_allow: resolve_chain(defined.iter().map(|t| t.script_allow.as_ref())),
        commands,
    }
}

fn merge_dims(secs: &[&CommandSection], pick: fn(&CommandSection) -> Option<&BucketSpec>) -> Dims {
    let specs: Vec<_> = secs.iter().filter_map(|s| pick(s)).collect();
    Dims {
        sub: resolve_chain(specs.iter().map(|b| b.sub.as_ref())),
        flag: resolve_chain(specs.iter().map(|b| b.flag.as_ref())),
    }
}

fn merge_command(secs: &[&CommandSection]) -> MergedCommand {
    MergedCommand {
        allow: merge_dims(secs, |s| s.allow.as_ref()),
        confirm: merge_dims(secs, |s| s.confirm.as_ref()),
        deny: merge_dims(secs, |s| s.deny.as_ref()),
        // secs 低 → 高；从高往低找第一个定义（高层覆盖）。
        default: secs.iter().rev().find_map(|s| s.default),
        script_allow: secs.iter().rev().find_map(|s| s.script_allow),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn f(src: &str) -> RulesFile {
        RulesFile::parse_toml(src).expect("fixture parses")
    }

    /// global/user/project 依次为三层的 TOML 文本（None = 层缺）。
    fn merged(global: Option<&str>, user: Option<&str>, project: Option<&str>) -> MergedRules {
        let g = global.map(f);
        let u = user.map(f);
        let p = project.map(f);
        merge(Layers {
            global: g.as_ref(),
            user: u.as_ref(),
            project: p.as_ref(),
        })
    }

    #[test]
    fn inherit_when_higher_layers_undefined() {
        let m = merged(Some("version = 1\n[local]\nallow = [\"a\"]"), None, None);
        assert_eq!(m.local.allow, ["a"]);
    }

    #[test]
    fn array_override_replaces_lower_completely_not_sticky() {
        let m = merged(
            Some("version = 1\n[local]\nallow = [\"a\", \"b\"]"),
            None,
            Some("version = 1\n[local]\nallow = [\"c\"]"),
        );
        assert_eq!(m.local.allow, ["c"], "数组 = 覆盖定义，低层不留残词");
    }

    #[test]
    fn buckets_merge_per_field_not_whole_table() {
        let m = merged(
            Some("version = 1\n[local]\nallow = [\"a\"]\nconfirm = [\"c\"]"),
            None,
            Some("version = 1\n[local]\ndeny = [\"d\"]"),
        );
        assert_eq!(m.local.allow, ["a"]);
        assert_eq!(m.local.confirm, ["c"]);
        assert_eq!(m.local.deny, ["d"]);
    }

    #[test]
    fn delta_add_remove_over_lower_set() {
        let m = merged(
            None,
            Some("version = 1\n[local]\nallow = [\"curl\", \"cat\", \"grep\"]"),
            Some("version = 1\n[local]\nallow = { add = [\"jq\"], remove = [\"curl\"] }"),
        );
        assert_eq!(m.local.allow, ["cat", "grep", "jq"]);
    }

    #[test]
    fn flag_bucket_supports_remove() {
        let m = merged(
            None,
            Some("version = 1\n[local.git]\nconfirm.flag = [\"--force\", \"-h\"]"),
            Some("version = 1\n[local.git]\nconfirm.flag = { remove = [\"-h\"] }"),
        );
        assert_eq!(m.local.commands["git"].confirm.flag, ["--force"]);
    }

    #[test]
    fn delta_over_delta_appends_with_dedup() {
        let m = merged(
            Some("version = 1\n[local]\nallow = { add = [\"a\", \"b\"] }"),
            Some("version = 1\n[local]\nallow = { add = [\"b\", \"c\"] }"),
            None,
        );
        assert_eq!(m.local.allow, ["a", "b", "c"]);
    }

    #[test]
    fn delta_with_no_lower_base_starts_from_empty() {
        let m = merged(
            None,
            None,
            Some("version = 1\n[local]\nallow = { add = [\"x\"], remove = [\"y\"] }"),
        );
        assert_eq!(m.local.allow, ["x"]);
    }

    #[test]
    fn scalar_default_walks_project_user_global() {
        let m = merged(
            Some("version = 1\ndefault = \"deny\""),
            Some("version = 1\ndefault = \"confirm\""),
            Some("version = 1\ndefault = \"allow\""),
        );
        assert_eq!(m.default, Some(Decision::Allow));

        let m = merged(Some("version = 1\ndefault = \"deny\""), None, None);
        assert_eq!(m.default, Some(Decision::Deny));

        let m = merged(None, None, None);
        assert_eq!(m.default, None, "三层皆未定义 → None（查表落 fail-safe）");
    }

    #[test]
    fn precedence_high_layer_definition_wins() {
        let m = merged(
            Some("version = 1\nprecedence = [\"allow\", \"confirm\", \"deny\"]"),
            Some("version = 1\nprecedence = [\"confirm\", \"deny\", \"allow\"]"),
            None,
        );
        assert_eq!(
            m.precedence,
            [Decision::Confirm, Decision::Deny, Decision::Allow]
        );
    }

    #[test]
    fn precedence_falls_back_to_default_when_undefined_everywhere() {
        let m = merged(None, None, None);
        assert_eq!(m.precedence, DEFAULT_PRECEDENCE);
    }

    #[test]
    fn command_section_default_inherits_across_layers() {
        // 项目层只写 confirm.sub、未写 default → 继承用户层节内 default。
        let m = merged(
            None,
            Some("version = 1\n[local.npm]\ndefault = \"allow\""),
            Some("version = 1\n[local.npm]\nconfirm.sub = [\"install\"]"),
        );
        let npm = &m.local.commands["npm"];
        assert_eq!(npm.default, Some(Decision::Allow));
        assert_eq!(npm.confirm.sub, ["install"]);
    }

    #[test]
    fn command_sections_union_across_layers() {
        let m = merged(
            None,
            Some("version = 1\n[local.npm]\ndefault = \"allow\""),
            Some("version = 1\n[local.cargo]\nallow.sub = [\"build\"]"),
        );
        assert!(m.local.commands.contains_key("npm"));
        assert!(m.local.commands.contains_key("cargo"));
    }

    #[test]
    fn command_fields_merge_per_dimension() {
        let m = merged(
            None,
            Some("version = 1\n[local.git]\nconfirm.flag = [\"--force\"]"),
            Some("version = 1\n[local.git]\ndeny.sub = [\"push\"]"),
        );
        let git = &m.local.commands["git"];
        assert_eq!(git.confirm.flag, ["--force"], "低层维度照常继承");
        assert_eq!(git.deny.sub, ["push"]);
        assert!(git.allow.sub.is_empty());
    }

    #[test]
    fn command_default_three_layer_chain() {
        let m = merged(
            Some("version = 1\n[local.npm]\ndefault = \"deny\""),
            Some("version = 1\n[local.npm]\ndefault = \"confirm\""),
            Some("version = 1\n[local.npm]\ndefault = \"allow\""),
        );
        assert_eq!(m.local.commands["npm"].default, Some(Decision::Allow));
    }

    #[test]
    fn empty_merge_yields_empty_rules_with_default_precedence() {
        let m = merged(None, None, None);
        assert_eq!(m.default, None);
        assert_eq!(m.precedence, DEFAULT_PRECEDENCE);
        assert_eq!(m.local, MergedScope::default());
        assert_eq!(m.global, MergedScope::default());
    }

    #[test]
    fn global_scope_merges_independently_of_local() {
        let m = merged(
            Some("version = 1\n[global]\nallow = [\"docker\"]"),
            None,
            Some("version = 1\n[global]\nallow = { add = [\"kubectl\"] }"),
        );
        assert_eq!(m.global.allow, ["docker", "kubectl"]);
        assert!(m.local.allow.is_empty());
    }

    #[test]
    fn script_allow_top_list_merges_with_d02_semantics() {
        let m = merged(
            Some("version = 1\n[local]\nscript_allow = [\"a\", \"b\"]"),
            None,
            Some("version = 1\n[local]\nscript_allow = { remove = [\"b\"], add = [\"c\"] }"),
        );
        assert_eq!(m.local.script_allow, ["a", "c"]);
        // 数组 = 覆盖，不粘低层。
        let m = merged(
            Some("version = 1\n[local]\nscript_allow = [\"a\"]"),
            None,
            Some("version = 1\n[local]\nscript_allow = [\"x\"]"),
        );
        assert_eq!(m.local.script_allow, ["x"]);
    }

    #[test]
    fn script_allow_section_flag_inherits_and_revokes_across_layers() {
        // 低层节键 true 继承；高层 false = 显式取消。
        let m = merged(
            None,
            Some("version = 1\n[local.ls]\nscript_allow = true"),
            Some("version = 1\n[local.ls]\nallow.sub = [\"x\"]"),
        );
        assert_eq!(m.local.commands["ls"].script_allow, Some(true));
        let m = merged(
            None,
            Some("version = 1\n[local.ls]\nscript_allow = true"),
            Some("version = 1\n[local.ls]\nscript_allow = false"),
        );
        assert_eq!(m.local.commands["ls"].script_allow, Some(false));
    }

    #[test]
    fn script_allow_decls_union_two_forms_per_scope() {
        let m = merged(
            None,
            None,
            Some(concat!(
                "version = 1\n",
                "[local]\n",
                "script_allow = [\"ls\", \"docker\"]\n",
                "[local.git]\n",
                "script_allow = true\n",
            )),
        );
        assert_eq!(m.script_allow.scope_of("ls"), Some(DeclScope::Local));
        assert_eq!(m.script_allow.scope_of("docker"), Some(DeclScope::Local));
        assert_eq!(m.script_allow.scope_of("git"), Some(DeclScope::Local));
        assert_eq!(m.script_allow.scope_of("curl"), None);
    }

    #[test]
    fn script_allow_declared_in_both_scopes_global_wins() {
        let m = merged(
            Some("version = 1\n[local]\nscript_allow = [\"ls\"]"),
            None,
            Some("version = 1\n[global]\nscript_allow = [\"ls\"]"),
        );
        assert_eq!(
            m.script_allow.scope_of("ls"),
            Some(DeclScope::Global),
            "两表皆声明 → global 胜（M2.3 同规）"
        );
    }

    #[test]
    fn script_allow_scopes_stay_independent() {
        let m = merged(
            Some("version = 1\n[global]\nscript_allow = [\"docker\"]"),
            None,
            Some("version = 1\n[local]\nscript_allow = [\"ls\"]"),
        );
        assert_eq!(m.script_allow.scope_of("docker"), Some(DeclScope::Global));
        assert_eq!(m.script_allow.scope_of("ls"), Some(DeclScope::Local));
        assert!(!m.script_allow.is_empty());
    }
}
