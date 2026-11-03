#!/usr/bin/env python3
"""Render the home page's "How it works" section into index.html.

Four folded parts: AgentLM, Workflows, Memory and Policy. The visible part of each is
plain-language with a simple architecture picture; the technical detail sits inside the
fold. Output goes between the HOW-IT-WORKS markers in index.html (HTML) and the
HOW-IT-WORKS-CSS markers (styles). Shares the diagram renderer with build-faq.py.
Edit this file, not the generated block. Run it before build-faq.py.
"""
import importlib.util
import re
import sys
from pathlib import Path

sys.dont_write_bytecode = True

HERE = Path(__file__).parent
_spec = importlib.util.spec_from_file_location("build_faq", HERE / "build-faq.py")
faq = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(faq)
_spec = importlib.util.spec_from_file_location("build_hub", HERE / "build-hub.py")
hub = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(hub)
E, render = faq.E, faq.render

FAQ_ITEMS = {it["id"]: it for sec in faq.SECTIONS for it in sec["items"]}

ARR = '<svg width="11" height="11" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.8" aria-hidden="true"><path d="M3 8h10M9 4l4 4-4 4"/></svg>'
EXT = '<svg width="11" height="11" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.8" aria-hidden="true"><path d="M5 11 11 5M6 5h5v5"/></svg>'
CHEV = '<svg width="14" height="14" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.8" aria-hidden="true"><path d="M4 6l4 4 4-4"/></svg>'

# ─────────────────────────────────────────────────────────────────────────────
# Simple pictures for the visible part of each card: plain words, few boxes.
# ─────────────────────────────────────────────────────────────────────────────

SIMPLE = {
  "agentlm": {
    "w": 540, "h": 240,
    "groups": [("t", 8, 14, 372, 214, "YOUR ENVIRONMENT", "lm")],
    "nodes": [
        ("req", 24, 50, 110, 48, "Your request", [], "n"),
        ("lm", 164, 42, 130, 64, "AgentLM", ["does most work"], "lm"),
        ("mem", 24, 160, 110, 48, "What it learned", [], "mem"),
        ("ans", 164, 160, 130, 48, "Checked answer", [], "ok"),
        ("fr", 410, 42, 122, 64, "Frontier LLM", ["hard cases only"], "llm"),
    ],
    "edges": [
        E("req:r@74", "lm:l@74"),
        E("mem:r@184", "lm:l@92", "recall", k="mem", via=[(149, 184), (149, 92)], lp=(149, 136)),
        E("lm:b", "ans:t", k="ok"),
        E("lm:r", "fr:l", "if needed", k="llm", dash=True),
    ],
  },
  "workflows": {
    "w": 540, "h": 210,
    "nodes": [
        ("go", 10, 40, 96, 52, "An event", ["alert · order"], "n"),
        ("s1", 130, 40, 80, 52, "Step", [], "wf"),
        ("s2", 230, 40, 80, 52, "Step", [], "wf"),
        ("s3", 330, 40, 80, 52, "Step", [], "wf"),
        ("done", 432, 40, 100, 52, "Done", ["every time"], "ok"),
        ("lm", 180, 142, 180, 50, "AgentLM", ["handles the unusual"], "lm"),
    ],
    "edges": [
        E("go:r", "s1:l"), E("s1:r", "s2:l", k="wf"), E("s2:r", "s3:l", k="wf"), E("s3:r", "done:l", k="ok"),
        E("s2:b@270", "lm:t@270", "doesn’t fit", k="lm", dash=True),
        E("lm:r", "s3:b@370", "back on track", k="lm", lp=(420, 120)),
    ],
  },
  "memory": {
    "w": 540, "h": 220,
    "nodes": [
        ("ag", 10, 82, 110, 56, "Your agents", [], "n"),
        ("mem", 196, 30, 180, 160, "Agent memory", ["past cases", "proven fixes", "how things connect"], "mem"),
        ("out", 420, 84, 112, 52, "Better answers", ["fewer model calls"], "ok"),
    ],
    "edges": [
        E("ag:r@98", "mem:l@98", "learn", k="mem"),
        E("mem:l@124", "ag:r@124", "remember", k="mem"),
        E("mem:r@110", "out:l", k="ok"),
    ],
  },
  "governance": {
    "w": 540, "h": 250,
    "nodes": [
        ("us", 10, 14, 112, 48, "A person", ["signs in"], "n"),
        ("ag", 10, 100, 112, 52, "Any agent", ["its own ID"], "n"),
        ("idp", 10, 190, 112, 48, "Your IdP", ["Entra · Okta"], "n"),
        ("gw", 168, 70, 136, 112, "A3T gateway", ["checks your rules", "holds the keys"], "pol"),
        ("ok", 346, 22, 186, 46, "Allowed: tool runs", [], "ok"),
        ("no", 346, 103, 186, 46, "Not allowed: stopped", [], "deny"),
        ("log", 346, 184, 186, 46, "Always logged", [], "mem"),
    ],
    "edges": [
        E("us:b", "ag:t", "delegates", dash=True),
        E("ag:r", "gw:l@126", "tool call"),
        E("idp:r", "gw:l@168", "identity", via=[(145, 214), (145, 168)], lp=(145, 200)),
        E("gw:r@90", "ok:l", k="ok"),
        E("gw:r@126", "no:l", k="deny"),
        E("gw:r@164", "log:l", k="mem"),
    ],
  },
}

