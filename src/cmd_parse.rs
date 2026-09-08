//! bash 命令解析：tree-sitter AST 拉平 + 语义特征提取。
//!
//! 判定以 AST/语义而非正则前缀：复合命令（`echo hi && rm -rf /`）拆开逐条
//! 分类，写 flag（`git show --output=.git/config`）与路径逃逸精确识别。

use std::path::{Path, PathBuf};

use tree_sitter::{Language, Parser, Tree};

/// 解析拉平后的一条简单命令：词元序列 + 各节点原始文本。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SimpleCommand {
    /// 命令词元（按出现顺序，不含重定向操作符本身）。
    pub words: Vec<String>,
    /// 写文件重定向（`>` `>>` `>|` `<>` 且目标非丢弃设备）。
    pub writes_redirect: bool,
    /// 写型重定向的目标词元（M7.0 写目标感知；`writes_redirect` 为 true 但
    /// 目标不可识别的异常形态下为空——查表侧对「有写无可查目标」保守处理）。
    pub redirect_targets: Vec<String>,
}

impl SimpleCommand {
    /// 二进制名（首词元）；空命令返回 None。
    pub fn bin(&self) -> Option<&str> {
        self.words.first().map(String::as_str)
    }

    /// 去掉首词元后的参数词元。
    pub fn args(&self) -> &[String] {
        self.words.get(1..).unwrap_or(&[])
    }
}

/// 解析失败原因。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseError(pub String);

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for ParseError {}

fn bash_language() -> Language {
    tree_sitter_bash::LANGUAGE.into()
}

/// 解析并拉平为简单命令序列（含管道两侧、list/andor/compound 展开）。
pub fn flatten_commands(source: &str) -> Result<Vec<SimpleCommand>, ParseError> {
    let mut parser = Parser::new();
    parser
        .set_language(&bash_language())
        .map_err(|e| ParseError(e.to_string()))?;
    let tree: Tree = parser
        .parse(source, None)
        .ok_or_else(|| ParseError("parser returned no tree".into()))?;

    // ERROR 节点 = 语法不完整（如 heredoc 无终止符），与 bashlex ParsingError 同待遇。
    if has_error(&tree) {
        return Err(ParseError("syntax error / incomplete input".into()));
    }

    let mut out = Vec::new();
    collect_commands(tree.root_node(), source, &mut out);
    Ok(out)
}

fn has_error(tree: &Tree) -> bool {
    let mut cursor = tree.walk();
    loop {
        let node = cursor.node();
        if node.is_error() || node.is_missing() {
            return true;
        }
        if cursor.goto_first_child() {
            continue;
        }
        while !cursor.goto_next_sibling() {
            if !cursor.goto_parent() {
                return false;
            }
        }
    }
}

/// 深度优先收集 command 节点，展开容器节点（program/list/pipeline/重定向包裹等）。
fn collect_commands(node: tree_sitter::Node<'_>, source: &str, out: &mut Vec<SimpleCommand>) {
    match node.kind() {
        "command" => out.push(extract_command(node, source)),
        // binary_expression 覆盖 && 与 ||；redirected_statement 是 `cmd > file` 的
        // 顶层包裹（command + file_redirect 兄弟节点）；subshell/compound 递归展开。
        "program"
        | "list"
        | "pipeline"
        | "subshell"
        | "compound_statement"
        | "redirected_statement"
        | "binary_expression"
        | "function_definition"
        | "if_statement"
        | "for_statement"
        | "while_statement"
        | "case_statement" => {
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                collect_commands(child, source, out);
            }
        }
        _ => {}
    }
}

