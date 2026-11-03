#!/usr/bin/env python3
"""Generate faq.html for a3t.io.

Content and architecture diagrams live in this file. Page chrome (styles, nav,
footer, cookie banner, legal modals) is pulled from index.html on every run so
the two pages never drift. Diagrams are rendered to static inline SVG.

Usage: python3 build-faq.py   (writes faq.html next to this script)
"""
import html
import json
import re
from pathlib import Path

HERE = Path(__file__).parent
CALENDLY = "https://calendly.com/srirammails"

ROLES = {
    "builder": "Builder",
    "gov": "Governance",
    "platform": "Platform",
}

# ─────────────────────────────────────────────────────────────────────────────
# Diagram renderer
#
# A diagram is {w, h, groups, nodes, edges, texts}.
#   group: (id, x, y, w, h, label, kind)
#   node:  (id, x, y, w, h, title, [sub lines], kind)       kind "code" = mono block
#   edge:  dict(a="node:side[@coord]", b=..., l=label, k=kind, dash=bool, via=[(x,y)], lp=(x,y))
#   text:  (x, y, text, anchor)
# Sides: t b l r. "@coord" pins the port to an absolute x (t/b) or y (l/r).
# ─────────────────────────────────────────────────────────────────────────────

def esc(s):
    return html.escape(s, quote=True)


def port(boxes, spec):
    nid, _, side = spec.partition(":")
    side, _, at = side.partition("@")
    x, y, w, h = boxes[nid]
    v = float(at) if at else None
    if side == "t":
        return (v if v is not None else x + w / 2, y), side
    if side == "b":
        return (v if v is not None else x + w / 2, y + h), side
    if side == "l":
        return (x, v if v is not None else y + h / 2), side
    return (x + w, v if v is not None else y + h / 2), side


def route(p1, s1, p2, s2, via):
    if via:
        return [p1, *via, p2]
    horiz = {"l", "r"}
    if s1 in horiz and s2 in horiz:
        if abs(p1[1] - p2[1]) < 0.5:
            return [p1, p2]
        mx = (p1[0] + p2[0]) / 2
        return [p1, (mx, p1[1]), (mx, p2[1]), p2]
    if s1 not in horiz and s2 not in horiz:
        if abs(p1[0] - p2[0]) < 0.5:
            return [p1, p2]
        my = (p1[1] + p2[1]) / 2
        return [p1, (p1[0], my), (p2[0], my), p2]
    if s1 in horiz:
        return [p1, (p2[0], p1[1]), p2]
    return [p1, (p1[0], p2[1]), p2]


def render(did, d):
    W, H = d["w"], d["h"]
    out = [f'<svg class="dg" viewBox="0 0 {W} {H}" role="img" aria-labelledby="{did}-cap" xmlns="http://www.w3.org/2000/svg">']
    kinds = ["n", "wf", "llm", "mem", "pol", "deny", "ok", "lm"]
    out.append("<defs>")
    for k in kinds:
        out.append(f'<marker id="{did}-{k}" viewBox="0 0 10 10" refX="9" refY="5" markerWidth="7" markerHeight="7" orient="auto-start-reverse">'
                   f'<path d="M0 0L10 5L0 10z" class="mk mk-{k}"/></marker>')
    out.append("</defs>")

    boxes = {}
    for g in d.get("groups", []):
        gid, x, y, w, h, label, kind = g
        boxes[gid] = (x, y, w, h)
        out.append(f'<rect x="{x}" y="{y}" width="{w}" height="{h}" rx="14" class="grp g-{kind}"/>')
        out.append(f'<text x="{x + 14}" y="{y + 19}" class="glbl gl-{kind}">{esc(label)}</text>')

    for n in d.get("nodes", []):
        nid, x, y, w, h, title, subs, kind = n
        boxes[nid] = (x, y, w, h)
        out.append(f'<rect x="{x}" y="{y}" width="{w}" height="{h}" rx="9" class="nd k-{kind}"/>')
        if kind == "code":
            for i, line in enumerate(subs):
                out.append(f'<text x="{x + 14}" y="{y + 22 + i * 15}" class="code">{esc(line)}</text>')
            continue
        cx = x + w / 2
        if subs:
            block = 15 + 13.5 * len(subs)
            ty = y + (h - block) / 2 + 11
            out.append(f'<text x="{cx}" y="{ty}" class="nt t-{kind}">{esc(title)}</text>')
            for i, s in enumerate(subs):
                out.append(f'<text x="{cx}" y="{ty + 16 + i * 13.5}" class="ns s-{kind}">{esc(s)}</text>')
        else:
            out.append(f'<text x="{cx}" y="{y + h / 2 + 4.5}" class="nt t-{kind}">{esc(title)}</text>')

    labels = []
    for e in d.get("edges", []):
        p1, s1 = port(boxes, e["a"])
        p2, s2 = port(boxes, e["b"])
        pts = route(p1, s1, p2, s2, e.get("via"))
        k = e.get("k", "n")
        path = "M" + " L".join(f"{px:.1f} {py:.1f}" for px, py in pts)
        dash = " dash" if e.get("dash") else ""
        out.append(f'<path d="{path}" class="edge e-{k}{dash}" marker-end="url(#{did}-{k})"/>')
        if e.get("l"):
            if "lp" in e:
                lx, ly = e["lp"]
            else:
                segs = list(zip(pts, pts[1:]))
                (ax, ay), (bx, by) = max(segs, key=lambda s: abs(s[0][0] - s[1][0]) + abs(s[0][1] - s[1][1]))
                lx, ly = (ax + bx) / 2, (ay + by) / 2
            labels.append((lx, ly, e["l"], k))
    # Labels last so they sit above every edge.
    for lx, ly, text, k in labels:
        tw = len(text) * 6.0 + 12
        out.append(f'<rect x="{lx - tw / 2:.1f}" y="{ly - 8.5:.1f}" width="{tw:.1f}" height="17" rx="8.5" class="lb lb-{k}"/>')
        out.append(f'<text x="{lx:.1f}" y="{ly + 3.6:.1f}" class="lt lt-{k}">{esc(text)}</text>')

    for t in d.get("texts", []):
        x, y, text, anchor = t
        out.append(f'<text x="{x}" y="{y}" class="free" text-anchor="{anchor}">{esc(text)}</text>')
    out.append("</svg>")
    return "\n".join(out)


def E(a, b, l=None, k="n", **kw):
    return dict(a=a, b=b, l=l, k=k, **kw)


# ─────────────────────────────────────────────────────────────────────────────
# Content
# ─────────────────────────────────────────────────────────────────────────────