# The AgentLM detail picture (the others reuse their FAQ diagrams).
AGENTLM_DG = {
    "w": 760, "h": 330,
    "groups": [("t", 10, 14, 560, 302, "YOUR TENANT", "lm")],
    "nodes": [
        ("in", 30, 128, 110, 54, "Request", ["typed input"], "n"),
        ("gate", 180, 50, 170, 56, "Gate", ["solve · think · escalate"], "lm"),
        ("lm", 180, 146, 170, 70, "AgentLM", ["attends to memory", "loads skills"], "lm"),
        ("mem", 180, 248, 170, 52, "Agent memory", ["knowledge · sessions"], "mem"),
        ("pol", 395, 50, 160, 56, "Cedar policy", ["may this leave?"], "pol"),
        ("wf", 395, 146, 160, 70, "Workflow output", ["schema-constrained", "dry-run checked"], "wf"),
        ("run", 395, 248, 160, 52, "Run", ["governed runtime"], "ok"),
        ("fr", 610, 50, 140, 56, "Frontier LLM", ["escalation only"], "llm"),
    ],
    "edges": [
        E("in:t", "gate:l", "prompt", lp=(85, 78)),
        E("gate:b", "lm:t", "solve", k="lm"),
        E("gate:r", "pol:l", "escalate", k="llm"),
        E("pol:r", "fr:l", "allow", k="pol"),
        E("fr:b", "wf:r@196", "answer", k="llm", dash=True, lp=(680, 160)),
        E("lm:r@166", "wf:l@166", "flow", k="lm"),
        E("mem:t", "lm:b", "attend", k="mem"),
        E("wf:b", "run:t", "ok", k="ok"),
        E("run:l", "mem:r", "outcomes", k="mem"),
    ],
}

# The Governance detail picture: the gateway pipeline from the identity spec.
GOV_DG = {
    "w": 760, "h": 330,
    "groups": [("gw", 150, 14, 440, 214, "A3T POLICY GATEWAY", "pol")],
    "nodes": [
        ("ag", 10, 44, 120, 56, "Any agent", ["workload ID"], "n"),
        ("us", 10, 160, 120, 56, "User", ["delegation"], "n"),
        ("v", 166, 52, 90, 56, "Verify", ["both tokens"], "pol"),
        ("r", 270, 52, 90, 56, "Resolve", ["live roles"], "pol"),
        ("d", 374, 52, 90, 56, "Decide", ["Cedar"], "pol"),
        ("a", 478, 52, 96, 56, "Act", ["adds the key"], "ok"),
        ("reg", 166, 150, 130, 56, "Run registry", ["pause · kill"], "mem"),
        ("rec", 336, 150, 120, 56, "Decision log", ["every call"], "mem"),
        ("vault", 464, 150, 120, 56, "Vault", ["agents never see it"], "n"),
        ("tools", 630, 46, 120, 68, "Your tools", ["MCP · APIs"], "ok"),
        ("idp", 150, 262, 170, 52, "Your IdP", ["Entra · Okta · Keycloak"], "n"),
        ("siem", 336, 262, 150, 52, "Your SIEM", ["OpenTelemetry"], "n"),
    ],
    "edges": [
        E("us:t", "ag:b", "delegates", dash=True),
        E("ag:r@76", "v:l@76", "tool call"),
        E("v:r", "r:l", k="pol"), E("r:r", "d:l", k="pol"), E("d:r", "a:l", k="ok"),
        E("a:r@80", "tools:l@80", "allowed", k="ok"),
        E("vault:t@524", "a:b@524", k="n", dash=True),
        E("d:b@410", "rec:t@410", k="mem"),
        E("rec:b@410", "siem:t@410", "export", k="mem"),
        E("idp:t@230", "reg:b@230", "leaver · role change", k="mem", dash=True),
    ],
}