/// 从 command 节点提取词元与写重定向特征。
///
/// `cmd > file` 形态下 file_redirect 是 redirected_statement 里 command 的
/// **兄弟节点**而非子节点，因此遍历 command 的子节点抓不到重定向；这里
/// 向上查父节点（若为 redirected_statement）补扫其 file_redirect 子节点。
fn extract_command(node: tree_sitter::Node<'_>, source: &str) -> SimpleCommand {
    let mut words = Vec::new();
    let mut writes_redirect = false;
    let mut redirect_targets = Vec::new();
    let mut cursor = node.walk();

    for child in node.children(&mut cursor) {
        match child.kind() {
            "command_name" => {
                if let Some(w) = child.child(0) {
                    push_word(w, source, &mut words);
                }
            }
            "word" | "string" | "raw_string" | "concatenation" | "number" => {
                push_word(child, source, &mut words);
            }
            "file_redirect" => {
                let (writes, target) = redirect_writes_file(child, source);
                if writes {
                    writes_redirect = true;
                    if let Some(t) = target {
                        redirect_targets.push(t);
                    }
                }
            }
            "heredoc_redirect" => {}
            _ => {}
        }
    }

    if !writes_redirect
        && node
            .parent()
            .is_some_and(|p| p.kind() == "redirected_statement")
    {
        let mut pcursor = node.walk();
        if let Some(parent) = node.parent() {
            for child in parent.children(&mut pcursor) {
                if child.id() != node.id() && child.kind() == "file_redirect" {
                    let (writes, target) = redirect_writes_file(child, source);
                    if writes {
                        writes_redirect = true;
                        if let Some(t) = target {
                            redirect_targets.push(t);
                        }
                    }
                }
            }
        }
    }

    SimpleCommand {
        words,
        writes_redirect,
        redirect_targets,
    }
}

/// 提取词元文本；字符串节点取引号内内容，拼接节点递归取子词。
fn push_word(node: tree_sitter::Node<'_>, source: &str, out: &mut Vec<String>) {
    match node.kind() {
        "word" | "number" => {
            out.push(text_of(node, source));
        }
        "string" | "raw_string" => {
            out.push(unquote(&text_of(node, source)));
        }
        "concatenation" => {
            let mut cursor = node.walk();
            let mut buf = String::new();
            for child in node.children(&mut cursor) {
                match child.kind() {
                    "word" | "number" => buf.push_str(&text_of(child, source)),
                    "string" | "raw_string" => buf.push_str(&unquote(&text_of(child, source))),
                    _ => {}
                }
            }
            if !buf.is_empty() {
                out.push(buf);
            }
        }
        _ => {}
    }
}

fn text_of(node: tree_sitter::Node<'_>, source: &str) -> String {
    node.utf8_text(source.as_bytes())
        .unwrap_or_default()
        .to_string()
}

/// 去掉包裹引号（保留内部展开语义：此处仅用于词元展示与表匹配）。
fn unquote(raw: &str) -> String {
    let bytes = raw.as_bytes();
    if bytes.len() >= 2
        && (bytes[0] == b'"' && bytes[bytes.len() - 1] == b'"'
            || bytes[0] == b'\'' && bytes[bytes.len() - 1] == b'\'')
    {
        raw[1..raw.len() - 1].to_string()
    } else {
        raw.to_string()
    }
}

/// 判断 file_redirect 是否为真实写文件（排除输入型、fd dup/close、丢弃设备），
/// 并提取写目标词元（M7.0 写目标感知）。
///
/// 节点结构（tree-sitter-bash 0.25）：`file_redirect` 子节点为
/// `[file_descriptor] 操作符 目标`，操作符是匿名节点（`>` `>>` `<` `>&` `&>` 等），
/// 目标为 `word`（文件）或 `number`（fd dup 的目标 fd）。
///
/// 语义与 Python 版 `has_writing_redirect` 一致：
/// - 输入型 `<` `<<` `<<<` 不写；
/// - fd dup（`2>&1`，操作符 `>&`/`<&`）不写；
/// - 目标为 `/dev/null` / `NUL` / `null` / `-` 不写；
/// - 其余输出型（含 fd 作用域 `2> err.txt`、合并重定向 `&> file`）为写。
///
/// 返回（是否写，写目标词元——异常形态无可识别目标时为 None，调用方对
/// 「有写无可查目标」保守处理）。
fn redirect_writes_file(node: tree_sitter::Node<'_>, source: &str) -> (bool, Option<String>) {
    let mut cursor = node.walk();
    let mut op = String::new();
    let mut target: Option<String> = None;

    for child in node.children(&mut cursor) {
        match child.kind() {
            // fd 作用域（如 2> 的 "2"），不影响写判定。
            "file_descriptor" => {}
            // 匿名操作符节点（> >> < >& &> >| 等）。
            "word" | "string" | "raw_string" | "number" | "concatenation" => {
                if target.is_none() {
                    target = Some(unquote(&text_of(child, source)));
                }
            }
            _ => {
                if op.is_empty() {
                    op = text_of(child, source);
                }
            }
        }
    }

    // 输入型重定向永不写。
    if op == "<" || op == "<<" || op == "<<<" {
        return (false, None);
    }
    // fd dup（2>&1）/ fd close（2>&-）：操作符 >& / <& / >&- ，无持久写。
    if op == ">&" || op == "<&" || op == ">&-" {
        return (false, None);
    }

    match target {
        Some(t) if matches!(t.as_str(), "/dev/null" | "null" | "NUL" | "nul" | "-") => {
            (false, None)
        }
        // 无可识别目标（异常形态）：保守视为写、无目标词元可查。
        other => (true, other),
    }
}

