//! 管线原语：解析拉平、管道 sink 拓扑、组合裁决的注入式顶层入口。
//!
//! M3.3 起二进制不含任何内置策略（零内置策略收口）：单命令分类由配置层
//! 提供——`rules.toml` 查表（`lookup`）+ `rules.rhai` 脚本（`script`）。
//! guard.py 判定表的 Rust 平移已删除；其定位为语义参考而非验收标准
//! （doc/design.md「判定表」定位澄清，D-05），断言变更记录见
//! `tests/guard_regression.rs` 头部。

use std::path::{Path, PathBuf};

use crate::cmd_parse::SimpleCommand;
use crate::model::Verdict;

/// 管道危险 sink 判定（curl|sh 类）。判据是**原始命令行**的 `|` 拓扑：
/// flatten 保序（源码顺序），相邻命令即管道相邻段；`|` 连接符在
/// tree-sitter-bash 的 pipeline 节点中不产生独立词元，且 list（`;`/`&&`）
/// 会切断相邻性，故「相邻即管道」不会误报分号连接的无关命令。为规避歧义，
/// sink 判定基于原始命令行中的 `|`：对每条含 `|` 的行片段检查其下游。
/// 拓扑判定是引擎原语；「管道 → deny」的策略由脚本层承载（ctx.pipe_to_shell）。
pub fn pipe_to_shell(source: &str) -> bool {
    source.split([';', '&', '\n']).any(|segment| {
        // 仅处理含竖线的片段；排除 ||（逻辑或）与字典串中的 |。
        let segment = segment.trim();
        if !segment.contains('|') {
            return false;
        }
        segment
            .split('|')
            .filter(|s| !s.trim().is_empty())
            .skip(1)
            .any(|side| {
                let first = side.split_whitespace().next().unwrap_or("");
                let first = first.trim_matches(|c| c == '"' || c == '\'');
                PIPE_SINKS.contains(&first)
            })
    })
}

/// 管道危险 sink 集合。
const PIPE_SINKS: &[&str] = &[
    "bash", "sh", "zsh", "python", "python3", "perl", "php", "ruby",
];

/// 注入式单命令分类器签名：(命令, 段级 cwd 基准, 项目根, 管道 sink) → 裁决。
pub type ClassifyFn<'a> = &'a dyn Fn(&SimpleCommand, Option<&Path>, &Path, bool) -> Verdict;

/// 规则注入式顶层判定：解析 → 段级 cwd 基准 → 管道拓扑特征 → 逐条分类
/// → 组合裁决。分类器由调用方注入；第二参 = 该命令的段级基准目录
/// （M9.2：行内 `cd` 状态机，None = 不可解析基准），第三参 = 项目根，
/// 第四参 = 管道拓扑特征。「管道 → deny」的策略在默认 rules.rhai（脚本层）。
pub fn decide_with(command: &str, project: &Path, classify: ClassifyFn<'_>) -> Verdict {
    let commands = match crate::cmd_parse::flatten_commands(command) {
        Ok(c) => c,
        Err(e) => return crate::model::unparseable(e.to_string()),
    };
    if commands.is_empty() {
        return Verdict::confirm("empty command");
    }
    let bases = segment_bases(&commands, project);
    let pipe = pipe_to_shell(command);
    Verdict::combine(
        commands
            .iter()
            .enumerate()
            .map(|(i, c)| classify(c, bases[i].as_deref(), project, pipe)),
    )
}