# ─────────────────────────────────────────────────────────────────────────────
# Parts. Top: kicker, title, pitch, wins (plain words). Fold: everything else.
# ─────────────────────────────────────────────────────────────────────────────

PARTS = [
  {
    "id": "agentlm", "kicker": "AgentLM", "status": "Early access",
    "title": "Built-in agent model", "tagline": "Reads your memory first.",
    "pitch": "Before AgentLM does anything, it reads your enterprise memory: past cases, the procedures that worked and how your systems connect. With that context it does the everyday work itself, inside your environment. Only the genuinely hard cases go to a frontier LLM, so costs stay predictable and your data stays where it is.",
    "wins": ["Lower, predictable model costs", "Your data stays in your environment", "Answers checked before they run"],
    "cap": "AgentLM handles most requests itself and calls a frontier LLM only for the hard ones.",
    "lead": "AgentLM is A3T’s agent model. It runs on a single GPU inside your tenant, reads agent memory directly, and works inside your workflows and policies. Every request starts at a gate where AgentLM decides whether it can solve the request, how much thinking it needs, or whether to escalate to a frontier LLM. Its answer is a workflow that has been checked before it runs.",
    "dg": AGENTLM_DG,
    "dcap": "AgentLM, its memory and its output all stay inside your tenant. Only escalations cross the boundary, and only through Cedar.",
    "mech": [
        ("Solve-or-escalate gate.", "The model’s first layers estimate whether it can solve the request, which skills it needs and how much to think. If agent memory has no matching procedure, evidence or concept, it escalates regardless. A request it keeps costs nothing extra; one it escalates costs a fraction of one pass."),
        ("Reads memory directly.", "Knowledge-graph nodes are precomputed into attention blocks stored in agent memory. AgentLM attends to them instead of pasting text into a prompt."),
        ("Fixed-size sessions.", "A session is a fixed-size state plus a short window of recent text, saved to memory after every turn and restored on the next. Each turn processes only its new tokens."),
        ("Answers are workflows.", "Output is constrained to the CNCF Serverless Workflow schema and dry-run in A3T before it is returned."),
        ("Learns from outcomes.", "Verified outcomes and workflow changes are written back to memory. A background job consolidates episodes into knowledge for the next request."),
        ("Skills without retraining.", "Domain and tenant skills are small adapters stored in memory and paged onto the GPU when the gate asks for them. A new domain means training one adapter, not a new model."),
    ],
    "who": [
        ("builder", "Most requests are solved locally; frontier calls are reserved for the hard cases.", "a1"),
        ("gov", "Data stays in your tenant. Escalation is a policy decision, and every one is recorded.", "a3"),
        ("platform", "One GPU per model and fixed per-session memory: cost and latency you can plan.", "a4"),
    ],
    "note": "<b>Early access</b> covers the core model, memory attention and the solve-or-escalate gate. Skill adapters and trained thinking budgets are on the roadmap.",
  },
  {
    "id": "workflows", "kicker": "Workflows",
    "title": "Agents that become workflows", "tagline": "Known path, every time.",
    "pitch": "A3T runs the known path as code, the same way on every run, and picks up where it left off if something fails. Only the exceptions go to a model: AgentLM first, a frontier LLM only if AgentLM escalates. Each approved fix becomes the next workflow version, so your LLM bill shrinks and every change in behaviour is a reviewed diff.",
    "wins": ["Reliable, repeatable runs", "Survives failures and restarts", "Each fix becomes the next version"],
    "cap": "Known steps run as a workflow. Only the unusual case needs AgentLM.",
    "lead": "Workflows are written in the open <b>CNCF Serverless Workflow</b> spec and run on a Rust runtime with microsecond dispatch. Inputs are typed, every step is checkpointed, and the execution path is monitored. Only deviations from it go to a model: AgentLM first, a frontier LLM only if AgentLM escalates.",
    "dg_from": "w1",
    "mech": [
        ("Typed inputs.", "Every workflow declares its input schema, so bad input is caught at the edge instead of halfway through a run."),
        ("26 task types.", "Agent, model, MCP tool, human approval, switch, loop, transform, emit and more. Routine steps never spend a token."),
        ("Triggers and sandboxes.", "Start flows from webhooks, schedules and events, and run tasks in isolated or shared sandboxes."),
        ("Checkpoint and resume.", "State is saved after every task. A failed or restarted run resumes at the failed step without repeating side effects."),
        ("Monitored execution.", "Each run is compared with the expected path and recorded as an execution graph in agent memory. That graph is what AgentLM reads to handle a deviation and propose a change."),
        ("Versioned changes.", "A proposed change becomes a new workflow version once it is approved. The same YAML is on the canvas and in Git."),
    ],
    "code": ("contain-host.yaml", '''<span class="k">document</span>:
  <span class="k">dsl</span>: <span class="s">'1.0.0'</span>
  <span class="k">namespace</span>: secops
  <span class="k">name</span>: contain-host
<span class="k">input</span>:
  <span class="k">schema</span>: <span class="s">alert.v2</span>
<span class="k">do</span>:
  - <span class="k">triage</span>:
      <span class="k">call</span>: <span class="s">a3t:agent</span>
      <span class="k">with</span>:
        <span class="k">message</span>: <span class="s">${ .alert }</span>'''),
    "links": [("CNCF Serverless Workflow", "https://github.com/serverlessworkflow/specification")],
    "who": [
        ("builder", "Plain code for the known path; model calls only where judgement is needed.", "w1"),
        ("gov", "Every run is a graph you can trace step by step, and changes ship as approved versions.", "w5"),
        ("platform", "One Rust runtime, microsecond dispatch, no GC pauses, on Kubernetes.", "w3"),
    ],
  },
  {
    "id": "memory", "kicker": "Memory",
    "title": "Enterprise memory", "tagline": "Cases, procedures and how they connect.",
    "pitch": "Your agents keep what they learn: past cases, the procedures that worked, and how people, systems and incidents connect. AgentLM reads it directly, so agents recall instead of asking a model again, and you decide what is kept and what is forgotten.",
    "wins": ["No starting from scratch", "Fewer model calls", "Retention you control"],
    "cap": "Agents write what they learn to memory and read it back when they need it.",
    "lead": "Agent memory is one store for everything an agent knows: five memory types, a knowledge graph, and the sessions and attention blocks AgentLM reads directly. It is queried with <b>AQL, the Agent Query Language</b>, and every recall and write is checked against policy.",
    "dg_from": "m1",
    "mech": [
        ("Five memory types.", "Working, episodic, semantic, procedural and tool memory. Proven procedures are reused, so agents stop asking a model for what they already know."),
        ("AQL.", "RECALL, STORE, LINK and FORGET. Queries read like what the agent wants to know."),
        ("Knowledge graph.", "LINK connects memories into a graph agents can traverse, not just a pile of vectors."),
        ("Model memory.", "AgentLM’s sessions and attention blocks live here too. On a model upgrade they are recomputed from the source data."),
        ("Scopes and sharing.", "Namespaces keep agents and tenants apart; shared scopes let a team’s agents learn from each other."),
        ("Retention and a real forget.", "TTLs, history when facts change, and FORGET that actually removes data."),
    ],
    "code": ("recall.aql", '''<span class="k">RECALL FROM</span> EPISODIC
  <span class="k">WHERE</span> host = <span class="s">"lt-4471"</span>
  <span class="k">FOLLOW LINKS TYPE</span> <span class="s">"resolved_by"</span>
    <span class="k">INTO</span> PROCEDURAL
  <span class="k">RETURN</span> pattern, confidence'''),
    "links": [("AQL on GitHub", "https://github.com/srirammails/AQL"), ("AQL playground", "https://srirammails.github.io/AQL/playground/")],
    "who": [
        ("builder", "Agents remember across sessions and deployments, and recall by meaning.", "m1"),
        ("gov", "History, retention and forget you can show an auditor.", "m4"),
        ("platform", "One memory layer instead of a vector store, a cache and a database stitched together.", "m2"),
    ],
  },
  {
    "id": "governance", "kicker": "Governance",
    "title": "Governance on every action", "tagline": "Identity, delegation and a policy gate for any agent.",
    "pitch": "Any agent, built on A3T, LangChain, Strands or anything that speaks MCP, gets its own identity from your identity provider and works only with access a person has delegated to it. Every tool call goes through the A3T policy gateway, which checks it against your rules, uses credentials the agent never sees, and logs who did what, for whom and why. When someone’s access changes, their running agents pause or stop.",
    "wins": ["An identity for every agent", "Access delegated, never borrowed", "No credentials in agent code"],
    "cap": "Every tool call passes the gateway: identity checked, your rules applied, the key added, the decision logged.",
    "lead": "The A3T policy gateway sits between any agent and the tools it uses. It plugs into your identity provider (Keycloak, Entra, Okta, Auth0, Google or any OIDC), verifies the agent’s own identity and the user’s delegation on every call, decides with <b>Cedar</b>, and attaches the downstream credential itself. Agents need no auth code.",
    "dg": GOV_DG,
    "dcap": "Verify → Resolve → Decide → Act on every call. Credentials stay in the gateway, decisions go to your SIEM, and identity changes reach running agents.",
    "mech": [
        ("An identity for every agent.", "Each agent gets a stable ID with a human owner and a class that policy can target, plus a short-lived workload token at start. It can be mirrored to your IdP, such as Entra Agent ID."),
        ("Delegated, not borrowed.", "The user signs in through your IdP and delegates a scope, a set of tools and an expiry. Token exchange carries the user as subject and the agent as actor (RFC 8693, Entra on-behalf-of, ID-JAG)."),
        ("One gate for every tool call.", "Agents call tools through A3T’s MCP gateway. It verifies both identities, loads the user’s live roles, decides with Cedar, then acts. Agents only see the tools they may use, and risky calls can wait for a human approval."),
        ("Agents never hold credentials.", "The gateway keeps the downstream tokens and uses the user’s own delegated token wherever the app supports it, so the app’s permissions still apply. Nothing is written into the agent, its logs or traces."),
        ("Access changes reach running agents.", "A role change pauses affected runs at the next checkpoint. A leaver’s runs are stopped, their delegations revoked and their tokens deleted."),
        ("Audit you can prove.", "Every decision is logged with agent, user, delegation, tool and the policies that decided it, and exported over OpenTelemetry to your SIEM. Policy diffs and shadow mode show what a change would allow or deny before it ships."),
    ],
    "code": ("delegation.cedar", '''<span class="k">permit</span> (
  principal <span class="k">in</span> AgentClass::<span class="s">"itsm-assistant"</span>,
  action == Action::<span class="s">"servicenow__create_change"</span>,
  resource
) <span class="k">when</span> {
  context.on_behalf_of <span class="k">in</span> Role::<span class="s">"change-requester"</span> &amp;&amp;
  context.now &lt; context.delegation.expires_at
};'''),
    "links": [("cedarpolicy.com", "https://www.cedarpolicy.com"), ("RFC 8693 token exchange", "https://www.rfc-editor.org/rfc/rfc8693")],
    "who": [
        ("gov", "Every agent identified, delegated and checked, in policy you can read.", "p1"),
        ("builder", "No auth code in the agent: identity and credentials come from the gateway.", "p4"),
        ("platform", "Plug in the identity provider and gateways you already run.", "p5"),
    ],
  },
]

