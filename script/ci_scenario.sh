#!/usr/bin/env bash
# agent-matrix CI 场景组（deny / fail-open / updated_input）的 setup/assert 两段式
# （doc/test-and-ci.md「测试方法」节；设计：探针 perm 角色直回信封，引擎不在场，
# 物理副作用断言 = 工作区根 ci-exec.txt——mock 命令带写文件重定向）。
#
# 用法:
#   ci_scenario.sh setup  <deny|failopen|rewrite> <probe-dir> <agent-slug> [workspace]
#   ci_scenario.sh assert <deny|failopen|rewrite> <probe-dir> <agent-slug> [workspace]
#
# - setup:清探针目录与工作区 ci-exec.txt,按场景写控制文件(信封形态按 agent 分支:
#   claudecode/zcode = Claude 式 hookSpecificOutput;crush = 顶层 decision 信封)
# - assert:按场景断言 ci-exec.txt 存在性/内容 + dump 有 perm 裁决记录
#   (failopen 按 agent 分化——2026-09-11 实测定性:claude 无头把 hook 失联兜底为
#   拒绝(permission_denials 回执),crush/zcode 无头放行与交互一致)
# - headless 调用(每场景一次)与探针注册留在 workflow/调用方,本脚本不感知 agent 启动方式
set -euo pipefail

usage() { echo "usage: $0 {setup|assert} <deny|failopen|rewrite> <probe-dir> <agent-slug> [workspace]" >&2; exit 2; }

[ $# -ge 4 ] || usage
mode="$1"; scenario="$2"; probe_dir="$3"; agent="$4"; ws="${5:-$PWD}"

valid_agent() { case "$1" in claudecode|crush|zcode) return 0 ;; *) return 1 ;; esac; }
valid_agent "$agent" || usage

exec_file="$ws/ci-exec.txt"
dump_file="$probe_dir/dump.jsonl"

# 信封:rewrite 场景 claude/zcode 走全替换,须含 Bash 工具 schema 全部必填字段
envelope_for() {
  local agent="$1" scenario="$2"
  case "$agent" in
    claudecode|zcode)
      case "$scenario" in
        deny) echo '{"hookSpecificOutput":{"hookEventName":"PreToolUse","permissionDecision":"deny"}}' ;;
        rewrite) echo '{"hookSpecificOutput":{"hookEventName":"PreToolUse","permissionDecision":"allow","updatedInput":{"command":"echo rewritten-marker > ci-exec.txt","description":"rewritten by ci scenario"}}}' ;;
        *) return 1 ;;
      esac ;;
    crush)
      case "$scenario" in
        deny) echo '{"decision":"deny"}' ;;
        rewrite) echo '{"decision":"allow","updated_input":{"command":"echo rewritten-marker > ci-exec.txt"}}' ;;
        *) return 1 ;;
      esac ;;
    *) return 1 ;;
  esac
}

case "$mode" in
  setup)
    rm -rf "$probe_dir"
    mkdir -p "$probe_dir"
    rm -f "$exec_file"
    case "$scenario" in
      deny)
        envelope_for "$agent" deny > "$probe_dir/perm-out.txt"
        printf '0' > "$probe_dir/perm-exit.txt" ;;
      failopen)
        printf '3' > "$probe_dir/perm-exit.txt" ;;
      rewrite)
        envelope_for "$agent" rewrite > "$probe_dir/perm-out.txt"
        printf '0' > "$probe_dir/perm-exit.txt" ;;
      *) usage ;;
    esac
    echo "scenario [$scenario] armed (agent=$agent, probe-dir=$probe_dir)"
    ;;
  assert)
    [ -f "$dump_file" ] || { echo "ASSERT FAIL [$scenario]: no probe dump (hook never fired)"; exit 1; }
    grep -q -- '-perm' "$dump_file" || { echo "ASSERT FAIL [$scenario]: no perm verdict in dump"; exit 1; }
    case "$scenario" in
      deny)
        [ ! -f "$exec_file" ] || { echo "ASSERT FAIL [deny]: tool executed despite deny ($exec_file exists)"; exit 1; }
        echo "ASSERT OK [deny]: verdict reached, tool blocked" ;;
      failopen)
        if [ "$agent" = "claudecode" ]; then
          [ ! -f "$exec_file" ] || { echo "ASSERT FAIL [failopen]: expected headless denial, but tool ran ($exec_file exists)"; exit 1; }
          echo "ASSERT OK [failopen]: claude headless treats hook loss as denial (matches 2026-09-11 characterization)"
        else
          [ -f "$exec_file" ] || { echo "ASSERT FAIL [failopen]: tool not executed after exit-3 fail-open"; exit 1; }
          echo "ASSERT OK [failopen]: non-2 exit tolerated, tool ran"
        fi ;;
      rewrite)
        [ -f "$exec_file" ] || { echo "ASSERT FAIL [rewrite]: rewritten command never ran"; exit 1; }
        grep -q 'rewritten-marker' "$exec_file" || { echo "ASSERT FAIL [rewrite]: ci-exec.txt lacks rewritten-marker (original command ran?)"; exit 1; }
        echo "ASSERT OK [rewrite]: rewritten command executed" ;;
      *) usage ;;
    esac
    ;;
esac
