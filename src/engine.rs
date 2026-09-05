//! 判定表与规则链：Python 版 guard.py 判定表的 Rust 平移。
//!
//! 三档语义：deny（不可逆/破坏性）、confirm（有风险/可逆）、allow（只读/仓库内安全写）。
//! 分类对象是拉平后的每条简单命令；组合裁决见 [`crate::model::Verdict::combine`]。

use std::path::Path;

use crate::cmd_parse::{SimpleCommand, path_escapes, project_root};
use crate::model::Verdict;

/// 只读命令集（读文件/查询无写副作用）。
const READONLY: &[&str] = &[
    "ls",
    "cat",
    "grep",
    "rg",
    "find",
    "head",
    "tail",
    "wc",
    "pwd",
    "echo",
    "printf",
    "which",
    "file",
    "stat",
    "sort",
    "uniq",
    "comm",
    "diff",
    "md5sum",
    "sha1sum",
    "sha256sum",
    "sha512sum",
    "date",
    "env",
    "du",
    "nl",
    "less",
    "more",
    "tree",
    "ls-files",
    "rev-parse",
];

/// 破坏性命令集（rm 单列：项目决策为任何形态一律 confirm）。
const DESTRUCTIVE: &[&str] = &[
    "sudo", "rm", "mkfs", "shutdown", "reboot", "dd", "halt", "poweroff", "init", "parted",
    "fdisk", "wipefs",
];

/// git 纯读子命令。
const GIT_READONLY: &[&str] = &[
    "status",
    "log",
    "diff",
    "show",
    "branch",
    "--version",
    "remote",
    "ls-files",
    "config",
    "rev-parse",
    "blame",
    "shortlog",
    "tag",
    "help",
    "describe",
    "check-ignore",
    "show-ref",
    "for-each-ref",
    "cat-file",
    "ls-tree",
    "diff-tree",
    "name-rev",
    "merge-base",
    "rev-list",
    "diff-index",
    "diff-files",
    "check-ref-format",
];

/// git 仓库内安全写子命令（路径不逃逸仓库时 allow）。
const GIT_SAFE_WRITE: &[&str] = &["add", "commit", "checkout", "switch", "restore", "mv", "rm"];

/// git 破坏性子命令（硬 deny）。
const GIT_DESTRUCTIVE: &[&str] = &[
    "reset",
    "clean",
    "rebase",
    "revert",
    "cherry-pick",
    "push",
    "pull",
    "fetch",
    "gc",
    "prune",
    "filter-branch",
    "reflog",
];

/// git 两态子命令的写形态词元（出现即 confirm；纯读保持 allow）。
const GIT_ACTION_WRITE_TOKENS: &[(&str, &[&str])] = &[
    (
        "branch",
        &[
            "-d", "-D", "-m", "-M", "-c", "--delete", "--move", "--create",
        ],
    ),
    (
        "remote",
        &[
            "add",
            "set-url",
            "remove",
            "rename",
            "set-head",
            "set-branches",
        ],
    ),
    (
        "tag",
        &["-d", "--delete", "-a", "-s", "-m", "-f", "-u", "--annotate"],
    ),
    (
        "config",
        &[
            "--global",
            "--system",
            "--local",
            "--file",
            "--add",
            "--replace-all",
            "--unset",
            "--unset-all",
            "--rename-section",
            "--remove-section",
        ],
    ),
];

/// 视为写操作的 flag（只读命令带这些 flag 转 confirm）。
const WRITE_FLAGS: &[&str] = &[
    "--output",
    "-o",
    "--pretty",
    "--format",
    "-c",
    "--config",
    "-w",
    "--write",
    "--in-place",
    "--force",
    "-f",
    "--hard",
    "-h",
];

/// `--output=path` 形态的写 flag 前缀。
const WRITE_FLAG_PREFIXES: &[&str] = &["--output=", "-o=", "--pretty=", "--format=", "--config="];

/// 管道危险 sink（curl|sh 类）。
const PIPE_SINKS: &[&str] = &[
    "bash", "sh", "zsh", "python", "python3", "perl", "php", "ruby",
];