HW_CSS = r"""
  /* ═══════════════════════════════════════════════════════════ */
  /* HOW IT WORKS — generated by build-how.py */
  /* ═══════════════════════════════════════════════════════════ */
  #how-it-works { border-top: 1px solid var(--border); background: linear-gradient(180deg, var(--bg-warm) 0, var(--bg-page) 480px); }
  .hw-label { margin: 64px 0 16px; font-family: var(--font-mono); font-size: 11px; font-weight: 600; letter-spacing: .12em; text-transform: uppercase; color: var(--text-3); }
  .hw { display: grid; gap: 12px; }
  .hw-part { min-width: 0; background: var(--bg-surface); border: 1px solid var(--border); border-radius: var(--r-lg); box-shadow: var(--shadow-sm); scroll-margin-top: calc(var(--nav-h) + 16px); transition: border-color .15s, box-shadow .2s; }
  .hw-part:hover { border-color: var(--border-strong); }
  .hw-part[open] { box-shadow: var(--shadow); border-color: var(--border-strong); }
  .hw-part > summary { list-style: none; cursor: pointer; display: grid; grid-template-columns: 44px minmax(0, 1fr) 32px; align-items: center; gap: 14px; padding: 22px 24px; }
  .hw-part > summary::-webkit-details-marker { display: none; }
  .hw-part > summary:focus-visible { outline: 2px solid var(--ephemeral); outline-offset: 2px; border-radius: var(--r-lg); }
  .hw-num { font-family: var(--font-mono); font-size: 12px; font-weight: 500; color: var(--text-4); }
  .hw-head { display: flex; flex-direction: column; gap: 4px; min-width: 0; }
  .hw-title { font-family: var(--font-serif); font-size: 24px; font-weight: 500; letter-spacing: -.015em; line-height: 1.2; color: var(--text-1); }
  .hw-chev { width: 32px; height: 32px; border-radius: 50%; border: 1px solid var(--border); display: grid; place-items: center; color: var(--text-3); transition: transform .25s var(--ease), background .15s; }
  .hw-part[open] > summary .hw-chev { transform: rotate(180deg); background: var(--bg-subtle); }
  .hw-body { padding: 4px 28px 26px 82px; }
  .hw-top { display: grid; grid-template-columns: minmax(0, 1fr) minmax(0, 1.25fr); gap: 36px; align-items: center; }
  .hw-kick { display: flex; gap: 10px; align-items: center; font-family: var(--font-mono); font-size: 11px; font-weight: 600; letter-spacing: .1em; text-transform: uppercase; color: var(--text-3); }
  .hw-kick .st { padding: 2px 8px; border-radius: 999px; background: var(--checkpoint-bg); color: var(--checkpoint-ink); letter-spacing: .06em; }
  .hw-tagline { font-family: var(--font-serif); font-style: italic; font-size: 17px; color: var(--signal-ink); }
  .hw-pitch { margin-top: 0; font-size: 16px; color: var(--text-2); line-height: 1.6; }
  .hw-wins { list-style: none; display: flex; flex-wrap: wrap; gap: 8px; margin-top: 18px; }
  .hw-wins li { display: inline-flex; align-items: center; gap: 6px; padding: 5px 11px; border-radius: 999px; background: var(--ok-bg); color: var(--ok-ink); font-size: 13px; font-weight: 550; }
  .hw-fig { margin: 0; background: var(--bg-ivory); border: 1px solid var(--border); border-radius: var(--r-lg); padding: 14px 14px 10px; }
  .hw-fig figcaption { margin-top: 8px; font-size: 12.5px; color: var(--text-3); font-style: italic; }
  .dg-wrap { overflow-x: auto; -webkit-overflow-scrolling: touch; }
  .hw-fig svg.dg { min-width: 480px; }

  .hw-more { margin-top: 24px; border-top: 1px solid var(--divider); }
  .hw-more > summary { list-style: none; cursor: pointer; display: inline-flex; align-items: center; gap: 10px; padding: 16px 0 4px; font-size: 14px; font-weight: 550; color: var(--signal-ink); }
  .hw-more > summary::-webkit-details-marker { display: none; }
  .hw-more > summary .c { width: 26px; height: 26px; border-radius: 50%; border: 1px solid var(--border); display: grid; place-items: center; transition: transform .25s var(--ease); }
  .hw-more[open] > summary .c { transform: rotate(180deg); background: var(--bg-subtle); }
  .hw-more > summary .open-t { display: none; }
  .hw-more[open] > summary .open-t { display: inline; } .hw-more[open] > summary .closed-t { display: none; }
  .hw-more > summary:focus-visible { outline: 2px solid var(--ephemeral); outline-offset: 3px; border-radius: 6px; }
  .hw-tech { padding-top: 18px; }
  .hw-tech h4 { font-family: var(--font-mono); font-size: 11px; font-weight: 600; letter-spacing: .1em; text-transform: uppercase; color: var(--text-3); margin-bottom: 14px; }
  .hw-lead { font-size: 15.5px; color: var(--text-2); line-height: 1.65; max-width: 860px; }
  .hw-lead b { color: var(--text-1); font-weight: 600; }
  .hw-tech .arch-fig { margin: 22px 0 26px; background: var(--bg-ivory); border: 1px solid var(--border); border-radius: var(--r-lg); padding: 18px 18px 14px; }
  .hw-tech .arch-fig figcaption { margin-top: 10px; font-size: 12.5px; color: var(--text-3); font-style: italic; }
  .hw-steps { list-style: none; counter-reset: st; display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); gap: 12px 28px; }
  .hw-steps li { counter-increment: st; position: relative; padding-left: 34px; font-size: 14px; color: var(--text-2); line-height: 1.55; }
  .hw-steps li::before { content: counter(st, decimal-leading-zero); position: absolute; left: 0; top: 1px; font-family: var(--font-mono); font-size: 11px; font-weight: 600; color: var(--text-4); }
  .hw-steps li b { color: var(--text-1); font-weight: 600; }
  .hw-row { display: grid; grid-template-columns: minmax(0, 1.05fr) minmax(0, 1fr); gap: 28px; margin-top: 30px; align-items: start; }
  .hw-row.solo { grid-template-columns: minmax(0, 1fr); }
  .hw-who { display: grid; gap: 10px; }
  .hw-row.solo .hw-who { grid-template-columns: repeat(3, minmax(0, 1fr)); }
  .hw-who a { display: grid; gap: 6px; padding: 14px 16px; background: var(--bg-page); border: 1px solid var(--border); border-radius: var(--r-lg); font-size: 14px; color: var(--text-2); line-height: 1.5; transition: border-color .15s; }
  .hw-who a:hover { border-color: var(--border-strong); }
  .hw-role { justify-self: start; font-family: var(--font-mono); font-size: 10.5px; font-weight: 600; letter-spacing: .04em; text-transform: uppercase; padding: 3px 9px; border-radius: 999px; }
  .hw-role.builder { background: var(--ephemeral-bg); color: var(--ephemeral-ink); }
  .hw-role.gov { background: var(--ok-bg); color: var(--ok-ink); }
  .hw-role.platform { background: var(--signal-bg); color: var(--signal-ink); }
  .hw-who .more { display: inline-flex; align-items: center; gap: 5px; font-size: 12.5px; font-weight: 550; color: var(--signal-ink); }
  .hw-code .hiw-code { margin: 0; --ci: var(--signal-ink); font-size: 12px; }
  .hw-code .fn { font-family: var(--font-mono); font-size: 11px; color: var(--text-4); margin-bottom: 6px; }
  .hw-links { display: flex; flex-wrap: wrap; gap: 6px 16px; margin-top: 10px; }
  .hw-links a { display: inline-flex; align-items: center; gap: 5px; font-size: 13px; font-weight: 550; color: var(--signal-ink); }
  .hw-note { margin-top: 24px; padding: 14px 18px; border-radius: var(--r); border: 1px dashed var(--checkpoint); background: var(--checkpoint-bg); color: var(--checkpoint-ink); font-size: 14px; }

  @media (max-width: 980px) {
    .hw-top { grid-template-columns: minmax(0, 1fr); gap: 22px; }
    .hw-row { grid-template-columns: minmax(0, 1fr); }
    .hw-row.solo .hw-who { grid-template-columns: minmax(0, 1fr); }
  }
  @media (max-width: 640px) {
    .hw-part > summary { grid-template-columns: minmax(0, 1fr) 32px; padding: 18px 16px; }
    .hw-num { display: none; }
    .hw-title { font-size: 20px; }
    .hw-body { padding: 0 16px 18px; }
    .hw-steps { grid-template-columns: minmax(0, 1fr); }
    .hw-fig { padding: 10px; }
  }
"""