SECTIONS = [
  {
    "id": "agentlm", "name": "AgentLM", "cat": "AgentLM",
    "items": [
      {
        "id": "a1", "role": "builder",
        "resp": "keeping agent costs under control as usage grows.",
        "pain": "send every request to a frontier LLM, so the bill grows with traffic and I can’t predict it.",
        "lead": "Every request starts at <b>AgentLM’s gate</b>. The first layers of the model read the request and decide: answer directly, think first, or escalate. If agent memory has no procedure, evidence or concept that matches, it escalates no matter what the model thinks. Because the gate is the model’s own first layers, a request it keeps costs nothing extra, and one it escalates costs a fraction of a single pass.",
        "steps": [
          ("Gate first.", "Small heads on the early layers estimate whether AgentLM can solve the request, which skills it needs and how much thinking it deserves."),
          ("Memory coverage is a hard veto.", "No matching knowledge in agent memory means escalate. The model can’t talk itself into a guess."),
          ("Solve locally.", "Kept requests continue through the same pass on a GPU in your tenant. No per-token bill from a third party."),
          ("Escalate with context.", "When it escalates, the frontier LLM gets the request plus the context AgentLM gathered, and the answer comes back through the same checks."),
        ],
        "outcome": "Frontier spend tracks the hard cases, not total traffic. The escalation threshold starts high and comes down only as measured accuracy allows.",
        "cap": "The gate is part of AgentLM. Memory coverage can force escalation; policy decides whether escalation may leave the tenant.",
        "dg": {
          "w": 760, "h": 300,
          "groups": [("t", 10, 14, 560, 272, "YOUR TENANT", "lm")],
          "nodes": [
            ("in", 30, 120, 110, 54, "Request", ["typed input"], "n"),
            ("gate", 180, 50, 170, 56, "Gate", ["early layers + coverage"], "lm"),
            ("lm", 180, 150, 170, 60, "AgentLM", ["solve locally"], "lm"),
            ("mem", 180, 232, 170, 44, "Agent memory", [], "mem"),
            ("pol", 400, 50, 150, 56, "Cedar policy", ["may this leave?"], "pol"),
            ("wf", 400, 150, 150, 60, "Workflow", ["checked · runs"], "wf"),
            ("fr", 610, 50, 140, 56, "Frontier LLM", ["hard cases"], "llm"),
          ],
          "edges": [
            E("in:r", "gate:l"),
            E("gate:b", "lm:t", "keep", k="lm"),
            E("gate:r", "pol:l", "escalate", k="llm"),
            E("pol:r", "fr:l", "allow", k="pol"),
            E("lm:r", "wf:l", "answer", k="lm"),
            E("fr:b", "wf:r", "answer", k="llm", dash=True),
            E("mem:t", "lm:b", "context", k="mem"),
          ],
        },
      },
      {
        "id": "a2", "role": "builder",
        "resp": "agents that hold long conversations and long-running cases.",
        "pain": "forget context, or I keep re-sending ever-longer prompts and pay for the same tokens again on every turn.",
        "lead": "AgentLM doesn’t re-read the conversation each turn. A session is a <b>fixed-size state</b> plus a short window of recent text, saved to agent memory after every turn and restored on the next. Knowledge isn’t pasted into the prompt either: AgentLM <b>attends to memory directly</b>, using attention blocks precomputed from the knowledge graph.",
        "steps": [
          ("Fixed-size session state.", "Most of AgentLM’s layers keep a fixed-size state instead of a growing cache. The session costs the same on turn 500 as on turn 5."),
          ("Recent text, exactly.", "A sliding window keeps the last few thousand tokens verbatim, for exact recall of what was just said."),
          ("Memory as attention, not prompt.", "Knowledge-graph nodes are precomputed into attention blocks stored in agent memory. AgentLM attends to the ones it needs."),
          ("Saved every turn.", "The session snapshot is written back to memory, so a session survives restarts and can move between GPUs."),
        ],
        "outcome": "Each turn processes only its new tokens. Long sessions stop getting slower and more expensive.",
        "cap": "Each turn restores the session, attends to memory, and saves the session back. Only the new input is processed.",
        "dg": {
          "w": 760, "h": 270,
          "nodes": [
            ("in", 20, 40, 130, 54, "Turn n", ["new tokens only"], "n"),
            ("lm", 220, 30, 200, 74, "AgentLM", ["recurrent layers + window", "memory attention"], "lm"),
            ("out", 500, 40, 130, 54, "Answer", ["turn n"], "ok"),
            ("ss", 120, 180, 220, 64, "Session snapshot", ["fixed size · any length"], "mem"),
            ("kb", 400, 180, 220, 64, "Attention blocks", ["from the knowledge graph"], "mem"),
          ],
          "edges": [
            E("in:r", "lm:l", "input"),
            E("lm:r", "out:l", k="ok"),
            E("ss:t@200", "lm:b@260", "restore", k="mem"),
            E("lm:b@300", "ss:t@300", "save", k="mem", lp=(300, 140)),
            E("kb:t", "lm:b@380", "attend", k="mem"),
          ],
          "texts": [(120, 262, "AGENT MEMORY", "start")],
        },
      },
      {
        "id": "a3", "role": "gov",
        "resp": "what data leaves the company.",
        "pain": "send prompts full of customer and internal data to an external model on every call.",
        "lead": "AgentLM runs on a GPU <b>inside your tenant</b>, so most requests are answered without any data leaving it. Escalation to a frontier LLM is an action like any other: <b>Cedar decides</b> whether it may happen and what may be sent, and every escalation is recorded with what was sent and why.",
        "steps": [
          ("Local by default.", "AgentLM, agent memory and the session state all live in your tenant."),
          ("Escalation is an action.", "The llm:escalate action is checked like a tool call: by agent, by user, by data classification."),
          ("Forbid what must stay.", "A forbid policy can keep restricted data local whatever the model decides; AgentLM then answers or hands over to a person."),
          ("Recorded.", "Each escalation is stored as a decision record next to the run: who, what was sent, which policy allowed it."),
        ],
        "outcome": "You decide what may leave, per agent and per data class, and you can prove what did.",
        "cap": "Escalation passes Cedar before it leaves the tenant. Restricted data is stopped at the boundary.",
        "dg": {
          "w": 760, "h": 270,
          "groups": [("t", 10, 14, 520, 242, "YOUR TENANT", "pol")],
          "nodes": [
            ("lm", 30, 60, 160, 60, "AgentLM", ["wants to escalate"], "lm"),
            ("pol", 240, 60, 170, 60, "Cedar", ["llm:escalate?"], "pol"),
            ("rec", 240, 176, 170, 54, "Decision record", ["who · what · why"], "mem"),
            ("code", 20, 150, 200, 92, "", ["forbid (principal,", " action == \"llm:escalate\",", " resource)", "when { context.class", "  == \"restricted\" };"], "code"),
            ("fr", 590, 40, 160, 54, "Frontier LLM", ["allowed data only"], "llm"),
            ("dn", 590, 130, 160, 54, "Stays local", ["answer or hand over"], "deny"),
          ],
          "edges": [
            E("lm:r", "pol:l", "request", k="lm"),
            E("pol:r@75", "fr:l", "allow", k="pol"),
            E("pol:r@105", "dn:l", "forbid", k="deny"),
            E("pol:b", "rec:t", "log", k="mem"),
          ],
        },
      },
      {
        "id": "a4", "role": "platform",
        "resp": "running models at predictable cost and latency.",
        "pain": "depend on a frontier API whose latency, rate limits and price I don’t control, and fine-tuning a model per team is out of reach.",
        "lead": "AgentLM is a <b>4–8B-parameter model that runs on a single GPU</b>, served by vLLM. Session memory per user is fixed in size, so capacity planning is arithmetic. New domains come as <b>small skill adapters</b> stored in agent memory and paged onto the GPU when the gate asks for them, so teams don’t need their own fine-tuned model.",
        "steps": [
          ("One GPU per model.", "No cluster of accelerators per tenant. Start with one GPU and add more as traffic grows."),
          ("Fixed per-session memory.", "Tens of megabytes per session whatever its length, so a GPU’s capacity is a known number."),
          ("Skills paged in (roadmap).", "Domain and tenant skills are adapters trained against one frozen core, cached on the GPU and evicted when cold."),
          ("Versioned core.", "The core model changes rarely and deliberately. On an upgrade, memory’s attention blocks are recomputed from the source data as a background job."),
        ],
        "outcome": "Latency and cost you can plan, on hardware you control, without a model per team.",
        "cap": "One core model on one GPU. Skills are paged in from agent memory; sessions are fixed-size.",
        "dg": {
          "w": 760, "h": 270,
          "groups": [("g", 10, 14, 430, 242, "ONE GPU · vLLM", "lm")],
          "nodes": [
            ("core", 30, 50, 190, 64, "AgentLM core", ["frozen · versioned"], "lm"),
            ("cache", 240, 50, 180, 64, "Skill cache", ["adapters · LRU"], "lm"),
            ("sess", 30, 160, 390, 64, "Sessions", ["fixed size each · planned capacity"], "n"),
            ("lib", 520, 50, 230, 64, "Skill library", ["in agent memory"], "mem"),
            ("snap", 520, 160, 230, 64, "Session snapshots", ["in agent memory"], "mem"),
          ],
          "edges": [
            E("lib:l", "cache:r", "page in", k="mem"),
            E("core:r", "cache:l", k="lm"),
            E("sess:r", "snap:l", "save / restore", k="mem"),
          ],
        },
      },
      {
        "id": "a5", "role": "builder",
        "resp": "giving agents the knowledge they need.",
        "pain": "depend on retrieval plumbing — chunking, vector search, stuffing text into the prompt — that misses facts or crowds out the question.",
        "lead": "AgentLM doesn’t need knowledge pasted into its prompt. The knowledge graph in agent memory is precomputed into <b>attention blocks</b> that the model reads directly through dedicated memory-attention layers. The prompt stays short, and the knowledge is part of the same pass.",
        "steps": [
          ("Precomputed once.", "Knowledge-graph nodes — cases, procedures, concepts — are turned into attention blocks for this model and stored next to the source records in agent memory."),
          ("Selected by the gate.", "The gate picks the part of the graph that matters for this request, so only relevant blocks are loaded."),
          ("Attended, not pasted.", "Memory-attention layers read those blocks directly. The prompt stays short and the question keeps its place."),
          ("Source stays the truth.", "Records and the graph remain the source of truth; blocks are recomputed when the data or the model changes."),
        ],
        "outcome": "No chunking pipeline to tune and no context window to ration. The model works from the same knowledge your agents store.",
        "cap": "Memory is read through attention, not copied into the prompt.",
        "dg": {
          "w": 760, "h": 280,
          "nodes": [
            ("q", 20, 40, 130, 54, "Request", ["short prompt"], "n"),
            ("lm", 220, 30, 240, 74, "AgentLM", ["self-attention on the request", "memory attention on blocks"], "lm"),
            ("out", 560, 40, 180, 54, "Answer", ["grounded in memory"], "ok"),
            ("kg", 20, 196, 180, 60, "Knowledge graph", ["records · links"], "mem"),
            ("bl", 260, 196, 200, 60, "Attention blocks", ["precomputed per model"], "mem"),
            ("ps", 560, 196, 180, 60, "Pasted context", ["not needed"], "ghost"),
          ],
          "edges": [
            E("q:r", "lm:l@67", "prompt"),
            E("lm:r@67", "out:l", k="ok"),
            E("kg:r", "bl:l", "precompute", k="mem"),
            E("bl:t", "lm:b@360", "attend", k="mem"),
          ],
        },
      },
      {
        "id": "a6", "role": "builder",
        "resp": "turning model output into actions.",
        "pain": "return free text or half-valid tool calls that I have to parse, validate and retry.",
        "lead": "AgentLM answers with a <b>workflow</b>, not prose. Decoding is constrained by the CNCF Serverless Workflow schema, so every token keeps the output valid, and A3T dry-runs the workflow before returning it. What you receive is something you can inspect, approve and run.",
        "steps": [
          ("Schema-constrained decoding.", "The model can only emit tokens that keep the output a valid workflow. No parsing, no repair loop."),
          ("Dry run before return.", "A3T checks the workflow against its inputs, tools and policies without side effects."),
          ("Reviewable.", "The result is YAML you can read on the canvas and diff in Git, like any other workflow."),
          ("Same runtime.", "Once approved, it runs with checkpoints, policy checks and audit, like a hand-written flow."),
        ],
        "outcome": "Agent output stops being a string to interpret and becomes a checked plan you can run.",
        "cap": "Output is a valid workflow by construction, then dry-run before anything executes.",
        "dg": {
          "w": 760, "h": 250,
          "nodes": [
            ("lm", 20, 50, 150, 64, "AgentLM", ["decoding"], "lm"),
            ("sch", 220, 50, 180, 64, "Schema constraint", ["CNCF SWF grammar"], "wf"),
            ("wfy", 450, 50, 150, 64, "Workflow", ["valid YAML"], "wf"),
            ("free", 220, 170, 180, 54, "Free text", ["not possible"], "ghost"),
            ("dry", 450, 170, 150, 54, "Dry run", ["no side effects"], "pol"),
            ("run", 640, 170, 100, 54, "Run", ["approved"], "ok"),
          ],
          "edges": [
            E("lm:r", "sch:l", "tokens", k="lm"),
            E("sch:r", "wfy:l", k="wf"),
            E("wfy:b", "dry:t", k="pol"),
            E("dry:r", "run:l", "pass", k="ok"),
          ],
        },
      },
      {
        "id": "a7", "role": "builder",
        "resp": "answers people can trust.",
        "pain": "guess confidently when they don’t know, and nothing tells me it was a guess.",
        "lead": "AgentLM doesn’t answer from nowhere. Before it commits, the gate checks whether agent memory holds a matching procedure, evidence or concept. If nothing matches, it escalates, whatever the model itself thinks. It can also escalate mid-thought when a step needs knowledge it doesn’t have.",
        "steps": [
          ("Coverage check first.", "The gate looks for near-matching procedures, evidence and concepts in memory for this request."),
          ("Hard veto.", "No coverage means escalate. The model’s own confidence can’t override it."),
          ("Escalate mid-thought.", "While reasoning, AgentLM can call escalation as a tool when a step needs knowledge it doesn’t have."),
          ("A threshold you control.", "The escalation threshold starts high and comes down only as measured accuracy allows."),
        ],
        "outcome": "Answers are grounded in what your memory actually holds, and gaps show up as escalations you can see instead of confident guesses.",
        "cap": "No matching knowledge in memory means escalate, never guess.",
        "dg": {
          "w": 760, "h": 270,
          "nodes": [
            ("req", 20, 80, 120, 54, "Request", [], "n"),
            ("gate", 190, 70, 170, 74, "Gate", ["coverage check"], "lm"),
            ("mem", 190, 200, 170, 54, "Agent memory", ["procedures · evidence"], "mem"),
            ("yes", 470, 30, 190, 54, "Answer locally", ["grounded"], "ok"),
            ("no", 470, 150, 190, 54, "Escalate", ["no coverage"], "llm"),
          ],
          "edges": [
            E("req:r", "gate:l@107"),
            E("mem:t", "gate:b", "match?", k="mem"),
            E("gate:r@92", "yes:l", "covered", k="ok", lp=(415, 72)),
            E("gate:r@122", "no:l", "nothing matches", k="llm", dash=True, lp=(415, 152)),
          ],
        },
      },
      {
        "id": "a8", "role": "gov",
        "resp": "explaining what agents did.",
        "pain": "give answers I can’t trace. I don’t know what they knew, or why they asked an outside model.",
        "lead": "Every AgentLM answer is tied to the run that produced it. A3T records which memory the model read, whether the gate kept the request or escalated it and why, what was sent if it escalated, and the workflow that resulted. All of it sits in the execution graph next to the policy decisions.",
        "steps": [
          ("Memory read.", "The memory loaded for the request is recorded with the run."),
          ("Gate decision.", "Kept or escalated, with the coverage result that decided it."),
          ("Escalation record.", "What was sent, to which model, and which policy allowed it."),
          ("One graph.", "The answer, the workflow it became and every decision sit in the same execution graph, queryable with AQL."),
        ],
        "outcome": "For any run, you can answer “what did it know, and why did it do that?”",
        "cap": "Each run records the memory read, the gate decision, any escalation and the resulting workflow.",
        "dg": {
          "w": 760, "h": 280,
          "nodes": [
            ("run", 20, 108, 140, 60, "Run", ["one request"], "n"),
            ("m", 240, 20, 210, 48, "Memory read", ["what it knew"], "mem"),
            ("g", 240, 84, 210, 48, "Gate decision", ["kept or escalated"], "lm"),
            ("e", 240, 148, 210, 48, "Escalation", ["what · where · policy"], "llm"),
            ("w", 240, 212, 210, 48, "Workflow", ["dry-run result"], "wf"),
            ("graph", 540, 108, 200, 60, "Execution graph", ["queryable with AQL"], "mem"),
          ],
          "edges": [
            E("run:r", "m:l", k="n"), E("run:r", "g:l", k="n"), E("run:r", "e:l", k="n"), E("run:r", "w:l", k="n"),
            E("m:r", "graph:l", k="mem"), E("g:r", "graph:l", k="mem"), E("e:r", "graph:l", k="mem"), E("w:r", "graph:l", k="mem"),
          ],
        },
      },
      {
        "id": "a9", "role": "gov",
        "resp": "who can see which data.",
        "pain": "could leak data through the model. It may have seen records the person asking isn’t allowed to see.",
        "lead": "AgentLM only reads memory the request is allowed to see. Before any attention blocks are loaded, Cedar checks the agent, and the user it acts for, against each memory scope. Blocks from scopes they can’t read are never loaded, so the model can’t draw on them.",
        "steps": [
          ("Scopes on memory.", "Memory is organised by tenant, team and agent namespace, each covered by policy."),
          ("Checked before loading.", "Cedar decides which scopes this agent, acting for this user, may read — before any block reaches the GPU."),
          ("Nothing to leak.", "Blocks from denied scopes are never loaded, so they can’t influence the answer."),
          ("Logged.", "Every allow and deny for memory access is in the decision log."),
        ],
        "outcome": "The model never knows more than the person it’s working for.",
        "cap": "Cedar filters memory scopes before anything is loaded for AgentLM.",
        "dg": {
          "w": 760, "h": 250,
          "nodes": [
            ("u", 20, 90, 140, 60, "Agent for Maya", ["SOC team"], "n"),
            ("pol", 200, 90, 150, 60, "Cedar", ["which scopes?"], "pol"),
            ("s1", 420, 20, 170, 46, "SecOps scope", [], "mem"),
            ("s2", 420, 97, 170, 46, "Shared scope", [], "mem"),
            ("s3", 420, 174, 170, 46, "Finance scope", ["never loaded"], "deny"),
            ("lm", 640, 60, 110, 66, "AgentLM", ["reads allowed"], "lm"),
          ],
          "edges": [
            E("u:r", "pol:l", k="n"),
            E("pol:r@105", "s1:l", "allow", k="pol"),
            E("pol:r@120", "s2:l", k="pol"),
            E("pol:r@135", "s3:l", "deny", k="deny"),
            E("s1:r", "lm:l@82", k="mem"),
            E("s2:r", "lm:l@104", k="mem"),
          ],
        },
      },
      {
        "id": "a10", "role": "gov",
        "resp": "what agents are allowed to change.",
        "pain": "act straight from model output. A model decision becomes an action before anyone or anything checks it.",
        "lead": "AgentLM’s output is a workflow, and a workflow does nothing until it passes A3T’s checks. It is dry-run first, then every step is checked by Cedar as it runs, and steps you mark as sensitive wait for a human approval.",
        "steps": [
          ("A plan, not an action.", "The model produces a workflow. Nothing touches your systems at this point."),
          ("Dry run.", "Inputs, tools and policies are checked without side effects."),
          ("Policy on every step.", "Each step is authorized by Cedar as it runs, for this agent and the user it acts for."),
          ("Human gates.", "Sensitive steps pause for approval, and every decision is logged."),
        ],
        "outcome": "Model decisions go through the same controls as any other change.",
        "cap": "Between the model and your systems: a dry run, Cedar on every step, and human approval where you require it.",
        "dg": {
          "w": 760, "h": 250,
          "nodes": [
            ("lm", 20, 60, 120, 60, "AgentLM", ["proposes"], "lm"),
            ("wf", 175, 60, 120, 60, "Workflow", ["plan only"], "wf"),
            ("dry", 330, 60, 110, 60, "Dry run", [], "pol"),
            ("ced", 475, 60, 120, 60, "Cedar", ["each step"], "pol"),
            ("sys", 640, 60, 110, 60, "Your systems", [], "ok"),
            ("hum", 475, 176, 120, 54, "Approval", ["sensitive steps"], "n"),
          ],
          "edges": [
            E("lm:r", "wf:l", k="lm"), E("wf:r", "dry:l", k="wf"), E("dry:r", "ced:l", k="pol"),
            E("ced:r", "sys:l", k="ok"),
            E("ced:b", "hum:t", "pause", k="pol"),
            E("hum:r", "sys:b", "approved", k="ok"),
          ],
        },
      },
      {
        "id": "a11", "role": "platform",
        "resp": "GPU capacity.",
        "pain": "use more GPU memory the longer a conversation runs, so long sessions and many users don’t fit.",
        "lead": "Most of AgentLM’s layers are <b>recurrent</b>: they keep a fixed-size state instead of a cache that grows with every token. A few attention layers keep a short sliding window for exact recall of recent text. Per session, memory stays at tens of megabytes however long it runs.",
        "steps": [
          ("Fixed-size state.", "Recurrent layers carry the session forward in constant memory."),
          ("A short exact window.", "Attention layers keep only the last few thousand tokens verbatim."),
          ("Older knowledge lives in memory.", "Anything outside the window is read from agent memory through memory attention, not held in GPU cache."),
          ("Capacity is arithmetic.", "Sessions per GPU is a number you can plan, not one that shrinks as conversations grow."),
        ],
        "outcome": "Long sessions and many concurrent users fit on the GPUs you have.",
        "cap": "A typical model’s cache grows with every token. AgentLM’s session state stays the same size.",
        "dg": {
          "w": 760, "h": 210,
          "groups": [("a", 10, 14, 360, 182, "GROWING CACHE · TYPICAL MODEL", "deny"), ("b", 390, 14, 360, 182, "FIXED STATE · AGENTLM", "lm")],
          "nodes": [
            ("t1", 30, 44, 70, 34, "turn 1", [], "ghost"),
            ("t50", 30, 92, 160, 34, "turn 50", [], "ghost"),
            ("t500", 30, 140, 320, 34, "turn 500 · out of memory", [], "deny"),
            ("s1", 410, 44, 120, 34, "turn 1", [], "lm"),
            ("s50", 410, 92, 120, 34, "turn 50", [], "lm"),
            ("s500", 410, 140, 120, 34, "turn 500", [], "lm"),
          ],
          "texts": [(550, 65, "tens of MB", "start"), (550, 113, "tens of MB", "start"), (550, 161, "tens of MB", "start")],
        },
      },
      {
        "id": "a12", "role": "platform",
        "resp": "keeping agent sessions running through restarts and scaling.",
        "pain": "lose their session when a GPU node restarts, and I can’t move a session to another node.",
        "lead": "After every turn, AgentLM’s session — its fixed-size state, the recent-text window and pointers to the memory it used — is saved to agent memory as a snapshot. The next turn restores it on whichever GPU picks it up.",
        "steps": [
          ("Snapshot every turn.", "The session is written to agent memory after each answer."),
          ("Restore anywhere.", "Any GPU serving the same model version can load it and continue."),
          ("Survives restarts.", "A node restart or rescheduling costs one restore, not the conversation."),
          ("Scale freely.", "Sessions aren’t pinned, so you can add, drain or replace GPUs without stranding users."),
        ],
        "outcome": "GPU nodes become replaceable. Sessions live in memory, not on a machine.",
        "cap": "Sessions are saved to agent memory after each turn and restored on any GPU.",
        "dg": {
          "w": 760, "h": 260,
          "nodes": [
            ("g1", 20, 40, 180, 64, "GPU node A", ["turn 41"], "lm"),
            ("down", 290, 50, 180, 44, "Node A restarts", [], "deny"),
            ("g2", 560, 40, 180, 64, "GPU node B", ["turn 42"], "lm"),
            ("mem", 250, 176, 260, 64, "Agent memory", ["session snapshots"], "mem"),
          ],
          "edges": [
            E("g1:r", "down:l", k="deny", dash=True),
            E("g1:b", "mem:l", "save", k="mem"),
            E("mem:r", "g2:b", "restore", k="mem"),
          ],
        },
      },
    ],
  },
  {
    "id": "workflows", "name": "Workflows", "cat": "Workflow",
    "items": [
      {
        "id": "w1", "role": "builder",
        "resp": "shipping agents that do real work, not just chat.",
        "pain": "call the LLM for steps that should be plain code, so every run is slow, costly and a little different.",
        "lead": "An A3T agent is three parts working together: a <b>workflow</b>, <b>AgentLM</b> and <b>memory</b>. The workflow carries the known path with typed inputs and deterministic tasks, and its execution is monitored step by step. Only deviations from that path go to AgentLM, which uses the flow execution graph in agent memory to resolve the case and recommend how the workflow should change. If AgentLM can’t resolve it, it escalates to a frontier LLM, and only if policy allows.",
        "steps": [
          ("Typed inputs.", "Every workflow declares its input schema. Unexpected input is caught at the edge, not discovered by a model halfway through a run."),
          ("Deterministic by default.", "Switches, loops, transforms, MCP tool calls and triggers run as plain tasks on the Rust runtime — no tokens spent, the same result every time."),
          ("Execution is monitored.", "Each step is compared with the expected path. When a run leaves it — an unknown branch, a schema mismatch, an outcome out of range — only that deviation is handed to AgentLM."),
          ("AgentLM manages the flow.", "AgentLM reads the flow execution graph in agent memory — past runs, the paths they took and how they ended — to handle the case, then recommends a workflow change. Approved, it becomes the next version."),
          ("Escalate only when stuck.", "A case AgentLM can’t solve goes to a frontier LLM with the context it needs. Cedar decides whether it may leave your tenant, and the call is recorded."),
        ],
        "outcome": "Model calls shrink to the cases that need judgement, frontier calls shrink further still, and the workflow improves from its own history instead of re-reasoning every run.",
        "cap": "The known path runs as code. Deviations go to AgentLM, which reads the execution graph and proposes the next workflow version. A frontier LLM is the last resort.",
        "dg": {
          "w": 760, "h": 340,
          "groups": [("agent", 150, 14, 480, 312, "A3T AGENT = WORKFLOW + AGENTLM + MEMORY", "n")],
          "nodes": [
            ("in", 10, 60, 120, 54, "Typed input", ["schema-checked"], "n"),
            ("wf", 170, 44, 440, 86, "Workflow (CNCF Serverless Workflow)", ["validate → switch → MCP tool → transform → emit", "deterministic · typed · µs dispatch"], "wf"),
            ("out", 650, 28, 100, 50, "Result", ["known path"], "ok"),
            ("mon", 170, 162, 170, 50, "Execution monitor", ["path vs. expected"], "n"),
            ("llm", 420, 162, 190, 50, "AgentLM", ["deviations only"], "lm"),
            ("mem", 170, 250, 440, 58, "Agent memory", ["flow execution graph · past runs, paths, outcomes"], "mem"),
            ("chg", 650, 162, 100, 50, "Change", ["proposal"], "lm"),
            ("fr", 650, 262, 100, 50, "Frontier LLM", ["if stuck"], "llm"),
          ],
          "edges": [
            E("in:r", "wf:l@87", "typed"),
            E("wf:r@53", "out:l", k="ok"),
            E("wf:b@255", "mon:t", "every step"),
            E("mon:r", "llm:l", "deviation", k="lm"),
            E("llm:t", "wf:b@515", "resolve", k="lm"),
            E("mon:b", "mem:t@255", "record run", k="mem"),
            E("mem:t@515", "llm:b", "graph context", k="mem"),
            E("llm:r", "chg:l", k="lm"),
            E("llm:b@590", "fr:l", k="llm", dash=True),
            E("chg:t", "wf:r@112", "approved → v+1", k="pol", via=[(700, 112)], lp=(690, 136)),
          ],
        },
      },
      {
        "id": "w2", "role": "builder",
        "resp": "agents that finish long, multi-step jobs.",
        "pain": "fail halfway through, and I have to rerun everything from the start — including steps that already had side effects.",
        "lead": "A3T checkpoints workflow state after every task. When a step fails, the run resumes from that step with its state intact; steps that already finished are not repeated.",
        "steps": [
          ("Checkpoint after each task.", "Inputs, outputs and the run’s position in the flow are saved as it progresses."),
          ("Per-task failure policy.", "Each task carries its own retries, backoff and timeout, so one slow dependency doesn’t sink the whole run."),
          ("Resume, don’t restart.", "After a failure or a restart, the run picks up at the failed step. A ticket that was already opened, or a message already sent, stays done."),
        ],
        "outcome": "Long jobs survive failures and restarts without duplicate side effects or wasted compute.",
        "cap": "Steps 1–2 are checkpointed. Step 3 timed out and resumes from saved state.",
        "dg": {
          "w": 760, "h": 250,
          "nodes": [
            ("t1", 20, 40, 120, 54, "1 · fetch logs", ["done"], "ok"),
            ("t2", 170, 40, 120, 54, "2 · enrich", ["done"], "ok"),
            ("t3", 320, 40, 120, 54, "3 · call scanner", ["timed out"], "deny"),
            ("t4", 470, 40, 120, 54, "4 · open ticket", ["waiting"], "ghost"),
            ("t5", 620, 40, 120, 54, "5 · notify", ["waiting"], "ghost"),
            ("cp", 20, 170, 420, 54, "Checkpoint store", ["state saved after every task"], "mem"),
            ("pol", 500, 170, 240, 54, "Per-task policy", ["retries · backoff · timeout"], "n"),
          ],
          "edges": [
            E("t1:r", "t2:l", k="ok"), E("t2:r", "t3:l", k="ok"),
            E("t3:r", "t4:l", dash=True), E("t4:r", "t5:l", dash=True),
            E("t1:b", "cp:t@80", "save", k="mem"),
            E("t2:b", "cp:t@230", "save", k="mem"),
            E("cp:r", "t3:b", "resume at step 3", k="wf", lp=(380, 132)),
          ],
        },
      },
      {
        "id": "w3", "role": "platform",
        "resp": "the runtime agents depend on in production.",
        "pain": "stall under load, and latency jumps whenever the runtime pauses — not acceptable for bidding, trading or live monitoring.",
        "lead": "A3T’s core is written in Rust. There is no garbage collector to pause execution, and tasks are dispatched in microseconds, so latency stays predictable as load grows.",
        "steps": [
          ("Any trigger in.", "Webhooks, schedules and events all feed the same dispatcher."),
          ("Microsecond dispatch, no GC pauses.", "No stop-the-world pauses, and async I/O keeps workers busy instead of blocked."),
          ("LLM off the hot path.", "Deterministic tasks and tool calls run directly. The LLM is called only on deviations (see the first question), so a slow model doesn’t set your latency."),
        ],
        "outcome": "Tight latency budgets — like an RTB bid window — become something you can plan for, not hope for.",
        "cap": "One dispatcher feeds tasks, tools and LLM calls. The hot path stays deterministic.",
        "dg": {
          "w": 760, "h": 240,
          "nodes": [
            ("tw", 10, 30, 130, 44, "Webhook", [], "n"),
            ("ts", 10, 98, 130, 44, "Schedule", [], "n"),
            ("te", 10, 166, 130, 44, "Event", [], "n"),
            ("d", 200, 60, 190, 120, "Rust dispatcher", ["µs task dispatch", "no GC pauses", "async I/O"], "wf"),
            ("x1", 450, 30, 160, 44, "Deterministic tasks", [], "wf"),
            ("x2", 450, 98, 160, 44, "MCP tools", [], "n"),
            ("x3", 450, 166, 160, 44, "LLM", ["deviations only"], "llm"),
            ("lat", 650, 60, 100, 120, "Predictable", ["latency", "no pause", "spikes"], "ok"),
          ],
          "edges": [
            E("tw:r", "d:l@85"), E("ts:r", "d:l@120"), E("te:r", "d:l@155"),
            E("d:r@85", "x1:l", k="wf"), E("d:r@120", "x2:l", k="wf"), E("d:r@155", "x3:l", k="llm", dash=True),
            E("x1:r", "lat:l@85", k="ok"), E("x2:r", "lat:l@120", k="ok"), E("x3:r", "lat:l@155", k="ok", dash=True),
          ],
        },
      },
      {
        "id": "w4", "role": "platform",
        "resp": "supporting many teams that build agents.",
        "pain": "are built in a different framework by every team, and I have to deploy, secure and debug all of them.",
        "lead": "Every A3T agent is defined in the open CNCF Serverless Workflow spec and runs on one runtime. Tools, triggers, policy and memory are registered once and shared by every team.",
        "steps": [
          ("One definition format.", "Teams write workflows in CNCF Serverless Workflow — an open, portable spec, not a proprietary DSL."),
          ("One runtime to operate.", "The same deployment, scaling, logs and upgrade path for every agent."),
          ("Shared building blocks.", "MCP tools and triggers are registered once and reused. Policy and memory apply the same way to every team."),
        ],
        "outcome": "You run one platform instead of a framework zoo, and teams ship faster because the building blocks already exist.",
        "cap": "Many teams, one runtime, one set of shared tools and controls.",
        "dg": {
          "w": 760, "h": 260,
          "nodes": [
            ("a", 10, 20, 160, 54, "SecOps team", ["workflow.yaml"], "n"),
            ("b", 10, 103, 160, 54, "Trading team", ["workflow.yaml"], "n"),
            ("c", 10, 186, 160, 54, "Integration team", ["workflow.yaml"], "n"),
            ("rt", 250, 70, 230, 120, "One A3T runtime", ["CNCF Serverless Workflow", "same deploy · scale · logs"], "wf"),
            ("r1", 550, 20, 200, 54, "MCP tools", ["registered once, shared"], "n"),
            ("r2", 550, 103, 200, 54, "Triggers", ["schedule · webhook · event"], "n"),
            ("r3", 550, 186, 200, 54, "Policy + memory", ["same controls for every team"], "pol"),
          ],
          "edges": [
            E("a:r", "rt:l@95", "SWF"), E("b:r", "rt:l@130"), E("c:r", "rt:l@165"),
            E("rt:r@95", "r1:l", k="wf"), E("rt:r@130", "r2:l", k="wf"), E("rt:r@165", "r3:l", k="pol"),
          ],
        },
      },
      {
        "id": "w5", "role": "gov",
        "resp": "what automated systems do in the business.",
        "pain": "make decisions through a path nobody can reconstruct when something goes wrong.",
        "lead": "Every A3T run is a workflow, so every step is recorded: what came in, what memory was recalled, which policy decision was made, which tool ran and what came out. The run is stored as an execution graph in agent memory and can be queried later.",
        "steps": [
          ("Each step recorded.", "Inputs, outputs and policy decisions are captured per step — not just a final chat transcript."),
          ("Stored as a graph.", "Runs are linked to the incidents, entities and decisions they touched."),
          ("Ask why.", "Trace an outcome back through the steps and decisions that produced it, with an AQL query."),
        ],
        "outcome": "“Why did the agent do that?” gets a precise answer in minutes, not a forensic project.",
        "cap": "Every step of a run lands in the execution graph, ready to query.",
        "dg": {
          "w": 760, "h": 240,
          "nodes": [
            ("s1", 10, 30, 135, 62, "Trigger", ["alert #4471"], "n"),
            ("s2", 160, 30, 135, 62, "Recall", ["3 similar incidents"], "mem"),
            ("s3", 310, 30, 135, 62, "Policy check", ["allow · p-soc-12"], "pol"),
            ("s4", 460, 30, 135, 62, "Tool call", ["isolate host-17"], "wf"),
            ("s5", 610, 30, 140, 62, "Result", ["ticket SEC-88"], "ok"),
            ("g", 10, 156, 520, 62, "Execution graph in agent memory", ["inputs · outputs · decisions, per step"], "mem"),
            ("q", 570, 156, 180, 62, "You / auditor", ["RECALL … FOLLOW LINKS"], "n"),
          ],
          "edges": [
            E("s1:r", "s2:l"), E("s2:r", "s3:l"), E("s3:r", "s4:l"), E("s4:r", "s5:l"),
            E("s1:b", "g:t@77", k="mem", dash=True), E("s2:b", "g:t@227", k="mem", dash=True),
            E("s3:b", "g:t@377", k="mem", dash=True), E("s4:b@505", "g:t@505", k="mem", dash=True),
            E("q:l", "g:r", "ask why", k="mem"),
          ],
        },
      },
      {
        "id": "w6", "role": "gov",
        "resp": "change control on production systems.",
        "pain": "change behaviour the moment someone edits a prompt or a flow in production.",
        "lead": "In A3T, a workflow change is a new version, not a live edit. It is reviewed as a diff and promoted only after its owner approves it. Inside a flow, approval steps can pause a run before a sensitive action.",
        "steps": [
          ("Versioned definitions.", "Every change creates a new version; production keeps running the approved one."),
          ("Review the diff.", "Reviewers see exactly what changed — steps, tools, inputs — before it goes live."),
          ("Approval gate.", "The owner, or your CISO for sensitive agents, approves the promotion."),
          ("Human-in-the-loop steps.", "A flow can pause for a person to approve before a high-impact action."),
        ],
        "outcome": "Production agents change on purpose, with a record of who approved what.",
        "cap": "An edit becomes a draft version. Production changes only after approval.",
        "dg": {
          "w": 760, "h": 260,
          "nodes": [
            ("ed", 10, 30, 140, 54, "Someone edits", ["prompt or flow"], "n"),
            ("dr", 190, 30, 150, 54, "Draft v2", ["versioned, not live"], "wf"),
            ("rv", 380, 30, 150, 54, "Diff + review", ["what changed"], "n"),
            ("ap", 570, 30, 180, 54, "Approval gate", ["owner / CISO signs off"], "pol"),
            ("hl", 10, 166, 140, 70, "In-flow approval", ["pause before", "sensitive steps"], "pol"),
            ("pr", 190, 166, 360, 70, "Production runs v1", ["until v2 is approved"], "wf"),
          ],
          "edges": [
            E("ed:r", "dr:l"), E("dr:r", "rv:l"), E("rv:r", "ap:l", k="pol"),
            E("dr:b@265", "pr:t@265", "not live", k="deny", dash=True),
            E("ap:b", "pr:r", "promote v2", k="pol"),
          ],
        },
      },
    ],
  },
  {
    "id": "memory", "name": "Memory", "cat": "Memory",
    "items": [
      {
        "id": "m1", "role": "builder",
        "resp": "agents that people rely on day after day.",
        "pain": "forget everything between sessions or deployments, so users keep repeating themselves.",
        "lead": "A3T gives every agent durable memory across five types — working, episodic, semantic, procedural and tools. Facts survive restarts and redeploys, keep their history when they change, and are recalled by meaning rather than by scrolling back through recent messages.",
        "steps": [
          ("Write as you go.", "Agents STORE what they learn into the right memory type with AQL."),
          ("Survives everything.", "Memory is durable: sessions, restarts and redeploys don’t erase it."),
          ("Recall by meaning.", "RECALL finds what is relevant to the current task, not just the latest messages."),
        ],
        "outcome": "Better continuity, and agents stop asking for — or reconstructing — the same information.",
        "cap": "Sessions and deployments come and go. The agent’s memory stays.",
        "dg": {
          "w": 760, "h": 290,
          "groups": [("mg", 390, 14, 360, 262, "DURABLE AGENT MEMORY", "mem")],
          "nodes": [
            ("s1", 10, 30, 160, 54, "Session 1", ["user explains context"], "n"),
            ("rs", 10, 124, 160, 40, "Restart / redeploy", [], "ghost"),
            ("s2", 10, 204, 160, 54, "Session 2", ["agent already knows"], "ok"),
            ("aql", 220, 100, 120, 90, "AQL", ["STORE", "RECALL", "LINK"], "n"),
            ("m1", 405, 44, 330, 36, "Working — session state", [], "mem"),
            ("m2", 405, 88, 330, 36, "Episodic — what happened, when", [], "mem"),
            ("m3", 405, 132, 330, 36, "Semantic — facts, with history", [], "mem"),
            ("m4", 405, 176, 330, 36, "Procedural — how-to, with confidence", [], "mem"),
            ("m5", 405, 220, 330, 36, "Tools — what the agent can call", [], "mem"),
          ],
          "edges": [
            E("s1:b", "rs:t", dash=True), E("rs:b", "s2:t", dash=True),
            E("s1:r", "aql:l@120", "STORE", k="mem"),
            E("aql:l@170", "s2:r", "RECALL", k="mem"),
            E("aql:r", "mg:l", "by meaning", k="mem"),
          ],
        },
      },
      {
        "id": "m2", "role": "platform",
        "resp": "keeping the agent stack simple to run.",
        "pain": "need Postgres for facts, a vector store for recall, Neo4j for relationships, Redis or NATS for coordination, a change-notification mechanism and retention jobs — all stitched together by hand.",
        "lead": "A3T memory brings these functions together behind one durable data model and one query language, AQL. For typical agent workloads, that is one component to deploy, secure and understand instead of six plus the glue between them.",
        "steps": [
          ("Facts and history.", "STORE and UPDATE keep current values and how they changed."),
          ("Recall, relationships, coordination.", "RECALL by meaning, LINK and FOLLOW for graphs, topics and WATCH for coordination and change."),
          ("Retention built in.", "TTL and FORGET replace custom clean-up jobs."),
        ],
        "outcome": "Less custom engineering, fewer components to deploy, and one place to look when something breaks.",
        "cap": "Six systems and the glue code between them — or one memory layer and one query language.",
        "dg": {
          "w": 760, "h": 300,
          "groups": [
            ("left", 10, 14, 360, 272, "TODAY: STITCHED BY HAND", "deny"),
            ("right", 430, 14, 320, 272, "A3T: ONE MEMORY LAYER", "mem"),
          ],
          "nodes": [
            ("pg", 25, 46, 155, 50, "Postgres", ["facts + history"], "ghost"),
            ("vs", 200, 46, 155, 50, "Vector store", ["recall"], "ghost"),
            ("nj", 25, 124, 155, 50, "Neo4j", ["relationships"], "ghost"),
            ("rd", 200, 124, 155, 50, "Redis / NATS", ["coordination"], "ghost"),
            ("cd", 25, 202, 155, 50, "Change events", ["CDC / triggers"], "ghost"),
            ("cr", 200, 202, 155, 50, "Cron jobs", ["retention"], "ghost"),
            ("aql", 450, 44, 280, 40, "AQL — one query language", [], "wf"),
            ("f1", 450, 98, 280, 30, "STORE · UPDATE — facts + history", [], "mem"),
            ("f2", 450, 134, 280, 30, "RECALL — by meaning", [], "mem"),
            ("f3", 450, 170, 280, 30, "LINK · FOLLOW — relationships", [], "mem"),
            ("f4", 450, 206, 280, 30, "Topics · WATCH — coordination", [], "mem"),
            ("f5", 450, 242, 280, 30, "TTL · FORGET — retention", [], "mem"),
          ],
          "edges": [
            E("pg:r", "vs:l", k="deny", dash=True), E("nj:r", "rd:l", k="deny", dash=True), E("cd:r", "cr:l", k="deny", dash=True),
            E("pg:b", "nj:t", k="deny", dash=True), E("vs:b", "rd:t", k="deny", dash=True),
            E("nj:b", "cd:t", k="deny", dash=True), E("rd:b", "cr:t", k="deny", dash=True),
            E("left:r@150", "right:l@150", k="mem"),
          ],
          "texts": [(190, 278, "+ glue code between every pair", "middle")],
        },
      },
      {
        "id": "m3", "role": "builder",
        "resp": "multi-agent applications.",
        "pain": "need to share context with each other — but not everything, and not with every agent.",
        "lead": "A3T memory is organised by scope and namespace, so each agent or team has its own memory by default. Sharing is explicit: links connect records across scopes, and topics carry the messages agents need to exchange.",
        "steps": [
          ("Own namespace by default.", "Each agent or team writes into its own scope."),
          ("Share on purpose.", "LINK records into a shared scope, or publish to a topic other agents subscribe to."),
          ("Policy on top.", "Who can read a shared scope is a Cedar policy, not a convention (see the last Policy question)."),
        ],
        "outcome": "A common memory for multi-agent systems, without one undifferentiated conversation history that every agent can see.",
        "cap": "Agents read their own namespace. Only explicitly shared records cross over.",
        "dg": {
          "w": 760, "h": 290,
          "groups": [
            ("sec", 10, 106, 370, 120, "NAMESPACE: SECOPS", "mem"),
            ("trd", 560, 106, 190, 120, "NAMESPACE: TRADING", "mem"),
          ],
          "nodes": [
            ("a1", 25, 20, 160, 50, "Triage agent", ["SecOps"], "n"),
            ("a2", 205, 20, 160, 50, "Hunt agent", ["SecOps"], "n"),
            ("a3", 575, 20, 160, 50, "Pricing agent", ["Trading"], "n"),
            ("inc", 25, 142, 160, 64, "incidents", ["episodic"], "mem"),
            ("pb", 205, 142, 160, 64, "playbooks", ["procedural"], "mem"),
            ("sh", 410, 136, 120, 76, "Shared scope", ["explicit LINK", "or topic"], "pol"),
            ("pos", 575, 142, 160, 64, "positions", ["semantic"], "mem"),
          ],
          "edges": [
            E("a1:b", "inc:t", k="mem"), E("a2:b", "pb:t", k="mem"), E("a3:b", "pos:t", k="mem"),
            E("inc:r", "pb:l", k="mem", dash=True),
            E("pb:r", "sh:l", "LINK", k="pol"),
            E("pos:l", "sh:r", k="pol"),
          ],
          "texts": [(380, 262, "Default: an agent sees its own namespace. Sharing is explicit and policy-checked.", "middle")],
        },
      },
      {
        "id": "m4", "role": "gov",
        "resp": "data retention and the right to be forgotten.",
        "pain": "hold on to stale facts, and deleting data properly is hard.",
        "lead": "A3T updates facts with history instead of silently overwriting them, expires records with TTLs, and removes data physically with FORGET — not just by hiding it.",
        "steps": [
          ("Updates keep history.", "You can see what the agent believed, and when that changed."),
          ("TTL at write time.", "Records can carry an expiry from the moment they are stored."),
          ("Real deletion.", "FORGET removes data physically, for retention rules and erasure requests."),
        ],
        "outcome": "Control over stale facts, and a retention story you can explain to your DPO.",
        "cap": "Facts change with history. Expiry and FORGET remove data for real.",
        "dg": {
          "w": 760, "h": 230,
          "nodes": [
            ("v1", 10, 30, 150, 54, "Fact v1", ["supplier = Acme"], "mem"),
            ("v2", 220, 30, 150, 54, "Fact v2", ["supplier = Globex"], "mem"),
            ("hist", 10, 150, 360, 54, "History kept", ["who changed what, and when"], "ghost"),
            ("ttl", 420, 30, 150, 54, "TTL expiry", ["STORE … TTL 30d"], "n"),
            ("fg", 600, 30, 150, 54, "FORGET", ["physical removal"], "deny"),
            ("ret", 420, 150, 330, 54, "Retention you can show", ["expired and forgotten data is gone"], "pol"),
          ],
          "edges": [
            E("v1:r", "v2:l", "UPDATE", k="mem"),
            E("v1:b", "hist:t@85", k="mem", dash=True), E("v2:b", "hist:t@295", k="mem", dash=True),
            E("ttl:b", "ret:t@495", k="pol"), E("fg:b", "ret:t@675", k="pol"),
          ],
        },
      },
      {
        "id": "m5", "role": "builder",
        "resp": "agents that react to a changing world.",
        "pain": "can’t coordinate with each other, or notice when state changes, without custom queues and polling code.",
        "lead": "A3T memory includes ordered topics for agent-to-agent communication and watches that notify you when a query’s result changes.",
        "steps": [
          ("Ordered, restart-safe topics.", "Consumers keep their position, so an agent that restarts carries on from where it stopped."),
          ("WATCH a query.", "Subscribe to a RECALL and get notified when its result changes."),
          ("No glue services.", "Coordination lives in the same memory layer as the data it is about."),
        ],
        "outcome": "Less custom queue and event-processing code for reactive, multi-agent workflows.",
        "cap": "Topics carry events between agents. WATCH turns memory changes into notifications.",
        "dg": {
          "w": 760, "h": 240,
          "nodes": [
            ("a", 10, 30, 160, 54, "Detector agent", ["PUBLISH event"], "n"),
            ("tp", 230, 30, 260, 54, "Ordered topic", ["offset 1041 · 1042 · 1043"], "mem"),
            ("b", 550, 30, 200, 54, "Responder agent", ["resumes at last offset"], "ok"),
            ("w", 230, 160, 260, 54, "WATCH", ["RECALL … WHERE severity = high"], "mem"),
            ("n", 550, 160, 200, 54, "Notified on change", ["no polling"], "ok"),
          ],
          "edges": [
            E("a:r", "tp:l", "PUBLISH", k="mem"),
            E("tp:r", "b:l", "after restart", k="mem"),
            E("a:b", "w:l", "STORE", k="mem"),
            E("w:r", "n:l", "result changed", k="mem"),
          ],
        },
      },
      {
        "id": "m6", "role": "gov",
        "resp": "where company data goes.",
        "pain": "can’t send memory or embeddings to an external service.",
        "lead": "A3T can generate embeddings locally and be self-hosted, so recall works without calling an external embedding service. Memory, vectors and the model that makes them stay inside your boundary.",
        "steps": [
          ("Local embeddings.", "Text is embedded inside your environment."),
          ("Self-hosted memory.", "Records and vectors are stored where you run A3T."),
          ("No mandatory external calls.", "Recall doesn’t depend on a third-party API being available — or being allowed."),
          ("Local inference too.", "AgentLM runs in your tenant, so most requests are answered without any model call leaving it."),
        ],
        "outcome": "More control over data, fewer external dependencies, and more predictable cost.",
        "cap": "Embedding and recall happen inside your boundary.",
        "dg": {
          "w": 760, "h": 260,
          "groups": [("vpc", 10, 14, 530, 232, "YOUR BOUNDARY · VPC OR ON-PREM", "pol")],
          "nodes": [
            ("ag", 30, 56, 150, 54, "Agent", ["text to remember"], "n"),
            ("em", 230, 56, 170, 54, "Local embeddings", ["model runs in-cluster"], "wf"),
            ("mm", 230, 162, 290, 60, "Agent memory", ["records + vectors stay here"], "mem"),
            ("ex", 580, 56, 170, 54, "External embeddings", ["not required"], "ghost"),
          ],
          "edges": [
            E("ag:r", "em:l", "embed", k="wf"),
            E("em:b@315", "mm:t@315", "vectors", k="mem"),
            E("ag:b", "mm:l", "RECALL", k="mem"),
            E("em:r", "ex:l", "no call", k="deny", dash=True),
          ],
        },
      },
    ],
  },
  {
    "id": "policy", "name": "Policy", "cat": "Policy",
    "items": [
      {
        "id": "p1", "role": "gov",
        "resp": "access control across the company.",
        "pain": "run with the same broad access as whoever deployed them.",
        "lead": "Every A3T agent has its own service identity. Before each task, the Cedar policy engine built into the runtime checks who is acting, on whose behalf, which action and which resource. Anything not explicitly allowed is denied.",
        "steps": [
          ("Agent as a principal.", "Each agent is a named identity in your policies, separate from the person who built it."),
          ("On behalf of.", "When an agent acts for a user, that user’s groups are part of the decision."),
          ("Checked before it runs.", "Every task and tool call is evaluated first. Denied actions never execute, and are logged."),
        ],
        "outcome": "Least privilege for agents, written in policy you can read.",
        "cap": "Each action is evaluated against Cedar policy before it runs.",
        "dg": {
          "w": 760, "h": 300,
          "nodes": [
            ("ag", 10, 40, 170, 62, "SecOps agent", ["identity: svc-secops"], "n"),
            ("ob", 10, 176, 170, 62, "On behalf of", ["maya@acme.com · SOC"], "n"),
            ("ce", 250, 30, 230, 82, "Cedar engine", ["inside the runtime", "deny by default"], "pol"),
            ("al", 560, 22, 190, 50, "Task runs", ["isolate host-17"], "ok"),
            ("dn", 560, 90, 190, 50, "Blocked + logged", ["wipe host-17"], "deny"),
            ("pc", 250, 160, 500, 118, "", [
              "permit (",
              "  principal == Agent::\"svc-secops\",",
              "  action    == Action::\"isolate_host\",",
              "  resource  in HostGroup::\"prod\"",
              ") when { context.onBehalfOf in Group::\"soc\" };",
            ], "code"),
          ],
          "edges": [
            E("ag:r", "ce:l@71", "request", k="pol"),
            E("ob:t", "ag:b", "delegates", dash=True),
            E("ce:r@50", "al:l", "allow", k="ok"),
            E("ce:r@95", "dn:l", "deny", k="deny"),
            E("pc:t@365", "ce:b@365", "evaluated", k="pol", dash=True),
          ],
        },
      },
      {
        "id": "p2", "role": "gov",
        "resp": "joiners, movers and leavers.",
        "pain": "either break when the person who built them leaves, or keep acting with that person’s access.",
        "lead": "A3T agents run as service accounts owned by groups, not by individuals. Users and groups sync from your identity provider over SCIM, so when someone leaves, their access — and any delegation to agents — is removed, while production agents keep running.",
        "steps": [
          ("SCIM sync.", "Users and groups come from your IdP; there is no second user list to maintain."),
          ("Group-owned service accounts.", "Agents belong to a team, so ownership survives people changes."),
          ("Delegation follows the person.", "Acting on someone’s behalf ends when that person’s access ends."),
        ],
        "outcome": "No orphaned agents, no lingering access, and no outage when someone moves on.",
        "cap": "Leaving the IdP group revokes the person’s delegation. The agent keeps running as its service account.",
        "dg": {
          "w": 760, "h": 290,
          "groups": [("id", 220, 14, 300, 262, "A3T IDENTITY", "pol")],
          "nodes": [
            ("idp", 10, 114, 150, 62, "Your IdP", ["Entra · Okta · Keycloak"], "n"),
            ("grp", 240, 44, 260, 54, "Group: soc-analysts", ["maya removed"], "n"),
            ("svc", 240, 120, 260, 54, "Service account: svc-secops", ["owned by the group"], "pol"),
            ("dl", 240, 196, 260, 54, "Delegation from maya", ["revoked automatically"], "deny"),
            ("ag", 580, 120, 170, 54, "SecOps agent", ["keeps running"], "ok"),
          ],
          "edges": [
            E("idp:r", "id:l@145", "SCIM", k="pol"),
            E("svc:r", "ag:l", "runs as", k="ok"),
            E("dl:r", "ag:b", "ends", k="deny", dash=True),
          ],
        },
      },
      {
        "id": "p3", "role": "gov",
        "resp": "passing audits.",
        "pain": "can’t prove what they were allowed to do, or what they actually did.",
        "lead": "Every policy decision in A3T is recorded — principal, action, resource, result and the policy that matched. The policies themselves are Cedar: short, readable rules an auditor can check directly.",
        "steps": [
          ("Decision records.", "Every allow and deny is logged with its context."),
          ("Readable policy.", "Cedar rules state what an agent may do in plain terms."),
          ("Compare the two.", "“Allowed” versus “done” is a query, not an investigation."),
        ],
        "outcome": "Audit evidence comes from the system itself, not from screenshots and interviews.",
        "cap": "What an agent may do (policy) and what it did (decisions), side by side.",
        "dg": {
          "w": 760, "h": 200,
          "nodes": [
            ("ck", 10, 60, 150, 62, "Cedar decision", ["allow or deny"], "pol"),
            ("rec", 200, 18, 300, 120, "", [
              "time      09:14:02Z",
              "principal Agent::\"svc-secops\"",
              "action    isolate_host",
              "resource  host-17",
              "effect    ALLOW  · policy p-soc-12",
            ], "code"),
            ("pol", 580, 18, 170, 54, "Readable policy", ["Cedar, not code"], "pol"),
            ("au", 580, 110, 170, 54, "Auditor", ["allowed vs. done"], "n"),
          ],
          "edges": [
            E("ck:r", "rec:l@91", "record", k="pol"),
            E("rec:r@124", "au:l@124", "did"),
            E("pol:b", "au:t", "may do", k="pol"),
          ],
        },
      },
      {
        "id": "p4", "role": "builder",
        "resp": "getting agents approved and into production.",
        "pain": "have permission checks hard-coded in if-statements across the code, so every security review starts from scratch.",
        "lead": "In A3T, permissions are Cedar policy, outside your agent code. The runtime checks policy before every task and tool call, so your workflow just does the work — and security reviews read one set of policies, not your codebase.",
        "steps": [
          ("No auth code in agents.", "Tasks call tools; the runtime asks the policy engine first."),
          ("One place to review.", "Security reads the policies, not every branch of your code."),
          ("Change policy, not code.", "Updating a policy doesn’t need a new build of the agent."),
        ],
        "outcome": "Faster approvals, and fewer security bugs hiding in agent code.",
        "cap": "Permission logic moves out of agent code into policy the runtime enforces.",
        "dg": {
          "w": 760, "h": 270,
          "groups": [
            ("bf", 10, 14, 300, 244, "BEFORE", "deny"),
            ("af", 350, 14, 400, 244, "WITH A3T", "pol"),
          ],
          "nodes": [
            ("code", 25, 42, 270, 176, "", [
              "if user.role == \"admin\" {",
              "  if agent == \"secops\" {",
              "    if env != \"prod\" || ok {",
              "      call_tool(isolate)",
              "    }",
              "  }",
              "}",
              "// …repeated in every agent",
            ], "code"),
            ("tk", 370, 46, 160, 54, "Workflow task", ["call_tool(isolate)"], "wf"),
            ("ck", 570, 46, 160, 54, "Cedar check", ["before every task"], "pol"),
            ("st", 570, 160, 160, 70, "Policy store", ["reviewed in", "one place"], "pol"),
            ("rn", 370, 160, 160, 70, "Runs", ["only if allowed"], "ok"),
          ],
          "edges": [
            E("tk:r", "ck:l", "ask", k="pol"),
            E("st:t", "ck:b", "load", k="pol"),
            E("tk:b", "rn:t", "allowed", k="ok"),
          ],
        },
      },
      {
        "id": "p5", "role": "platform",
        "resp": "identity integration.",
        "pain": "each connect to our identity provider in their own way.",
        "lead": "A3T has pluggable OIDC for sign-in — Keycloak, Microsoft Entra ID, Okta or any standards-compliant provider — plus SCIM for users and groups. You integrate once, and every agent and team uses it.",
        "steps": [
          ("Bring your IdP.", "Any OIDC provider plugs in."),
          ("SCIM for users and groups.", "Group membership drives policy automatically."),
          ("One integration.", "New agents inherit identity; nobody wires up auth again."),
        ],
        "outcome": "One identity integration to own, instead of one per agent.",
        "cap": "One identity layer between your IdP and every agent.",
        "dg": {
          "w": 760, "h": 230,
          "nodes": [
            ("k", 10, 26, 150, 44, "Keycloak", [], "n"),
            ("e", 10, 93, 150, 44, "Microsoft Entra ID", [], "n"),
            ("o", 10, 160, 150, 44, "Okta", [], "n"),
            ("il", 250, 55, 230, 120, "A3T identity layer", ["OIDC sign-in", "SCIM users + groups", "one integration"], "pol"),
            ("a1", 580, 26, 170, 44, "SecOps agent", [], "n"),
            ("a2", 580, 93, 170, 44, "Trading agent", [], "n"),
            ("a3", 580, 160, 170, 44, "Monitoring agent", [], "n"),
          ],
          "edges": [
            E("k:r", "il:l@80", k="pol"), E("e:r", "il:l@115", "OIDC + SCIM", k="pol"), E("o:r", "il:l@150", k="pol"),
            E("il:r@80", "a1:l", k="pol"), E("il:r@115", "a2:l", k="pol"), E("il:r@150", "a3:l", k="pol"),
          ],
        },
      },
      {
        "id": "p6", "role": "platform",
        "resp": "isolating teams and tenants.",
        "pain": "could read memory that belongs to another team or customer.",
        "lead": "Memory reads and writes in A3T go through the same Cedar policy check as actions. Access is scoped by tenant, team and agent, and every read and write is logged.",
        "steps": [
          ("Policy on memory.", "RECALL and STORE are checked like any other action."),
          ("Scoped by default.", "An agent reaches only the scopes its policy allows."),
          ("Logged.", "Every memory access leaves a record."),
        ],
        "outcome": "Multi-tenant agents without cross-tenant leaks.",
        "cap": "Each memory query is checked against tenant and team scope before it reaches the data.",
        "dg": {
          "w": 760, "h": 270,
          "nodes": [
            ("ta", 10, 40, 160, 54, "Tenant A agent", ["RECALL …"], "n"),
            ("tb", 10, 170, 160, 54, "Tenant B agent", ["RECALL …"], "n"),
            ("pc", 220, 95, 170, 74, "Cedar check", ["tenant · team", "agent scope"], "pol"),
            ("ma", 520, 26, 230, 54, "Tenant A memory", ["B → A: denied"], "mem"),
            ("mb", 520, 105, 230, 54, "Tenant B memory", ["A → B: denied"], "mem"),
            ("lg", 500, 192, 250, 54, "Decision log", ["every read and write"], "n"),
          ],
          "edges": [
            E("ta:r", "pc:l@115", k="pol"), E("tb:r", "pc:l@150", k="pol"),
            E("pc:r@110", "ma:l", "A reads A", k="mem"),
            E("pc:r@135", "mb:l", "B reads B", k="mem", lp=(484, 132)),
            E("pc:b", "lg:l", "logged"),
          ],
        },
      },
    ],
  },
]