/// 判定单条简单命令。
pub fn classify(cmd: &SimpleCommand, project: &Path) -> Verdict {
    let Some(bin) = cmd.bin() else {
        return Verdict::confirm("empty command");
    };
    let args = cmd.args();

    // --- 破坏性 / 不可逆：硬 deny --------------------------------------
    if bin == "sudo" {
        return Verdict::deny("sudo blocked");
    }
    if DESTRUCTIVE.contains(&bin) {
        // rm 项目决策：任何形态（-rf/--force/-fx/裸 rm）一律 confirm，不自动放行。
        if bin == "rm" {
            return Verdict::confirm("rm requires confirmation");
        }
        return Verdict::deny(format!("{bin} blocked"));
    }
    // 前缀形态破坏性：mkfs.ext4 等。
    if DESTRUCTIVE.iter().any(|d| *d != "rm" && bin.starts_with(d)) {
        return Verdict::deny(format!("{bin} blocked"));
    }
    if bin == "curl" || bin == "wget" {
        return Verdict::confirm(format!("{bin} requires confirmation"));
    }

    // --- git ------------------------------------------------------------
    if bin == "git" {
        return classify_git(args, cmd, project);
    }

    // --- 包管理器（写）：confirm ----------------------------------------
    if bin == "npm"
        && args.first().is_some_and(|s| {
            matches!(
                s.as_str(),
                "install" | "i" | "ci" | "add" | "uninstall" | "remove" | "update" | "publish"
            )
        })
    {
        return Verdict::confirm("npm install/publish requires confirmation");
    }
    if matches!(bin, "pnpm" | "yarn" | "bun")
        && args.first().is_some_and(|s| {
            matches!(
                s.as_str(),
                "install" | "add" | "remove" | "upgrade" | "update" | "publish"
            )
        })
    {
        return Verdict::confirm("package install requires confirmation");
    }
    if bin == "pip" || bin == "pip3" || bin == "npx" {
        return Verdict::confirm(format!("{bin} requires confirmation"));
    }

    // --- 构建/工具链：allow（路径逃逸时 confirm）------------------------
    if bin == "go"
        && args
            .first()
            .is_some_and(|s| matches!(s.as_str(), "build" | "test" | "vet" | "fmt" | "mod" | "run"))
    {
        return toolchain_verdict(cmd, project);
    }
    if bin == "cargo"
        && args
            .first()
            .is_some_and(|s| matches!(s.as_str(), "build" | "test" | "fmt" | "check" | "clippy"))
    {
        return toolchain_verdict(cmd, project);
    }
    if matches!(
        bin,
        "gofmt" | "black" | "ruff" | "dprint" | "pytest" | "touch" | "mkdir"
    ) {
        return toolchain_verdict(cmd, project);
    }
    if bin == "make" || bin == "just" {
        return Verdict::allow();
    }

    // --- 只读命令 ---------------------------------------------------------
    if READONLY.contains(&bin) {
        if has_write_flag(args) || cmd.writes_redirect {
            return Verdict::confirm("read-only command with write side-effect");
        }
        // find 带 -delete/-exec/-execdir/-ok 突变，绕过 rm 门。
        if bin == "find" && args.iter().any(|w| is_find_mutation(w)) {
            return Verdict::confirm("find mutation requires confirmation");
        }
        // 读仓库外文件无害，不应用 path_escapes。
        return Verdict::allow();
    }

    // --- 默认：confirm ----------------------------------------------------
    Verdict::confirm(format!("{bin} requires confirmation"))
}

