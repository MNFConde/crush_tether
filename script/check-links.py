#!/usr/bin/env python3
"""check-links.py — 校验 doc/ 与 cairn/ 各 Markdown 的跨文件与站内锚点引用一致性。

用法:
  uv run --directory script check-links.py
  uv run --directory script check-links.py --doc-root <doc目录> [--cairn-root <cairn目录>]

扫描 doc/ 与 cairn/ 顶层全部 *.md（不递归，doc 侧自然排除 archive_doc_v* 子目录）;
解析时跳过 fenced code block 与行内反引号代码，避免把示例代码当链接校验。
对形如 `](…md#anchor)` 的跨文件链接与形如 `](#anchor)` 的站内链接,
跨文件目标按「源文件所在目录」的相对路径解析（如 cairn/ 内指 doc/ 用 `../doc/x.md`），
逐一与目标文件标题生成的 GitHub slug 比对; 有任何不匹配时列出并返回
exit code 1 (供 CI/本地检查).
"""

import argparse
import re
import sys
import unicodedata
from pathlib import Path

HEADING_RE = re.compile(r"^#{1,6} ")
LINK_RE = re.compile(r"\]\(([^)]+)\)")
TARGET_RE = re.compile(r"^([^#]*)#(.+)$")
INLINE_CODE_RE = re.compile(r"`[^`]*`")


def github_slug(heading: str) -> str:
    out = ""
    for ch in heading.lower():
        if unicodedata.category(ch)[0] in ("L", "N") or ch in (" ", "-"):
            out += ch
    out = out.strip()
    while "  " in out:
        out = out.replace("  ", " ")
    return out.replace(" ", "-")


def headings_of(path: Path) -> list[str]:
    out = []
    for line in path.read_text(encoding="utf-8").splitlines():
        if HEADING_RE.match(line):
            out.append(re.sub(r"^#+\s*", "", line))
    return out


def main() -> int:
    parser = argparse.ArgumentParser(description="校验 doc/ 与 cairn/ 锚点引用一致性")
    parser.add_argument(
        "--doc-root",
        type=Path,
        default=Path(__file__).resolve().parent.parent / "doc",
        help="doc 目录（默认仓库根下 doc/）",
    )
    parser.add_argument(
        "--cairn-root",
        type=Path,
        default=Path(__file__).resolve().parent.parent / "cairn",
        help="cairn 目录（默认仓库根下 cairn/；传不存在的路径可只查 doc/）",
    )
    args = parser.parse_args()

    # 目标全集 = doc/ 与 cairn/ 顶层 *.md（既是扫描对象，也是合法的跨文件链接目标；glob 不递归）
    universe: dict[Path, str] = {}
    sources: list[tuple[str, Path]] = []
    for label, base in (("doc", args.doc_root), ("cairn", args.cairn_root)):
        if not base.is_dir():
            continue
        for p in sorted(base.glob("*.md")):
            resolved = p.resolve()
            universe[resolved] = p.name
            sources.append((f"{label}/{p.name}", resolved))
    cache: dict[Path, list[str]] = {}
    issues: list[tuple[str, int, str, str]] = []
    checked = 0

    for display, path in sources:
        lines = path.read_text(encoding="utf-8").splitlines()
        in_fence = False
        for i, line in enumerate(lines):
            if line.startswith("```"):
                # 围栏行只翻转状态，本身不提取链接
                in_fence = not in_fence
                continue
            if in_fence:
                # 围栏（代码块/mermaid 块）内的行跳过
                continue
            # 先剔除行内反引号代码片段，避免把 `[文本](#锚点)` 之类当链接校验
            text = INLINE_CODE_RE.sub(" ", line)
            for m in LINK_RE.finditer(text):
                inner = m.group(1).strip()
                tm = TARGET_RE.match(inner)
                if not tm:
                    continue
                target, anchor = tm.group(1).strip(), tm.group(2)
                if target == "":
                    # 站内链接：锚点对本文件标题
                    target_display = display
                    heads = cache.setdefault(path, headings_of(path))
                else:
                    # 跨文件链接：按源文件所在目录解析相对路径，目标须在全集内
                    resolved = (path.parent / target).resolve()
                    if resolved not in universe:
                        continue
                    target_display = universe[resolved]
                    heads = cache.setdefault(resolved, headings_of(resolved))
                checked += 1
                ok = any(github_slug(h) == anchor for h in heads)
                if not ok:
                    issues.append((display, i + 1, target_display, anchor))

    print(f"checked {checked} anchor link(s)")
    if not issues:
        print("OK: all anchors resolve")
        return 0
    for file, lineno, target, anchor in issues:
        print(f"MISMATCH {file}:{lineno} -> {target}#{anchor}")
    return 1


if __name__ == "__main__":
    sys.stdout.reconfigure(encoding="utf-8")
    sys.exit(main())