# ─────────────────────────────────────────────────────────────────────────────
# Page assembly
# ─────────────────────────────────────────────────────────────────────────────

# AgentLM questions read by role: builder, then governance, then platform.
AGENTLM_ORDER = ["a1", "a2", "a5", "a6", "a7", "a3", "a8", "a9", "a10", "a4", "a11", "a12"]
SECTIONS[0]["items"].sort(key=lambda it: AGENTLM_ORDER.index(it["id"]))

def strip_tags(s):
    return re.sub(r"<[^>]+>", "", s)


def item_html(it, num, sec):
    role = ROLES[it["role"]]
    steps = "\n".join(f'<li><b>{b}</b> {t}</li>' for b, t in it["steps"])
    dg = render(it["id"], it["dg"])
    return f'''
<details class="faq" id="{it["id"]}" data-role="{it["role"]}" data-cat="{sec["id"]}">
  <summary>
    <span class="faq-num">{num}</span>
    <span class="faq-q">
      <span class="tags"><button type="button" class="tag r-{it["role"]}" data-k="role" data-v="{it["role"]}" title="Show only {role} questions">{role}</button><button type="button" class="tag c-{sec["id"]}" data-k="cat" data-v="{sec["id"]}" title="Show only {sec["cat"]} questions">{sec["cat"]}</button></span>
      <span class="faq-resp"><span class="iam">I am responsible for</span> {it["resp"]}</span>
    </span>
    <span class="chev" aria-hidden="true"><svg width="14" height="14" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.8"><path d="M4 6l4 4 4-4"/></svg></span>
  </summary>
  <div class="faq-body">
    <div class="pain"><span class="lbl">My agents…</span><p>{it["pain"]}</p></div>
    <div class="help">
      <span class="lbl ok">How A3T helps</span>
      <p class="lead">{it["lead"]}</p>
      <figure class="arch-fig">
        <div class="dg-wrap">{dg}</div>
        <figcaption id="{it["id"]}-cap">{it["cap"]}</figcaption>
      </figure>
      <ol class="steps">{steps}</ol>
      <p class="outcome"><span>What changes</span>{it["outcome"]}</p>
    </div>
  </div>
</details>'''


