#!/usr/bin/env python3
"""M7.3 agent-matrix CI：本地双协议 LLM mock——Anthropic /v1/messages + OpenAI /v1/chat/completions。

固定一轮 tool_use（echo mock-hook-test），见到工具结果回 end_turn。stream（SSE）与
非流式都支持。hook 链路是 agent 本地行为、与 LLM 无关，故 mock 驱动即可零凭证走完
agent 对话与 hook 全链（doc/test-and-ci.md「测试方法」节）。

要点（三坑实录，勿回退）：
- 工具参数必须含 agent schema 的全部必填字段（Bash 的 description 缺失会被
  crush/fantasy 静默拒绝，error tool result 无任何告警）
- 工具名从请求的 tools 列表自适应选取（Windows 下 claude -p 可能提供
  PowerShell 而无 Bash；盲发 Bash 会报 No such tool available）
- OpenAI 协议的 stream=true 必须回 SSE 分块（role → tool_calls → finish），
  回 JSON 会得到 unexpected EOF
"""
import argparse
import json
from http.server import BaseHTTPRequestHandler, HTTPServer

CMD = "echo mock-hook-test"


def make_mock(log_body, cmd):
    def pick_tool_name(body, default):
        names = [t.get("name") for t in (body.get("tools") or [])]
        if default in names:
            return default
        for cand in ("Bash", "PowerShell", "bash", "powershell"):
            if cand in names:
                return cand
        return default

    def anthropic_plan(body):
        has_tool_result = any(
            isinstance(c, dict) and c.get("type") == "tool_result"
            for m in body.get("messages", []) if m.get("role") == "user"
            for c in (m.get("content") if isinstance(m.get("content"), list) else []))
        if has_tool_result:
            return {"text": "spike done"}
        log_body(body)
        return {"tool": pick_tool_name(body, "Bash")}

    def anthropic(body):
        plan = anthropic_plan(body)
        if "text" in plan:
            content = [{"type": "text", "text": plan["text"]}]
            stop = "end_turn"
        else:
            content = [{"type": "tool_use", "id": "toolu_1", "name": plan["tool"],
                        "input": {"command": cmd, "description": "mock spike echo"}}]
            stop = "tool_use"
        return {"id": "msg_spike", "type": "message", "role": "assistant",
                "model": body.get("model", "m"), "content": content,
                "stop_reason": stop, "stop_sequence": None,
                "usage": {"input_tokens": 10, "output_tokens": 10}}

    def anthropic_sse(body):
        resp = anthropic(body)
        block = resp["content"][0]
        yield ("message_start", {"type": "message_start", "message": {**resp, "content": []}})
        if block["type"] == "text":
            yield ("content_block_start", {"type": "content_block_start", "index": 0,
                                           "content_block": {"type": "text", "text": ""}})
            yield ("content_block_delta", {"type": "content_block_delta", "index": 0,
                                           "delta": {"type": "text_delta", "text": block["text"]}})
        else:
            yield ("content_block_start", {"type": "content_block_start", "index": 0,
                                           "content_block": {"type": "tool_use", "id": block["id"],
                                                             "name": block["name"], "input": {}}})
            yield ("content_block_delta", {"type": "content_block_delta", "index": 0,
                                           "delta": {"type": "input_json_delta",
                                                     "partial_json": json.dumps(block["input"])}})
        yield ("content_block_stop", {"type": "content_block_stop", "index": 0})
        yield ("message_delta", {"type": "message_delta",
                                 "delta": {"stop_reason": resp["stop_reason"],
                                           "stop_sequence": None},
                                 "usage": {"output_tokens": 10}})
        yield ("message_stop", {"type": "message_stop"})

    def openai_plan(body):
        def have_result(m):
            c = m.get("content")
            return m.get("role") == "tool" or (isinstance(c, list)
                                               and any(x.get("type") == "tool_result" for x in c))
        if any(have_result(m) for m in body.get("messages", [])):
            return "bash", "spike done", "stop"
        log_body(body)
        name = pick_tool_name(body, "bash")
        args = json.dumps({"command": cmd, "description": "mock spike echo"})
        return name, args, "tool_calls"

    def openai(body):
        name, payload, finish = openai_plan(body)
        if finish == "stop":
            msg = {"role": "assistant", "content": payload}
        else:
            msg = {"role": "assistant", "content": None,
                   "tool_calls": [{"id": "call_1", "type": "function",
                                   "function": {"name": name, "arguments": payload}}]}
        return {"id": "chatcmpl-spike", "object": "chat.completion", "created": 1,
                "model": body.get("model", "m"),
                "choices": [{"index": 0, "message": msg, "finish_reason": finish}],
                "usage": {"prompt_tokens": 1, "completion_tokens": 1, "total_tokens": 2}}

    def openai_sse(body):
        name, payload, finish = openai_plan(body)
        base = {"id": "chatcmpl-spike", "object": "chat.completion.chunk", "created": 1,
                "model": body.get("model", "m")}
        chunks = [{**base, "choices": [{"index": 0, "delta": {"role": "assistant"},
                                        "finish_reason": None}]}]
        if finish == "tool_calls":
            chunks.append({**base, "choices": [{"index": 0, "delta": {"tool_calls": [
                {"index": 0, "id": "call_1", "type": "function",
                 "function": {"name": name, "arguments": ""}}]}, "finish_reason": None}]})
            chunks.append({**base, "choices": [{"index": 0, "delta": {"tool_calls": [
                {"index": 0, "function": {"arguments": payload}}]}, "finish_reason": None}]})
        else:
            chunks.append({**base, "choices": [{"index": 0, "delta": {"content": payload},
                                                "finish_reason": None}]})
        chunks.append({**base, "choices": [{"index": 0, "delta": {}, "finish_reason": finish}]})
        return "".join(f"data: {json.dumps(c)}\n\n" for c in chunks) + "data: [DONE]\n\n"

    return anthropic, anthropic_sse, openai, openai_sse


