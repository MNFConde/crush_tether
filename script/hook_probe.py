#!/usr/bin/env python3
"""hook 探针（M7.2，design.md「hook 探针方法（定稿）」的 python 参考实现）。

零第三方依赖，经 ``uv run python hook_probe.py <role>`` 调用；注册到任意项目的
zcode 工作区 hook（``type:"process"``，command=uv，args 向量），实测「hook 真被
拉起、载荷与裁决真的流转」。

角色（与退役的 node 探针 probe.js 语义 1:1）：
- bash  ：dump stdin 载荷 → 转发真实引擎（``<engine> hook --agent <slug>``）→
          原样回传引擎 stdout 与退出码（观测真实链路）
- perm  ：dump stdin → 探针目录有 perm-out.txt 则原样写到 stdout（模拟许可信封），
          退出码取 perm-exit.txt（默认 0）——观测 agent 如何采纳回包/退出码
- post  ：仅 dump stdin（PostToolUse 只有执行结果、无用户选择回传）
- fail  ：按 fail-exit.txt 内容作退出码（默认 0），dump 截断 stdin——模拟 hook
          进程异常退出，观测 fail-open / fail-closed 语义

控制文件（在探针目录内，换实验改文件、不改注册、不重启会话）：
- perm-out.txt   perm 角色回包内容（缺席 = 静默 exit 0 放行）
- perm-exit.txt  perm 角色退出码覆写
- fail-exit.txt  fail 角色退出码覆写
- delay.txt      全角色通用：dump 载荷后挂起 N 秒再继续（浮点秒，缺席/非法 = 0）
                 ——模拟 hook 慢/挂死，观测 agent 超时与杀进程行为

用法：
    uv run python hook_probe.py <bash|perm|post|fail>
        [--probe-dir DIR] [--engine EXE] [--agent SLUG] [--source TAG]

探针目录解析：--probe-dir > 环境变量 HOOK_PROBE_DIR > ``<cwd>/.zcode/hook-probe``；
dump 与控制文件都在该目录。引擎解析：--engine > 环境变量 CRUSH_TETHER_EXE >
``crush-tether``（裸命令名 PATH 解析，与正式插件分发路线一致）。
"""

from __future__ import annotations

import argparse
import json
import os
import subprocess
import sys
import time
from datetime import datetime, timezone
from pathlib import Path

DUMP_NAME = "dump.jsonl"
CONTROL_FILES = ("perm-out.txt", "perm-exit.txt", "fail-exit.txt", "delay.txt")


def resolve_probe_dir(arg_value: str | None) -> Path:
    if arg_value:
        return Path(arg_value)
    env = os.environ.get("HOOK_PROBE_DIR")
    if env:
        return Path(env)
    return Path.cwd() / ".zcode" / "hook-probe"


def read_control(probe_dir: Path, name: str) -> str | None:
    try:
        return (probe_dir / name).read_text(encoding="utf-8").strip()
    except OSError:
        return None


def control_exit_code(probe_dir: Path, name: str) -> int:
    raw = read_control(probe_dir, name)
    if raw is None:
        return 0
    try:
        return int(raw)
    except ValueError:
        return 0


def control_delay(probe_dir: Path) -> float:
    raw = read_control(probe_dir, "delay.txt")
    if raw is None:
        return 0.0
    try:
        return max(0.0, float(raw))
    except ValueError:
        return 0.0


def dump(probe_dir: Path, role_tag: str, tag: str, **extra: object) -> None:
    """JSONL 追加一条观测记录；探针自身故障不外泄（不改变被观测链路的行为）。"""
    try:
        probe_dir.mkdir(parents=True, exist_ok=True)
        record = {
            "ts": datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%S.%f")[:-3] + "Z",
            "role": role_tag,
            "tag": tag,
            **extra,
        }
        with (probe_dir / DUMP_NAME).open("a", encoding="utf-8") as f:
            f.write(json.dumps(record, ensure_ascii=False) + "\n")
    except OSError:
        pass


def read_stdin() -> str:
    return sys.stdin.buffer.read().decode("utf-8", errors="replace")


def main() -> int:
    parser = argparse.ArgumentParser(description="crush_tether hook 探针")
    parser.add_argument("role", choices=("bash", "perm", "post", "fail"))
    parser.add_argument("--probe-dir", default=None, help="dump 与控制文件目录（默认 <cwd>/.zcode/hook-probe）")
    parser.add_argument("--engine", default=None, help="引擎可执行（默认 crush-tether，PATH 解析）")
    parser.add_argument("--agent", default="zcode", help="转发给引擎的 --agent slug")
    parser.add_argument("--source", default="ws", help="注册来源标记（配置轨/插件轨区分用）")
    args = parser.parse_args()

    probe_dir = resolve_probe_dir(args.probe_dir)
    role_tag = f"{args.source}-{args.role}"
    delay = control_delay(probe_dir)

    if args.role == "fail":
        code = control_exit_code(probe_dir, "fail-exit.txt")
        dump(probe_dir, role_tag, f"fail-exit={code}", stdin=read_stdin()[:500], delay=delay)
        if delay:
            time.sleep(delay)
        return code

    stdin = read_stdin()
    dump(probe_dir, role_tag, "stdin", stdin=stdin, delay=delay)
    if delay:
        time.sleep(delay)

    if args.role == "post":
        return 0

    if args.role == "perm":
        perm_out = read_control(probe_dir, "perm-out.txt")
        if perm_out is not None:
            sys.stdout.buffer.write(perm_out.encode("utf-8"))
            sys.stdout.buffer.flush()
        code = control_exit_code(probe_dir, "perm-exit.txt")
        dump(probe_dir, role_tag, f"perm-exit={code}")
        return code

    # role == "bash"：转发真实引擎，原样回传 stdout / 退出码
    engine = args.engine or os.environ.get("CRUSH_TETHER_EXE") or "crush-tether"
    try:
        proc = subprocess.run(
            [engine, "hook", "--agent", args.agent],
            input=stdin.encode("utf-8"),
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            timeout=30,
        )
    except (OSError, subprocess.TimeoutExpired) as exc:
        dump(probe_dir, role_tag, "engine-error", error=repr(exc))
        print(f"hook_probe: engine {engine!r} 转发失败：{exc}", file=sys.stderr)
        return 1
    dump(
        probe_dir,
        role_tag,
        "engine",
        status=proc.returncode,
        stdout=proc.stdout.decode("utf-8", errors="replace")[:2000],
        stderr=proc.stderr.decode("utf-8", errors="replace")[:2000],
        error=None,
    )
    sys.stdout.buffer.write(proc.stdout)
    sys.stdout.buffer.flush()
    return proc.returncode


if __name__ == "__main__":
    sys.exit(main())