def list_html():
    out = []
    for sec in SECTIONS:
        out += [item_html(it, f"{sec['name'][0]}{i + 1}", sec) for i, it in enumerate(sec["items"])]
    return "".join(out)


def faq_jsonld():
    qs = []
    for sec in SECTIONS:
        for it in sec["items"]:
            q = f"I am responsible for {strip_tags(it['resp'])} My agents {strip_tags(it['pain'])} How does A3T help?"
            a = strip_tags(it["lead"]) + " " + " ".join(f"{b} {t}" for b, t in it["steps"]) + " " + it["outcome"]
            qs.append({"@type": "Question", "name": q, "acceptedAnswer": {"@type": "Answer", "text": a}})
    return json.dumps({"@context": "https://schema.org", "@type": "FAQPage", "mainEntity": qs}, ensure_ascii=False, indent=1)


FAQ_CSS = r"""
  /* ═══ FAQ page ═══ */
  .faq-page .container { max-width: 1040px; }
  .faq-hero { padding: 64px 0 28px; }
  .faq-hero h1 { font-family: var(--font-serif); font-size: 36px; font-weight: 500; letter-spacing: -0.01em; line-height: 1.2; }
  .faq-sec { padding-bottom: 8px; }
  .faq-list { display: grid; grid-template-columns: minmax(0, 1fr); gap: 12px; }
  .faq, .faq-body > *, .faq-q { min-width: 0; }

  .faq { background: var(--bg-surface); border: 1px solid var(--border); border-radius: var(--r-lg); box-shadow: var(--shadow-sm); transition: border-color .15s, box-shadow .2s; scroll-margin-top: calc(var(--nav-h) + 16px); }
  .faq:hover { border-color: var(--border-strong); }
  .faq[open] { box-shadow: var(--shadow); border-color: var(--border-strong); }
  .faq summary { list-style: none; cursor: pointer; display: grid; grid-template-columns: 44px 1fr 32px; align-items: center; gap: 14px; padding: 20px 22px; }
  .faq summary::-webkit-details-marker { display: none; }
  .faq summary:focus-visible { outline: 2px solid var(--ephemeral); outline-offset: 2px; border-radius: var(--r-lg); }
  .faq-num { font-family: var(--font-mono); font-size: 12px; color: var(--text-4); font-weight: 500; }
  .faq-q { display: flex; flex-direction: column; gap: 8px; align-items: flex-start; }
  .faq-resp { font-family: var(--font-serif); font-size: 21px; line-height: 1.3; letter-spacing: -0.01em; color: var(--text-1); }
  .faq-resp .iam { color: var(--text-3); font-style: italic; }
  .tags { display: flex; gap: 6px; flex-wrap: wrap; }
  .tag { font-family: var(--font-mono); font-size: 10.5px; font-weight: 600; letter-spacing: .04em; text-transform: uppercase; padding: 3px 9px; border-radius: 999px; border: 1px solid transparent; cursor: pointer; transition: background .12s, color .12s, box-shadow .12s; }
  .tag:hover { box-shadow: 0 0 0 3px var(--bg-sunken); }
  .tag:focus-visible { outline: 2px solid var(--ephemeral); outline-offset: 2px; }
  .r-builder { background: var(--ephemeral-bg); color: var(--ephemeral-ink); }
  .r-gov { background: var(--ok-bg); color: var(--ok-ink); }
  .r-platform { background: var(--signal-bg); color: var(--signal-ink); }
  .c-agentlm, .c-workflows, .c-memory, .c-policy { background: var(--bg-surface); }
  .c-agentlm { color: var(--text-1); border-color: var(--text-1); }
  .c-workflows { color: var(--k8s-ink); border-color: var(--k8s); }
  .c-memory { color: var(--checkpoint-ink); border-color: var(--checkpoint); }
  .c-policy { color: var(--ok-ink); border-color: var(--ok); }
  /* Active filter: the pill fills with its ink colour. */
  .tag.on { color: #fff; }
  .r-builder.on { background: var(--ephemeral-ink); }
  .r-gov.on { background: var(--ok-ink); }
  .r-platform.on { background: var(--signal-ink); }
  .c-agentlm.on { background: var(--text-1); border-color: var(--text-1); }
  .c-workflows.on { background: var(--k8s-ink); border-color: var(--k8s-ink); }
  .c-memory.on { background: var(--checkpoint-ink); border-color: var(--checkpoint-ink); }
  .c-policy.on { background: var(--ok-ink); border-color: var(--ok-ink); }
  .faq.hidden { display: none; }

  .faq-filter { display: none; align-items: center; gap: 8px; flex-wrap: wrap; margin: -12px 0 18px; font-size: 13.5px; color: var(--text-3); }
  .faq-filter.show { display: flex; }
  .faq-filter .tag::after { content: '×'; margin-left: 6px; font-weight: 500; }
  .faq-filter .clear { border: 0; background: none; color: var(--text-2); font-size: 13.5px; text-decoration: underline; text-underline-offset: 3px; cursor: pointer; }
  .chev { width: 32px; height: 32px; border-radius: 50%; border: 1px solid var(--border); display: grid; place-items: center; color: var(--text-3); transition: transform .25s var(--ease), background .15s; }
  .faq[open] .chev { transform: rotate(180deg); background: var(--bg-subtle); }

  .faq-body { padding: 0 22px 26px 80px; display: grid; grid-template-columns: minmax(0, 1fr); gap: 22px; }
  .lbl { display: inline-block; font-family: var(--font-mono); font-size: 10.5px; font-weight: 600; text-transform: uppercase; letter-spacing: .1em; color: var(--deny); margin-bottom: 8px; }
  .lbl.ok { color: var(--ok-ink); }
  .pain { border-left: 2px solid var(--deny); padding: 2px 0 2px 16px; }
  .pain p { font-size: 17px; color: var(--text-1); line-height: 1.5; max-width: 760px; }
  .help .lead { font-size: 15.5px; color: var(--text-2); line-height: 1.65; max-width: 820px; }
  .help .lead b { color: var(--text-1); font-weight: 600; }

  .arch-fig { margin: 18px 0 18px; background: var(--bg-ivory); border: 1px solid var(--border); border-radius: var(--r-lg); padding: 18px 18px 14px; }
  .dg-wrap { overflow-x: auto; -webkit-overflow-scrolling: touch; }
  .arch-fig figcaption { margin-top: 10px; font-size: 12.5px; color: var(--text-3); font-style: italic; }
  svg.dg { display: block; width: 100%; min-width: 620px; height: auto; font-family: var(--font-sans); }
  svg.dg .grp { fill: none; stroke-width: 1.2; stroke-dasharray: 5 4; }
  svg.dg .g-n { stroke: var(--border-strong); fill: rgba(255,255,255,.5); }
  svg.dg .g-mem { stroke: var(--checkpoint); fill: rgba(251,240,224,.35); }
  svg.dg .g-pol { stroke: var(--ok); fill: rgba(227,244,239,.35); }
  svg.dg .g-deny { stroke: var(--deny); fill: rgba(251,233,231,.3); }
  svg.dg .glbl { font-family: var(--font-mono); font-size: 9.5px; font-weight: 600; letter-spacing: .1em; fill: var(--text-3); }
  svg.dg .gl-mem { fill: var(--checkpoint-ink); }
  svg.dg .gl-pol { fill: var(--ok-ink); }
  svg.dg .gl-deny { fill: var(--deny); }
  svg.dg .nd { stroke-width: 1.2; }
  svg.dg .k-n { fill: var(--bg-surface); stroke: var(--border-strong); }
  svg.dg .k-wf { fill: var(--k8s-bg); stroke: var(--k8s); }
  svg.dg .k-llm { fill: var(--signal-bg); stroke: var(--signal); }
  svg.dg .k-mem { fill: var(--checkpoint-bg); stroke: var(--checkpoint); }
  svg.dg .k-pol { fill: var(--ok-bg); stroke: var(--ok); }
  svg.dg .k-ok { fill: var(--ok-bg); stroke: var(--ok); }
  svg.dg .k-deny { fill: var(--deny-bg); stroke: var(--deny); }
  svg.dg .k-ghost { fill: var(--bg-subtle); stroke: var(--border-strong); stroke-dasharray: 4 3; }
  svg.dg .k-code { fill: #1d1c19; stroke: #1d1c19; }
  svg.dg .k-lm { fill: #1d1c19; stroke: #1d1c19; }
  svg.dg .g-lm { stroke: var(--text-3); fill: rgba(253,251,245,.6); } svg.dg .gl-lm { fill: var(--text-2); }
  svg.dg .nt { font-size: 12.5px; font-weight: 600; text-anchor: middle; fill: var(--text-1); }
  svg.dg .t-wf { fill: var(--k8s-ink); }
  svg.dg .t-llm { fill: var(--signal-ink); }
  svg.dg .t-mem { fill: var(--checkpoint-ink); }
  svg.dg .t-pol, svg.dg .t-ok { fill: var(--ok-ink); }
  svg.dg .t-deny { fill: var(--deny); }
  svg.dg .t-ghost { fill: var(--text-3); }
  svg.dg .ns { font-family: var(--font-mono); font-size: 10px; text-anchor: middle; fill: var(--text-3); }
  svg.dg text.t-lm { fill: #fff; } svg.dg text.s-lm { fill: #cfcabd; }
  svg.dg .code { font-family: var(--font-mono); font-size: 11px; fill: #e9e5da; white-space: pre; }
  svg.dg .edge { fill: none; stroke-width: 1.4; stroke-linejoin: round; }
  svg.dg .edge.dash { stroke-dasharray: 4 3; }
  svg.dg .e-n { stroke: var(--text-4); }   svg.dg .mk-n { fill: var(--text-4); }
  svg.dg .e-wf { stroke: var(--k8s); }     svg.dg .mk-wf { fill: var(--k8s); }
  svg.dg .e-llm { stroke: var(--signal); } svg.dg .mk-llm { fill: var(--signal); }
  svg.dg .e-mem { stroke: var(--checkpoint); } svg.dg .mk-mem { fill: var(--checkpoint); }
  svg.dg .e-pol, svg.dg .e-ok { stroke: var(--ok); } svg.dg .mk-pol, svg.dg .mk-ok { fill: var(--ok); }
  svg.dg .e-deny { stroke: var(--deny); }  svg.dg .mk-deny { fill: var(--deny); }
  svg.dg .e-lm { stroke: var(--text-1); }  svg.dg .mk-lm { fill: var(--text-1); }
  svg.dg .lb { fill: var(--bg-ivory); stroke: var(--border); }
  svg.dg .lt { font-family: var(--font-mono); font-size: 10px; text-anchor: middle; fill: var(--text-2); }
  svg.dg .lt-wf { fill: var(--k8s-ink); } svg.dg .lt-llm { fill: var(--signal-ink); } svg.dg .lt-mem { fill: var(--checkpoint-ink); }
  svg.dg .lt-pol, svg.dg .lt-ok { fill: var(--ok-ink); } svg.dg .lt-deny { fill: var(--deny); } svg.dg .lt-lm { fill: var(--text-1); font-weight: 600; }
  svg.dg .free { font-family: var(--font-mono); font-size: 10.5px; fill: var(--text-3); }

  .steps { list-style: none; counter-reset: st; display: grid; grid-template-columns: repeat(2, 1fr); gap: 12px 28px; max-width: 900px; }
  .steps li { counter-increment: st; position: relative; padding-left: 34px; font-size: 14px; color: var(--text-2); line-height: 1.55; }
  .steps li::before { content: counter(st, decimal-leading-zero); position: absolute; left: 0; top: 1px; font-family: var(--font-mono); font-size: 11px; font-weight: 600; color: var(--text-4); }
  .steps li b { color: var(--text-1); font-weight: 600; }
  .outcome { margin-top: 20px; display: flex; gap: 14px; align-items: baseline; padding: 14px 18px; background: var(--ok-bg); border-radius: var(--r); color: var(--ok-ink); font-size: 14.5px; font-weight: 500; max-width: 900px; }
  .outcome span { font-family: var(--font-mono); font-size: 10.5px; text-transform: uppercase; letter-spacing: .1em; white-space: nowrap; opacity: .8; }

  .faq-cta { padding: 40px 0 0; }
  .faq-cta p { font-size: 15.5px; color: var(--text-2); }
  .faq-cta a { color: var(--ephemeral-ink); font-weight: 500; text-decoration: underline; text-underline-offset: 3px; }

  @media (max-width: 640px) {
    .faq-hero { padding: 40px 0 20px; }
    .faq-hero h1 { font-size: 28px; }
    .faq summary { grid-template-columns: 1fr 32px; padding: 18px 16px; }
    .faq-num { display: none; }
    .faq-resp { font-size: 18.5px; }
    .faq-body { padding: 0 16px 22px; }
    .arch-fig { padding: 10px; }
    .steps { grid-template-columns: 1fr; }
    .outcome { flex-direction: column; gap: 4px; }
  }
"""