// ---------------------------------------------------------------------------
// 仓库边界
// ---------------------------------------------------------------------------

// 项目根解析已收敛至 `config::discover::find_project_root`（环境变量注入或
// cwd 上溯的单一实现；本模块不再持有副本）。

/// 路径是否落在仓库树内（相对路径按仓库根解析；大小写不敏感用于 Windows）。
pub fn inside_repo(path: &str, project_root: &Path) -> bool {
    let expanded = expanduser(path);
    let joined = if Path::new(&expanded).is_absolute() {
        PathBuf::from(&expanded)
    } else {
        project_root.join(&expanded)
    };
    let root = norm(project_root);
    let target = norm(&joined);
    target == root || target.starts_with(&root)
}

fn expanduser(path: &str) -> String {
    if let Some(rest) = path.strip_prefix("~/")
        && let Ok(home) = std::env::var("USERPROFILE").or_else(|_| std::env::var("HOME"))
    {
        return format!("{}/{}", home.trim_end_matches(['/', '\\']), rest);
    }
    path.to_string()
}

fn norm(path: &Path) -> PathBuf {
    // 词法归一化：消除 `..`/`.`（Path 不做，Python 版 abspath 做；否则
    // `mdor\..\..\x` 的 starts_with 误判仍在仓库内）。大小写不敏感仅 Windows。
    let mut parts: Vec<std::ffi::OsString> = Vec::new();
    for comp in path.components() {
        match comp {
            std::path::Component::ParentDir => {
                match parts.last() {
                    // 栈顶是普通组件 → 抵消一个 ..；栈空/已是 ..（根外）→ 保留 ..。
                    Some(top) if top != std::ffi::OsStr::new("..") => {
                        parts.pop();
                    }
                    _ => parts.push("..".into()),
                }
            }
            std::path::Component::CurDir => {}
            other => parts.push(other.as_os_str().to_os_string()),
        }
    }
    let mut out = PathBuf::new();
    for p in parts {
        out.push(p);
    }
    #[cfg(windows)]
    {
        PathBuf::from(out.to_string_lossy().to_lowercase().replace('/', "\\"))
    }
    #[cfg(not(windows))]
    {
        out
    }
}

/// 词元是否为逃逸仓库的路径（`../` 出逃或绝对路径在仓库外）。
pub fn path_escapes(word: &str, project_root: &Path) -> bool {
    if word == "." || word == ".." {
        return !inside_repo(word, project_root);
    }
    let w = word.replace('\\', "/");
    if w.starts_with('-') || w.starts_with('=') {
        return false;
    }
    let is_abs = w.starts_with('/') || (w.len() >= 2 && w.as_bytes()[1] == b':');
    if is_abs || w.contains('/') || w.contains("..") {
        return !inside_repo(&w, project_root);
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    fn one(s: &str) -> SimpleCommand {
        flatten_commands(s)
            .expect("parses")
            .into_iter()
            .next()
            .expect("one command")
    }

    #[test]
    fn redirect_targets_collected_for_write_redirects_only() {
        let c = one("ls > out.txt");
        assert!(c.writes_redirect);
        assert_eq!(c.redirect_targets, ["out.txt"]);

        // 追加/覆盖写都收集；丢弃设备不收集也不算写。
        assert_eq!(one("ls >> log.txt").redirect_targets, ["log.txt"]);
        let c = one("ls > /dev/null");
        assert!(!c.writes_redirect);
        assert!(c.redirect_targets.is_empty());
        // fd dup 与输入型不写。
        assert!(!one("ls 2>&1").writes_redirect);
        assert!(!one("cat < in.txt").writes_redirect);
        // `cmd > file` 顶层包裹形态（目标在兄弟节点）同样收集。
        assert_eq!(one("echo hi > ../outside").redirect_targets, ["../outside"]);
    }

    #[test]
    fn plain_args_do_not_become_redirect_targets() {
        // 读参数词不是重定向目标——M7.0 读豁免的解析层基础。
        let c = one("ls -la ../outside");
        assert!(!c.writes_redirect);
        assert!(c.redirect_targets.is_empty());
        assert_eq!(c.args(), ["-la", "../outside"]);
    }
}