def main():
    ap = argparse.ArgumentParser(description="dual-protocol LLM mock for agent-matrix CI")
    ap.add_argument("--port", type=int, default=8787)
    ap.add_argument("--cmd", default=CMD,
                    help="tool_use command served to the agent (default: echo mock-hook-test)")
    ap.add_argument("--log", default="", help="optional request log JSONL (tool names recorded)")
    args = ap.parse_args()

    def log_body(body):
        if not args.log:
            return
        try:
            with open(args.log, "a", encoding="utf-8") as f:
                f.write(json.dumps({"model": body.get("model"),
                                    "tools": [t.get("name") for t in (body.get("tools") or [])]},
                                   ensure_ascii=False) + "\n")
        except OSError as e:
            print(f"mock: log write failed ({e}); continuing", flush=True)

    anthropic, anthropic_sse, openai, openai_sse = make_mock(log_body, args.cmd)

    class Handler(BaseHTTPRequestHandler):
        def do_POST(self):
            n = int(self.headers.get("content-length", 0) or
                    self.headers.get("Content-Length", 0))
            body = json.loads(self.rfile.read(n) or b"{}")
            if "chat/completions" in self.path:
                if body.get("stream"):
                    ct, data = "text/event-stream", openai_sse(body).encode()
                else:
                    ct, data = "application/json", json.dumps(openai(body)).encode()
            else:
                if body.get("stream"):
                    ct = "text/event-stream"
                    data = "".join(f"event: {e}\ndata: {json.dumps(d)}\n\n"
                                   for e, d in anthropic_sse(body)).encode()
                else:
                    ct, data = "application/json", json.dumps(anthropic(body)).encode()
            self.send_response(200)
            self.send_header("content-type", ct)
            self.send_header("content-length", str(len(data)))
            self.end_headers()
            self.wfile.write(data)

        def log_message(self, *a):
            pass

    srv = HTTPServer(("127.0.0.1", args.port), Handler)
    print(f"dual-protocol mock on 127.0.0.1:{args.port}", flush=True)
    srv.serve_forever()


if __name__ == "__main__":
    main()