FAQ_JS = r"""
  /* ── FAQ: click a role or category pill to filter; click again to clear ── */
  (function () {
    const items = Array.from(document.querySelectorAll('.faq'));
    const bar = document.getElementById('faqFilter');
    const chips = document.getElementById('faqFilterTags');
    const count = document.getElementById('faqFilterCount');
    const state = { role: null, cat: null };

    function apply(anchor) {
      const before = anchor ? anchor.getBoundingClientRect().top : 0;
      let shown = 0;
      items.forEach(it => {
        const match = (!state.role || it.dataset.role === state.role) && (!state.cat || it.dataset.cat === state.cat);
        it.classList.toggle('hidden', !match);
        if (match) shown++;
      });
      document.querySelectorAll('.faq-list .tag').forEach(t => {
        const on = state[t.dataset.k] === t.dataset.v;
        t.classList.toggle('on', on);
        t.setAttribute('aria-pressed', String(on));
      });
      chips.innerHTML = '';
      ['role', 'cat'].forEach(k => {
        if (!state[k]) return;
        const src = document.querySelector(`.faq-list .tag[data-k="${k}"][data-v="${state[k]}"]`);
        const b = src.cloneNode(true);
        b.title = 'Remove filter';
        b.addEventListener('click', () => { state[k] = null; apply(); });
        chips.appendChild(b);
      });
      count.textContent = shown + (shown === 1 ? ' question' : ' questions');
      bar.classList.toggle('show', Boolean(state.role || state.cat));
      // Keep the clicked card where it was on screen.
      if (anchor) window.scrollBy(0, anchor.getBoundingClientRect().top - before);
    }

    document.querySelectorAll('.faq-list .tag').forEach(t => t.addEventListener('click', e => {
      e.preventDefault();
      e.stopPropagation();
      state[t.dataset.k] = state[t.dataset.k] === t.dataset.v ? null : t.dataset.v;
      apply(t.closest('.faq'));
    }));
    document.getElementById('faqFilterClear').addEventListener('click', () => { state.role = state.cat = null; apply(); });
  })();

  /* ── FAQ: deep links open their question ── */
  (function () {
    function openHash() {
      const el = location.hash && document.getElementById(location.hash.slice(1));
      if (el && el.classList.contains('faq')) {
        if (el.classList.contains('hidden')) document.getElementById('faqFilterClear').click();
        el.open = true;
        el.scrollIntoView({ block: 'start' });
      }
    }
    window.addEventListener('hashchange', openHash);
    openHash();
    document.querySelectorAll('.faq').forEach(it => it.addEventListener('toggle', () => {
      if (it.open) history.replaceState(null, '', '#' + it.id);
    }));
  })();

  window.addEventListener('scroll', () => nav.classList.toggle('scrolled', window.scrollY > 12), { passive: true });
"""


