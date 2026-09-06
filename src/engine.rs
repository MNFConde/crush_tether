//! 管线原语：解析拉平、管道 sink 拓扑、组合裁决的注入式顶层入口。
//!
//! M3.3 起二进制不含任何内置策略（零内置策略收口）：单命令分类由配置层
//! 提供——`rules.toml` 查表（`lookup`）+ `rules.rhai` 脚本（`script`）。
//! guard.py 判定表的 Rust 平移已删除；其定位为语义参考而非验收标准
//! （doc/design.md「判定表」定位澄清，D-05），断言变更记录见
//! `tests/guard_regression.rs` 头部。

use std::path::Path;

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

/// 规则注入式顶层判定：解析 → 管道拓扑特征 → 逐条分类 → 组合裁决。
/// 分类器由调用方注入；管道拓扑特征（整条命令行级）作为第三参传入，
/// 供脚本谓词消费。「管道 → deny」的策略在默认 rules.rhai（脚本层）。
pub fn decide_with(
    command: &str,
    project: &Path,
    classify: &dyn Fn(&SimpleCommand, &Path, bool) -> Verdict,
) -> Verdict {
    let commands = match crate::cmd_parse::flatten_commands(command) {
        Ok(c) => c,
        Err(e) => return crate::model::unparseable(e.to_string()),
    };
    if commands.is_empty() {
        return Verdict::confirm("empty command");
    }
    let pipe = pipe_to_shell(command);
    Verdict::combine(commands.iter().map(|c| classify(c, project, pipe)))
}