fn classify_git(args: &[String], cmd: &SimpleCommand, project: &Path) -> Verdict {
    let Some(sub) = args.first().map(String::as_str) else {
        return Verdict::confirm("git requires subcommand");
    };

    if GIT_DESTRUCTIVE.contains(&sub) {
        return Verdict::deny(format!("git {sub} blocked"));
    }
    // git rm / restore / reset 始终变更工作树/索引。
    if matches!(sub, "rm" | "restore" | "reset") {
        return Verdict::confirm(format!("git {sub} requires confirmation"));
    }
    if sub == "config" {
        // 写形态：显式写 flag，或 key+value 两个位置参数；裸 key/--list 为读。
        if args.iter().any(|a| {
            matches!(
                a.as_str(),
                "--add"
                    | "--replace-all"
                    | "--unset"
                    | "--unset-all"
                    | "--remove-section"
                    | "--rename-section"
            )
        }) {
            return Verdict::confirm("git config write requires confirmation");
        }
        let positionals = args[1..].iter().filter(|a| !a.starts_with('-')).count();
        if positionals >= 2 {
            return Verdict::confirm("git config write requires confirmation");
        }
        return Verdict::allow();
    }
    // 两态子命令：写词元出现才 confirm，纯读保持 allow。
    if let Some((_, tokens)) = GIT_ACTION_WRITE_TOKENS.iter().find(|(s, _)| *s == sub) {
        if args[1..].iter().any(|a| {
            tokens
                .iter()
                .any(|t| a == t || a.strip_prefix(&format!("{t}=")).is_some())
        }) {
            return Verdict::confirm(format!("git {sub} write requires confirmation"));
        }
        return Verdict::allow();
    }
    if GIT_READONLY.contains(&sub) {
        if has_write_flag(&args[1..]) {
            return Verdict::confirm("git read subcommand with write flag");
        }
        return Verdict::allow();
    }
    if GIT_SAFE_WRITE.contains(&sub) {
        if command_path_escapes(cmd, project) {
            return Verdict::confirm("path escapes repository");
        }
        return Verdict::allow();
    }
    Verdict::confirm(format!("git {sub} requires confirmation"))
}

fn toolchain_verdict(cmd: &SimpleCommand, project: &Path) -> Verdict {
    if command_path_escapes(cmd, project) {
        return Verdict::confirm("path escapes repository");
    }
    Verdict::allow()
}

/// 任一路径词元逃逸仓库。
fn command_path_escapes(cmd: &SimpleCommand, project: &Path) -> bool {
    cmd.args().iter().any(|w| path_escapes(w, project))
}

fn has_write_flag(args: &[String]) -> bool {
    args.iter().any(|w| {
        let flag = w.split('=').next().unwrap_or(w);
        WRITE_FLAGS.contains(&flag) || WRITE_FLAG_PREFIXES.iter().any(|p| w.starts_with(p))
    })
}

fn is_find_mutation(word: &str) -> bool {
    matches!(word, "-delete" | "-exec" | "-execdir" | "-ok" | "-okdir")
        || word.starts_with("-delete")
}

/// 管道任一下游为 shell/解释器 sink → deny（curl|sh 类）。
///
/// flatten 保序（源码顺序），因此相邻命令即管道相邻段；`|` 连接符在
/// tree-sitter-bash 的 pipeline 节点中不产生独立词元，且 list（`;`/`&&`）会
/// 切断相邻性，故「相邻即管道」不会误报分号连接的无关命令。为规避歧义，
/// sink 判定基于原始命令行中的 `|`：对每条含 `|` 的行片段检查其下游。
pub fn pipe_to_shell(source: &str, commands: &[SimpleCommand]) -> bool {
    let _ = commands;
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

// ---------------------------------------------------------------------------
// 顶层入口
// ---------------------------------------------------------------------------

/// 顶层判定：解析 → 管道 sink → 逐条分类 → 组合裁决。
pub fn decide(command: &str) -> Verdict {
    decide_in(command, &project_root())
}

/// 测试辅助：以指定仓库根判定（单测不依赖环境变量）。
pub fn decide_in(command: &str, project: &Path) -> Verdict {
    decide_with(command, project, &|cmd, project, _| classify(cmd, project))
}

/// 规则注入式顶层判定：管线原语（解析拉平、管道 sink、组合裁决）固定，
/// 单命令分类器由调用方注入（内置判定表或 `rules.toml` 查表 + 脚本层）。
/// 管道拓扑特征（整条命令行级）作为第三参传入分类器，供脚本谓词消费。
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
    let pipe = pipe_to_shell(command, &commands);
    if pipe {
        return Verdict::deny("pipe to shell blocked");
    }
    Verdict::combine(commands.iter().map(|c| classify(c, project, pipe)))
}