def between(text, start, end, include_end=True):
    i = text.index(start)
    j = text.index(end, i)
    return text[i:j + (len(end) if include_end else 0)]


ANCHORS = ["tour", "personas", "how-it-works", "agentlm", "workflows", "memory", "governance"]


def chrome(src, current):
    """Shared page parts lifted from index.html. `current` is the nav link to mark (faq.html)."""
    c = {}
    style = between(src, "<style>", "</style>")
    c["style"] = style.replace("</style>", FAQ_CSS + "</style>")
    c["consent_head"] = between(src, "<!-- OpenReplay", "</script>")
    top = between(src, '<div class="announce">', "</nav>")
    footer = between(src, '<footer class="site-footer">', "</footer>")
    for anchor in ANCHORS:
        top = top.replace(f'href="#{anchor}"', f'href="./#{anchor}"')
        footer = footer.replace(f'href="#{anchor}"', f'href="./#{anchor}"')
    top = top.replace('href="#" class="brand"', 'href="./" class="brand"')
    footer = footer.replace('href="#" class="brand"', 'href="./" class="brand"')
    top = top.replace(f'<a href="{current}" class="nav-link">', f'<a href="{current}" class="nav-link" aria-current="page">')
    c["top"], c["footer"] = top, footer
    c["after_footer"] = between(src, '<div class="cookie-banner"', '\n<script>', include_end=False)
    script = src[src.index("<script>\n  const nav"):]
    c["nav_js"] = between(script, "<script>", "/* ── Hero: rotating", include_end=False)
    c["tail_js"] = between(script, "/* ── Cookie consent ── */", "</script>", include_end=False)
    return c