def diagram_css():
    """The svg.dg rules from the FAQ page, so the same diagrams render here."""
    return "\n".join(l for l in faq.FAQ_CSS.splitlines() if "svg.dg" in l) + "\n"


def part_html(p, num):
    if "dg_from" in p:
        src = FAQ_ITEMS[p["dg_from"]]
        dg, dcap = src["dg"], src["cap"]
    else:
        dg, dcap = p["dg"], p["dcap"]
    sid, did = f"hw-{p['id']}", f"hwd-{p['id']}"
    wins = "".join(f"<li>{w}</li>" for w in p["wins"])
    mech = "\n".join(f"<li><b>{b}</b> {t}</li>" for b, t in p["mech"])
    who = "\n".join(
        f'<a href="faq.html#{fid}"><span class="hw-role {role}">{faq.ROLES[role]}</span>'
        f'<span>{text}</span><span class="more">Read the question {ARR}</span></a>'
        for role, text, fid in p["who"])
    status = f'<span class="st">{p["status"]}</span>' if p.get("status") else ""
    side = ""
    if p.get("code"):
        fn, code = p["code"]
        links = "".join(f'<a href="{u}" target="_blank" rel="noopener">{t} {EXT}</a>' for t, u in p.get("links", []))
        side = f'<div class="hw-code"><h4>Example</h4><div class="fn">{fn}</div><pre class="hiw-code">{code}</pre><div class="hw-links">{links}</div></div>'
    note = f'<p class="hw-note">{p["note"]}</p>' if p.get("note") else ""
    return f'''
      <details class="hw-part reveal" id="{p["id"]}">
        <summary>
          <span class="hw-num">{num:02d}</span>
          <span class="hw-head">
            <span class="hw-kick"><span>{p["kicker"]}</span>{status}</span>
            <span class="hw-title">{p["title"]}</span>
            <span class="hw-tagline">{p["tagline"]}</span>
          </span>
          <span class="hw-chev" aria-hidden="true">{CHEV}</span>
        </summary>
        <div class="hw-body">
          <div class="hw-top">
            <div>
              <p class="hw-pitch">{p["pitch"]}</p>
              <ul class="hw-wins">{wins}</ul>
            </div>
            <figure class="hw-fig">
              <div class="dg-wrap">{render(sid, SIMPLE[p["id"]])}</div>
              <figcaption id="{sid}-cap">{p["cap"]}</figcaption>
            </figure>
          </div>
          <details class="hw-more">
            <summary><span class="c">{CHEV}</span><span class="closed-t">Show the technical details</span><span class="open-t">Hide the technical details</span></summary>
            <div class="hw-tech">
              <p class="hw-lead">{p["lead"]}</p>
              <figure class="arch-fig">
                <div class="dg-wrap">{render(did, dg)}</div>
                <figcaption id="{did}-cap">{dcap}</figcaption>
              </figure>
              <h4>How it works</h4>
              <ol class="hw-steps">{mech}</ol>
              <div class="hw-row{"" if side else " solo"}">
                <div><h4>Who it helps</h4><div class="hw-who">{who}</div></div>
                {side}
              </div>
              {note}
            </div>
          </details>
        </div>
      </details>'''


