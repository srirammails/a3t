#!/usr/bin/env python3
"""Move the RECOVERED chat sessions from one owner to another.

The recovery assigned every reconstructed conversation to whichever principal the
migration ran as. That is not necessarily the human who uses the console: this
deployment has two superadmin identities (the console's local HS256 `usr-superadmin`
and the Keycloak account `superadmin@a3t.io`), and only rows written by the live
chat carry a truthful owner.

So this moves ONLY rows marked `recovered: true` and leaves genuinely-owned
sessions where they are — re-running the recovery under a new owner would instead
re-stamp real sessions too.

    python3 repoint.py <token> <from-owner> <to-owner> [--apply]
"""
import json, sys, urllib.request

API = "https://console.dev.a3t.io"

def rpc(tok, name, args):
    req = urllib.request.Request(f"{API}/message",
        data=json.dumps({"jsonrpc":"2.0","id":1,"method":"tools/call",
                         "params":{"name":name,"arguments":args}}).encode(),
        headers={"Content-Type":"application/json","Authorization":f"Bearer {tok}"})
    with urllib.request.urlopen(req, timeout=120) as r:
        d = json.load(r)
    res = d.get("result", {})
    text = (res.get("content") or [{}])[0].get("text", "")
    if res.get("isError"): raise RuntimeError(text)
    try: return json.loads(text)
    except Exception: return text

def get_state(tok, key):
    o = rpc(tok, "get_state", {"key": key})
    # get_state omits `found` on a hit; only a miss sets found:false.
    v = o.get("value") if isinstance(o, dict) else None
    return v if isinstance(v, dict) else None

def main():
    tok, src, dst = sys.argv[1], sys.argv[2], sys.argv[3]
    apply = "--apply" in sys.argv

    src_idx = get_state(tok, f"agent:sessions:{src}") or {}
    dst_idx = get_state(tok, f"agent:sessions:{dst}") or {}
    src_entries = src_idx.get("sessions") or []
    dst_entries = dst_idx.get("sessions") or []
    print(f"source index {src}: {len(src_entries)} entries")
    print(f"target index {dst}: {len(dst_entries)} entries\n")

    move, keep, missing = [], [], []
    for e in src_entries:
        sid = e["session_id"]
        row = get_state(tok, f"agent:session:{sid}")
        if row is None:
            missing.append(e); continue
        (move if row.get("recovered") else keep).append((e, row))

    print(f"recovered (move to {dst}): {len(move)}")
    print(f"genuinely owned by {src} (keep):   {len(keep)}")
    for e, r in keep: print(f"    keep  {e['session_id']}")
    if missing:
        print(f"index entries with no transcript row (pruned): {len(missing)}")

    if apply:
        for e, row in move:
            row["owner"] = dst
            rpc(tok, "set_state", {"key": f"agent:session:{e['session_id']}", "value": row})
        have = {x["session_id"] for x in dst_entries}
        merged = dst_entries + [e for e, _ in move if e["session_id"] not in have]
        merged.sort(key=lambda x: x.get("updated_at") or "", reverse=True)
        rpc(tok, "set_state", {"key": f"agent:sessions:{dst}",
                               "value": {"owner": dst, "sessions": merged}})
        kept = [e for e, _ in keep]
        kept.sort(key=lambda x: x.get("updated_at") or "", reverse=True)
        rpc(tok, "set_state", {"key": f"agent:sessions:{src}",
                               "value": {"owner": src, "sessions": kept}})
        print(f"\napplied: {dst} -> {len(merged)} entries, {src} -> {len(kept)} entries")
    else:
        print(f"\n(dry run) would give {dst} "
              f"{len(dst_entries) + len([1 for e,_ in move if e['session_id'] not in {x['session_id'] for x in dst_entries}])} "
              f"entries and leave {src} with {len(keep)}")

main()