def main():
    src = (HERE / "index.html").read_text()
    c = chrome(src, "faq.html")
    style, consent_head, chrome_top, footer = c["style"], c["consent_head"], c["top"], c["footer"]
    after_footer, nav_js, tail_js = c["after_footer"], c["nav_js"], c["tail_js"]

    count = sum(len(s["items"]) for s in SECTIONS)

    page = f'''<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="UTF-8" />
<meta name="viewport" content="width=device-width, initial-scale=1.0, viewport-fit=cover" />
<meta name="theme-color" content="#faf9f6" />
<title>A3T FAQ — AgentLM, Workflows, Agent Memory and Policy</title>
<meta name="description" content="How A3T solves the problems agent builders, governance and platform teams hit with AI agents: AgentLM in your tenant, deterministic workflows, durable agent memory and Cedar policy on every action — with architecture for each answer." />
<meta name="robots" content="index, follow, max-image-preview:large" />
<link rel="canonical" href="https://a3t.io/faq.html" />
<meta property="og:type" content="website" />
<meta property="og:url" content="https://a3t.io/faq.html" />
<meta property="og:site_name" content="A3T" />
<meta property="og:title" content="A3T FAQ — what it solves, for whoever you are" />
<meta property="og:description" content="Workflows, memory and policy for enterprise AI agents — the pain, the fix and the architecture, for builders, governance and platform teams." />
<meta property="og:image" content="https://a3t.io/assets/screens/16-chat-home.png" />
<meta name="twitter:card" content="summary_large_image" />
<meta name="twitter:title" content="A3T FAQ — Workflows, Agent Memory and Policy" />
<meta name="twitter:description" content="The pain, the fix and the architecture, for builders, governance and platform teams." />
<meta name="twitter:image" content="https://a3t.io/assets/screens/16-chat-home.png" />
<script type="application/ld+json">
{faq_jsonld()}
</script>
<link rel="icon" href="favicon.ico" />
<link rel="preconnect" href="https://fonts.googleapis.com">
<link rel="preconnect" href="https://fonts.gstatic.com" crossorigin>
<link href="https://fonts.googleapis.com/css2?family=Playfair+Display:ital,wght@0,400;0,500;0,600;0,700;1,400;1,500;1,600;1,700&family=IBM+Plex+Mono:wght@400;500;600&family=Inter:wght@400;450;500;550;600&display=swap" rel="stylesheet">
{consent_head}
{style}
</head>
<body>
<!-- Generated by build-faq.py — edit that file, not this one. -->
<a class="skip-link" href="#main">Skip to content</a>

{chrome_top}

<main id="main" class="faq-page">
<header class="faq-hero">
  <div class="container">
    <h1>Frequently asked questions</h1>
  </div>
</header>

<div class="container">
  <div class="faq-filter" id="faqFilter" aria-live="polite">
    <span>Showing</span><span class="tags" id="faqFilterTags"></span>
    <span id="faqFilterCount"></span>
    <button type="button" class="clear" id="faqFilterClear">Show all</button>
  </div>
</div>

<section class="faq-sec">
  <div class="container">
    <div class="faq-list">{list_html()}
    </div>
  </div>
</section>

<section class="faq-cta">
  <div class="container">
    <p>Didn’t find your question? <a href="{CALENDLY}" target="_blank" rel="noopener">Book a demo</a> and ask us directly.</p>
  </div>
</section>
</main>

{footer}

{after_footer}
{nav_js}{tail_js}{FAQ_JS}</script>
</body>
</html>
'''
    (HERE / "faq.html").write_text(page)
    print(f"Wrote faq.html ({count} questions, {len(page) // 1024} KB)")


if __name__ == "__main__":
    main()