def section_html():
    parts = "".join(part_html(p, i + 1) for i, p in enumerate(PARTS))
    return f'''
<section class="section" id="how-it-works">
  <div class="container">
    <div class="section-head reveal">
      <div class="kicker violet">How it works</div>
      <h2 class="h2 s1">An agent model that reads<br/><em>your enterprise memory.</em></h2>
      <p class="h2-sub">AgentLM reads your enterprise memory directly: past cases, proven procedures and how your systems connect are part of how it thinks, not text pasted into a prompt. That is why it can solve most work itself, inside your tenant. Workflows give it a known path, and policy decides what it may do. Only what it can’t solve is routed to a frontier LLM, and only if policy allows.</p>
    </div>

    <figure class="hub reveal">
      <div class="hub-wrap">
{hub.svg()}
      </div>
      <figcaption id="hubCap">AgentLM sits on your enterprise memory and reads it as it works. It solves what it can inside your tenant; only what it can’t solve is routed, through policy, to a frontier LLM.</figcaption>
    </figure>

    <ol class="path reveal" aria-label="What happens to one request">
      <li><b>Attend</b><span>AgentLM reads the relevant enterprise memory directly, as part of how it thinks.</span></li>
      <li><b>Solve or route</b><span>It solves the request itself, or routes the hard case to a frontier LLM.</span></li>
      <li><b>Govern</b><span>Cedar policy checks every action it takes, routing included, for this agent and this user.</span></li>
      <li><b>Answer</b><span>Its answer is a typed workflow, checked against the schema before it runs.</span></li>
      <li><b>Run</b><span>The workflow runs checkpointed and audited.</span></li>
      <li><b>Learn</b><span>Verified outcomes go back to memory, so AgentLM knows more next time.</span></li>
    </ol>

    <div class="hw-label">The four parts</div>
    <div class="hw">{parts}
    </div>
    <script>
      /* Open a part when a link points at it (hero tiles, persona links, deep links). */
      (function () {{
        function openHash() {{
          const el = location.hash && document.getElementById(location.hash.slice(1));
          if (el && el.classList.contains('hw-part')) {{ el.open = true; el.scrollIntoView({{ behavior: 'smooth', block: 'start' }}); }}
        }}
        window.addEventListener('hashchange', openHash);
        document.querySelectorAll('a[href^="#"]').forEach(a => a.addEventListener('click', () => {{
          if (a.getAttribute('href') === location.hash) openHash();
        }}));
        openHash();
      }})();
    </script>
  </div>
</section>
'''


def splice(text, start, end, body):
    pat = re.compile(re.escape(start) + r".*?" + re.escape(end), re.S)
    assert pat.search(text), f"missing markers {start} … {end}"
    return pat.sub(lambda _: start + body + end, text, count=1)


def main():
    path = HERE / "index.html"
    src = path.read_text()
    src = splice(src, "<!-- HOW-IT-WORKS:START -->", "<!-- HOW-IT-WORKS:END -->", section_html())
    src = splice(src, "/* HOW-IT-WORKS-CSS:START */", "/* HOW-IT-WORKS-CSS:END */", HW_CSS + diagram_css())
    src = splice(src, "/* HUB-CSS:START */", "/* HUB-CSS:END */", hub.CSS)
    path.write_text(src)
    print(f"Wrote How it works into index.html ({len(PARTS)} parts)")


if __name__ == "__main__":
    main()
