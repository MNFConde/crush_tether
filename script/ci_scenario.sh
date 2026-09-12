#!/usr/bin/env bash
# agent-matrix CI 场景组（deny / failopen / rewrite / ask / timeout / exitjson）
# 的 setup/assert 两段式（doc/test-and-ci.md「测试方法」节；设计：探针 perm
# 角色直回信封，引擎不在场，物理副作用断言 = 工作区根 ci-exec.txt——mock
# 命令带写文件重定向）。
#
# 用法:
#   ci_scenario.sh setup  <scenario> <probe-dir> <agent-slug> [workspace]
#   ci_scenario.sh assert <scenario> <probe-dir> <agent-slug> [workspace]
#
# - setup:清探针目录与工作区 ci-exec.txt,按场景写控制文件(信封形态按 agent 分支:
#   claudecode/zcode = Claude 式 hookSpecificOutput;crush = 顶层 decision 信封)
# - assert:按场景断言 ci-exec.txt 存在性/内容 + dump 有 perm 裁决记录
#   (failopen 按 agent 分化——2026-09-11 实测定性:claude 无头把 hook 失联兜底为
#   拒绝(permission_denials 回执),crush/zcode 无头放行与交互一致)
# - ask:hook 回 ask/confirm,无头收场定性(2026-09-12 实测:claude=拒绝/crush=放行
#   [run 原生权限自动接受]/zcode=拒绝[2026-09-11 定性])
# - timeout:hook delay 40s 压过注册 timeout(claudecode/crush=30s,zcode 探针=30s),
#   按 agent 分化断言——2026-09-12 实测:claude 无头超时=拒绝(交互 32s 放行的形态
#   分化,「交互如此≠无头如此」又一实例)/crush 无头超时=放行(34s,与 2026-09-09
#   33s 定性一致)/zcode 超时未定性不入环(挂账 §5)
# - exitjson:deny 信封 + exit 2 并发(B4),断言工具被阻断(两通道同向 deny,
#   上游聚合 halt > deny > allow 任一解释下结果一致)
# - headless 调用(每场景一次)与探针注册留在 workflow/调用方,本脚本不感知 agent 启动方式
set -euo pipefail

usage() { echo "usage: $0 {setup|assert} <deny|failopen|rewrite|ask|timeout|exitjson> <probe-dir> <agent-slug> [workspace]" >&2; exit 2; }

[ $# -ge 4 ] || usage
mode="$1"; scenario="$2"; probe_dir="$3"; agent="$4"; ws="${5:-$PWD}"

valid_agent() { case "$1" in claudecode|crush|zcode) return 0 ;; *) return 1 ;; esac; }
valid_agent "$agent" || usage

exec_file="$ws/ci-exec.txt"
dump_file="$probe_dir/dump.jsonl"

# 信封:rewrite 场景 claude/zcode 走全替换,须含 Bash 工具 schema 全部必填字段;
# ask 场景 crush = 空输出(exit 0 无输出 = 「无意见」走原生权限流,契约定稿)
envelope_for() {
  local agent="$1" scenario="$2"
  case "$agent" in
    claudecode|zcode)
      case "$scenario" in
        deny|exitjson) echo '{"hookSpecificOutput":{"hookEventName":"PreToolUse","permissionDecision":"deny"}}' ;;
        ask) echo '{"hookSpecificOutput":{"hookEventName":"PreToolUse","permissionDecision":"ask"}}' ;;
        timeout) echo '{"hookSpecificOutput":{"hookEventName":"PreToolUse","permissionDecision":"allow"}}' ;;
        rewrite) echo '{"hookSpecificOutput":{"hookEventName":"PreToolUse","permissionDecision":"allow","updatedInput":{"command":"echo rewritten-marker > ci-exec.txt","description":"rewritten by ci scenario"}}}' ;;
        *) return 1 ;;
      esac ;;
    crush)
      case "$scenario" in
        deny|exitjson) echo '{"decision":"deny"}' ;;
        ask) echo '' ;;
        timeout) echo '{"decision":"allow"}' ;;
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
      ask)
        envelope_for "$agent" ask > "$probe_dir/perm-out.txt"
        printf '0' > "$probe_dir/perm-exit.txt" ;;
      timeout)
        envelope_for "$agent" timeout > "$probe_dir/perm-out.txt"
        printf '0' > "$probe_dir/perm-exit.txt"
        # delay 40s 压过注册 timeout(claudecode/crush=30s;zcode 探针 30s 亦被压过)
        printf '40' > "$probe_dir/delay.txt" ;;
      exitjson)
        envelope_for "$agent" exitjson > "$probe_dir/perm-out.txt"
        printf '2' > "$probe_dir/perm-exit.txt" ;;
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
      ask)
        # 2026-09-12 定性:无头无宿主应答,三 agent 对 ask/confirm 均收场为拒绝
        [ ! -f "$exec_file" ] || { echo "ASSERT FAIL [ask]: expected headless denial, but tool ran ($exec_file exists)"; exit 1; }
        echo "ASSERT OK [ask]: headless ask resolves as denial (matches 2026-09-12 characterization)" ;;
      timeout)
        # 形态分化(2026-09-12 实测):claude 无头超时=拒绝/交互放行(2026-09-09);
        # crush 无头超时=放行(34s 复证 2026-09-09 33s);zcode 未定性不入环
        if [ "$agent" = "claudecode" ]; then
          [ ! -f "$exec_file" ] || { echo "ASSERT FAIL [timeout]: expected headless denial, but tool ran ($exec_file exists)"; exit 1; }
          echo "ASSERT OK [timeout]: claude headless hook-timeout resolves as denial (2026-09-12)"
        else
          [ -f "$exec_file" ] || { echo "ASSERT FAIL [timeout]: tool not executed after hook timeout"; exit 1; }
          echo "ASSERT OK [timeout]: hook killed at registered timeout, tool proceeded"
        fi ;;
      exitjson)
        # exit 2 与 JSON deny 并发:两通道同向,工具必被阻断(B4,上游聚合语义引用)
        [ ! -f "$exec_file" ] || { echo "ASSERT FAIL [exitjson]: tool executed despite deny x exit-2 ($exec_file exists)"; exit 1; }
        echo "ASSERT OK [exitjson]: deny envelope + exit 2 concur on block" ;;
      *) usage ;;
    esac
    ;;
esac