/// 段级 cwd 基准（M9.2）：行内 `cd` 状态机——`bases[i]` = 第 i 条简单命令
/// 执行时的相对路径解析基准。初值 = 项目根；`cd` 切换；子 shell 进组压栈/
/// 出组弹栈（组内 `cd` 不外泄）。不可解析目标（`$VAR`/`cd -`/pushd 等）
/// → `None` **毒化**：其后各段若有写效果目标，保守判逃逸（宁多拦不漏放）。
pub fn segment_bases(commands: &[SimpleCommand], project: &Path) -> Vec<Option<PathBuf>> {
    let mut bases = Vec::with_capacity(commands.len());
    let mut cur: Option<PathBuf> = Some(project.to_path_buf());
    // 栈存 (组 id, 进组时基准)；顶层（None 组）不入栈——出组即回到进组前基准。
    let mut stack: Vec<(usize, Option<PathBuf>)> = Vec::new();
    for cmd in commands {
        match cmd.subshell_id {
            Some(id) => {
                if stack.last().map(|(g, _)| *g) != Some(id) {
                    match stack.iter().position(|(g, _)| *g == id) {
                        // 回到外层已有组：弹栈并恢复进组时基准。
                        Some(pos) => {
                            stack.truncate(pos + 1);
                            cur = stack.last().expect("non-empty after truncate").1.clone();
                        }
                        // 全新组：以当前基准进入。
                        None => stack.push((id, cur.clone())),
                    }
                }
            }
            None if !stack.is_empty() => {
                stack.clear();
                cur = Some(project.to_path_buf());
            }
            None => {}
        }
        bases.push(cur.clone());
        if cmd.bin() == Some("cd") {
            cur = resolve_cd_target(cmd, cur.as_deref(), project);
        }
    }
    bases
}

/// 解析 `cd` 的目标基准：无参/`~` 形态 → HOME；`$VAR`/`cd -`/`pushd` 类
/// 静态不可解析 → None（毒化）；绝对路径原样；相对路径 join 当前基准。
fn resolve_cd_target(cmd: &SimpleCommand, base: Option<&Path>, _project: &Path) -> Option<PathBuf> {
    if cmd.has_expansion {
        // 目标含 /命令替换：展开值静态不可知 → 毒化。
        return None;
    }
    let Some(arg) = cmd.args().first() else {
        // `cd` 无参 = 回 HOME（HOME 可取则可解析，否则毒化）。
        return std::env::var("USERPROFILE")
            .or_else(|_| std::env::var("HOME"))
            .ok()
            .map(PathBuf::from);
    };
    if arg == "-" || arg.contains('$') {
        return None;
    }
    match base {
        // 基准已毒化：绝对目标仍可解析，相对目标不可知。
        Some(b) => Some(crate::cmd_parse::resolve_against_base(arg, b)),
        None => {
            let p = Path::new(arg);
            if p.is_absolute() {
                Some(p.to_path_buf())
            } else {
                None
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bases(cmd: &str, project: &str) -> Vec<Option<PathBuf>> {
        let commands = crate::cmd_parse::flatten_commands(cmd).expect("parses");
        segment_bases(&commands, Path::new(project))
    }

    #[test]
    fn cd_switches_base_until_next_cd() {
        let b = bases("cd A && x && cd B && y", "D:/proj");
        assert_eq!(b[0], Some(PathBuf::from("D:/proj")));
        assert_eq!(b[1], Some(PathBuf::from("D:/proj/A")));
        assert_eq!(b[2], Some(PathBuf::from("D:/proj/A")));
        assert_eq!(
            b[3],
            Some(PathBuf::from("D:/proj/A/B")),
            "cd B 相对 A 解析（bash 语义；Path 分量等值）"
        );
    }

    #[test]
    fn subshell_cd_does_not_leak_and_restores_entry_base() {
        let b = bases("(cd A); x", "D:/proj");
        assert_eq!(b[0], Some(PathBuf::from("D:/proj")));
        assert_eq!(b[1], Some(PathBuf::from("D:/proj")));
    }

    #[test]
    fn unresolvable_cd_poisons_until_absolute_cd() {
        // 绝对路径形态按平台取（Windows 有盘符才视为绝对）。
        let (proj, tmp) = if cfg!(windows) {
            ("D:/proj", "D:/tmp")
        } else {
            ("/proj", "/tmp")
        };
        let b = bases(&format!("cd $D && x && cd {tmp} && y"), proj);
        assert_eq!(b[1], None, "毒化");
        assert_eq!(b[2], None, "毒化持续");
        assert_eq!(b[3].as_deref(), Some(Path::new(tmp)), "绝对 cd 恢复可解析");
    }
}
