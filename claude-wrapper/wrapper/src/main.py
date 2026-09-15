"""Claude Code → OpenAI-compatible wrapper.

Exposes Claude Code (via `claude-agent-sdk`) behind an OpenAI Chat Completions
API so A3T/FlowR `call:llm` can use it as an ordinary LLM resource. FlowR MCP is
pre-wired in /workspace/.mcp.json and enabled with ENABLE_FLOWR_MCP=true.

Auth: the Claude Code CLI reads ANTHROPIC_API_KEY from the environment.
"""

import json
import os
import time
import uuid

from fastapi import FastAPI, HTTPException
from fastapi.responses import StreamingResponse

from claude_agent_sdk import (
    AssistantMessage,
    ClaudeAgentOptions,
    ResultMessage,
    TextBlock,
    query,
)

app = FastAPI(title="Claude Code OpenAI-compatible wrapper", version="0.1.0")

CWD = os.environ.get("CLAUDE_CWD", "/workspace")
# Default model exposed + used unless the request names a Claude model/alias.
DEFAULT_MODEL = os.environ.get("CLAUDE_MODEL", "sonnet")
ENABLE_MCP = os.environ.get("ENABLE_FLOWR_MCP", "false").lower() in ("1", "true", "yes")
PERMISSION_MODE = os.environ.get("CLAUDE_PERMISSION_MODE", "bypassPermissions")


def _load_mcp_servers():
    """FlowR MCP config from /workspace/.mcp.json (only when explicitly enabled)."""
    if not ENABLE_MCP:
        return None
    try:
        with open(os.path.join(CWD, ".mcp.json")) as fh:
            return json.load(fh).get("mcpServers") or None
    except Exception:
        return None


def _flatten(content):
    """OpenAI message content may be a string or a list of parts."""
    if isinstance(content, str):
        return content
    if isinstance(content, list):
        return "".join(
            part.get("text", "") for part in content if isinstance(part, dict) and part.get("type") in (None, "text")
        )
    return "" if content is None else str(content)


def _render(messages):
    """Split system prompt out, render the remaining turns into a single prompt."""
    system_parts, convo = [], []
    for m in messages:
        role = m.get("role")
        text = _flatten(m.get("content"))
        if role == "system":
            system_parts.append(text)
        elif role in ("user", "assistant", "tool"):
            convo.append(f"{role}: {text}")
    system = "\n\n".join(p for p in system_parts if p).strip() or None
    prompt = "\n\n".join(convo).strip()
    return system, prompt


def _claude_model(requested: str | None) -> str | None:
    """Only pass a model to Claude when it's actually a Claude model/alias —
    otherwise let the CLI pick its default (A3T may send an OpenAI-style name)."""
    if not requested:
        return DEFAULT_MODEL if DEFAULT_MODEL else None
    r = requested.lower()
    if r.startswith("claude") or r in ("sonnet", "opus", "haiku"):
        return requested
    return DEFAULT_MODEL if DEFAULT_MODEL else None


async def _run(system, prompt, model):
    kwargs = {"cwd": CWD, "permission_mode": PERMISSION_MODE}
    if system:
        kwargs["system_prompt"] = system
    m = _claude_model(model)
    if m:
        kwargs["model"] = m
    mcp = _load_mcp_servers()
    if mcp:
        kwargs["mcp_servers"] = mcp
    options = ClaudeAgentOptions(**kwargs)

    text, result = "", None
    async for message in query(prompt=prompt, options=options):
        if isinstance(message, AssistantMessage):
            for block in message.content:
                if isinstance(block, TextBlock):
                    text += block.text
        elif isinstance(message, ResultMessage):
            result = message

    final = (getattr(result, "result", None) or text or "").strip()
    usage = {"prompt_tokens": 0, "completion_tokens": 0, "total_tokens": 0}
    u = getattr(result, "usage", None) if result else None
    if isinstance(u, dict):
        usage["prompt_tokens"] = int(u.get("input_tokens", 0) or 0)
        usage["completion_tokens"] = int(u.get("output_tokens", 0) or 0)
        usage["total_tokens"] = usage["prompt_tokens"] + usage["completion_tokens"]
    return final, usage


@app.get("/health")
def health():
    return {"ok": True, "mcp": ENABLE_MCP}


@app.get("/v1/models")
def models():
    return {"object": "list", "data": [{"id": DEFAULT_MODEL, "object": "model", "owned_by": "anthropic"}]}


@app.post("/v1/chat/completions")
async def chat_completions(body: dict):
    messages = body.get("messages") or []
    model = body.get("model") or DEFAULT_MODEL
    stream = bool(body.get("stream"))
    system, prompt = _render(messages)
    if not prompt:
        raise HTTPException(status_code=400, detail="no user/assistant message to send")

    try:
        text, usage = await _run(system, prompt, model)
    except Exception as exc:  # noqa: BLE001 — surface any SDK/CLI failure to the client
        raise HTTPException(status_code=502, detail=f"claude error: {exc}") from exc

    cid = "chatcmpl-" + uuid.uuid4().hex[:24]
    created = int(time.time())

    if stream:
        def gen():
            head = {
                "id": cid, "object": "chat.completion.chunk", "created": created, "model": model,
                "choices": [{"index": 0, "delta": {"role": "assistant", "content": text}, "finish_reason": None}],
            }
            yield f"data: {json.dumps(head)}\n\n"
            tail = {
                "id": cid, "object": "chat.completion.chunk", "created": created, "model": model,
                "choices": [{"index": 0, "delta": {}, "finish_reason": "stop"}],
            }
            yield f"data: {json.dumps(tail)}\n\n"
            yield "data: [DONE]\n\n"

        return StreamingResponse(gen(), media_type="text/event-stream")

    return {
        "id": cid,
        "object": "chat.completion",
        "created": created,
        "model": model,
        "choices": [{"index": 0, "message": {"role": "assistant", "content": text}, "finish_reason": "stop"}],
        "usage": usage,
    }
