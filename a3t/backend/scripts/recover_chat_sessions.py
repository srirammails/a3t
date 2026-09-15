#!/usr/bin/env python3
"""Reconstruct pre-rollout chat conversations from execution history.

Transcript rows written before owner-stamping carried a 1800s TTL and are gone
(LanceStateStore deletes an expired row on read). Their turns survive in the
execution history, though: each chat run persisted `input.message` and
`output.content`. This rebuilds one session row per conversation and points the
owner's index at them.

Ownership is ASSIGNED, not restored — no execution row recorded a principal, so
there is nothing to recover it from. Everything is attributed to the id passed in.

Usage:
    python3 recover_chat_sessions.py <bearer-token> <owner-principal-id> [--apply]

Without --apply it is a dry run and writes nothing. Run the dry run first: the
index write MERGES into the owner's existing list, and a misread there would
replace live sessions with only the recovered ones.
"""
import json, sys, urllib.request

API   = "https://console.dev.a3t.io"
# Only the chat *wrappers*. system/agent-engine and system/agent-template are the
# subflows these dispatch to and repeat the same session_id + message, so
# including them would duplicate every turn.
CHAT_WORKFLOWS = {("a3t", "a3t-chat-engine"), ("a3t", "a3t-chat-agent")}

def rpc(tok, name, args):
    req = urllib.request.Request(f"{API}/message",
        data=json.dumps({"jsonrpc":"2.0","id":1,"method":"tools/call",
                         "params":{"name":name,"arguments":args}}).encode(),
        headers={"Content-Type":"application/json","Authorization":f"Bearer {tok}"})
    with urllib.request.urlopen(req, timeout=120) as r:
        d = json.load(r)
    res = d.get("result", {})
    text = (res.get("content") or [{}])[0].get("text", "")
    if res.get("isError"):
        raise RuntimeError(text)
    try: return json.loads(text)
    except Exception: return text

def js(v):
    if isinstance(v, str):
        try: return json.loads(v)
        except Exception: return {}
    return v or {}

def derive_title(msgs):
    """Same rule as the server's derive_title, so recovered rows look native."""
    first = next((m["content"].strip() for m in msgs
                  if m["role"] == "user" and m["content"].strip()), "New conversation")
    one = " ".join(first.split())
    return one[:60] + ("…" if len(one) > 60 else "")

def main():
    tok, owner, apply = sys.argv[1], sys.argv[2], "--apply" in sys.argv
    # list_recent applies `limit` with NO ordering, and the tool over-fetches
    # only `limit.max(500)` — so a small limit samples an arbitrary window of
    # storage and silently hides older runs. Ask for far more than the table
    # holds so the whole history is considered.
    rows = rpc(tok, "list_executions", {"limit": 100000})
    rows = rows if isinstance(rows, list) else rows.get("executions", [])

    by = {}
    for r in sorted(rows, key=lambda x: x.get("started_at") or ""):
        if (r.get("namespace"), r.get("name")) not in CHAT_WORKFLOWS:
            continue
        inp = js(r.get("input"))
        sid = inp.get("session_id")
        msg = inp.get("message")
        if not sid or not isinstance(msg, str) or not msg.strip():
            continue
        out = js(r.get("output"))
        ans = out.get("content") if isinstance(out, dict) else None
        turns = by.setdefault(sid, [])
        turns.append({"role": "user", "content": msg})
        if isinstance(ans, str) and ans.strip():
            turns.append({"role": "assistant", "content": ans})
        by.setdefault(f"__meta__{sid}", {})["updated_at"] = (
            r.get("completed_at") or r.get("started_at"))

    sessions = {k: v for k, v in by.items() if not k.startswith("__meta__")}
    print(f"{len(sessions)} conversations, "
          f"{sum(len(v) for v in sessions.values())} messages\n")

    index = []
    for sid, msgs in sessions.items():
        meta = by.get(f"__meta__{sid}", {})
        updated = meta.get("updated_at") or ""
        title = derive_title(msgs)
        key = f"agent:session:{sid}"
        value = {
            "messages": msgs,
            "last_response": next((m["content"] for m in reversed(msgs)
                                   if m["role"] == "assistant"), None),
            "iteration_count": 0,
            "tool_calls_made": 0,
            "owner": owner,
            "owner_kind": "human",
            "updated_at": updated,
            # Marks these as reconstructed, so a later audit can tell an assigned
            # owner from a recorded one.
            "recovered": True,
            "recovered_from": "execution_history",
        }
        index.append({"session_id": sid, "title": title,
                      "message_count": len(msgs), "updated_at": updated})
        print(f"  {'WRITE' if apply else 'would write'} {key}")
        print(f"        {len(msgs)} msgs | {updated[:19]} | {title[:52]}")
        if apply:
            rpc(tok, "set_state", {"key": key, "value": value})   # no ttl_secs → no expiry

    # Merge into the owner's index rather than replacing it: today's real
    # sessions must survive.
    ikey = f"agent:sessions:{owner}"
    existing = []
    try:
        cur = rpc(tok, "get_state", {"key": ikey})
        # get_state omits `found` on a HIT and only sets `found: false` on a miss,
        # so keying off `found` read a populated index as empty — which would have
        # replaced today's real sessions with only the recovered ones. Test the
        # value instead.
        if isinstance(cur, dict) and isinstance(cur.get("value"), dict):
            existing = cur["value"].get("sessions") or []
        if not existing and isinstance(cur, dict) and cur.get("value") is not None \
           and not isinstance(cur.get("value"), dict):
            raise RuntimeError(f"unexpected index shape: {cur!r}")
    except Exception as e:
        print("  (could not read existing index:", e, ")")
    have = {e["session_id"] for e in existing}
    merged = existing + [e for e in index if e["session_id"] not in have]
    merged.sort(key=lambda e: e.get("updated_at") or "", reverse=True)
    print(f"\nindex {ikey}: {len(existing)} existing + "
          f"{len(merged) - len(existing)} recovered = {len(merged)}")
    if apply:
        rpc(tok, "set_state", {"key": ikey, "value": {"owner": owner, "sessions": merged}})
        print("index written")
    else:
        print("(dry run — nothing written)")

main()
