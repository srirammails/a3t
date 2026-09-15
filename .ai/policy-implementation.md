# Runtime Policy — Layered Authorization, Interactive Grants, and the IGA Seam

**Status:** Phase 1 landed and verified; Phases 2–5 specified, not started
**Version:** 0.1 — September 2026
**Primary purpose of this document:** Define how an agent's *permitted* surface is decided at runtime, how a developer authors that permission interactively from chat, and where the boundary sits between the runtime, AESOP, and an external IGA. Sections 4–8 are the actionable part; 1–3 are the model everything else depends on.

**Related:** `aesop-design-and-a3t-ce-contract.md` (the CE/EE seam), `A3T_IAM_AND_AUTHORIZATION.md` (identity), `A3T_EE_E2-5_ROLES_PLAN.md` (role canonicalization).

---

## 1. The model

An agent is described by three separable things. Keeping them separate is the whole architecture.

| Concern | Where it lives | Shape |
|---|---|---|
| **What the agent does** | the flow | instruction set + tool set offered to the LLM |
| **What the agent is allowed to do** | policy | rules over those instructions and tools |
| **What the agent operates as** | the trigger source | identity, derived from the invoker |

The first two are *configuration to the agent runtime*. The third is not configuration at all — it is derived, and an agent must never be able to declare it.

The runtime's own job is small and fixed: **two choke points** that check tool calls and task steps. Everything else is policy or flow. No policy logic belongs in the runtime.

### 1.1 Identity comes from the trigger, never from the agent

- Invoked from chat → the agent inherits the human's identity and authority, attenuated.
- Invoked by another agent → inherits the parent agent's *already-attenuated* authority, so a delegation chain can only narrow.
- Invoked by an event/trigger with no authenticated caller → runs as a service principal with **no inherited roles at all**. It stands on its own attached policy or it does not run.

A service has no roles to lend. An agent that could manufacture authority from ambient trust is the identity-collapse failure, and it is the single most important property here.

---

## 2. Policy layers

Four layers contribute to one decision:

1. **Standard / org policy** — the organization's floor. Compiled baseline plus InfoSec-authored policy resources.
2. **Workflow / agent / template policy** — scoped to a flow, agent, or template.
3. **Execution-time grants** — authorized during a run by the person running it.
4. **Approved entitlements** — durable grants that exceeded the granter's own authority and were approved by someone who held it.

### 2.1 Layers attenuate; they never override

This is the load-bearing rule, and it is a deliberate departure from the intuitive "inner layer overrides outer layer" framing.

If an inner layer can override an outer one, the standard policy stops being a ceiling and becomes a default. Two things are lost: the organizational guarantee, and static analyzability — you can no longer answer *"can anyone, anywhere, do X?"* by reading the policy set, because some agent config may have overridden it.

There is no need for overrides, because the thing people want from them is already available in a safer form. **An "override with an approver" is not a merge operation — it is a delegation record**: a new `permit`, contributed by someone who provably held the authority to give it. That composes cleanly, because it *adds* authority that demonstrably existed rather than *subtracting* a restriction.

### 2.2 The composition discipline

To get "inner operates within outer" from Cedar's native semantics with no custom merge algorithm:

- **Outer layers may write `forbid`.** These are ceilings.
- **Inner layers contribute only `permit`s (grants) and _data_** — the ceiling values that central forbids read (`exposedTools`, `enabledTools`, `tags`, `runtimePolicy`).
- Concatenate every layer into **one Cedar policy set** and let `forbid`-overrides-`permit` do the work.

An agent-level policy that wants to *restrict* does not author a forbid. It narrows its declared data and the existing central forbid binds it. This is already how `exposedTools ∩ enabledTools` works, and it is why that design was right: **rules central, data per-template.**

Consequence: the policy set stays statically analyzable, and there is no merge code to get wrong.

### 2.3 Grants are policy documents, not a special case

All grant scopes are Cedar `permit`s. Only their lifetime and storage differ, so a session-scoped "yes" and a persisted one cannot drift apart in behaviour.

| Scope | Lifetime | Storage | Needs approval? |
|---|---|---|---|
| `Once` | one call, consumed | memory | no — within granter's own authority |
| `Session` | the chat session | memory, TTL'd | no — same |
| `Agent` | until revoked | registry resource (a file) | no, **if** within the owner's authority |
| — | until expiry/recert | entitlement store | **yes** — exceeded the owner's authority |

---

## 3. Four outcomes, and how an allow/deny engine produces a question

The decision is one of four: **execute**, **do not execute**, **ask the current user**, **ask an approver on behalf of the trigger**.

Cedar only ever allows or denies. `Ask` is therefore **derived, not declared**:

> When the *agent* is denied, re-run the identical request as the human it acts for. Allowed for them but not for the agent = a delegation that human is entitled to widen, so it becomes a prompt. Denied for both = a real denial.

This is what stops the approval card from becoming a privilege-escalation path: **nobody is ever asked to grant authority they do not already hold**, so the worst a prompt can do is re-delegate something the approver could have done by hand. The property is structural, not a convention to remember.

A principal with no approver behind it (a service/triggered run) never produces an `Ask` — its denials stay denials.

---

## 4. Responsibility boundaries

| Component | Role | Owns | Must never |
|---|---|---|---|
| **Runtime** (`flowr-ce`) | PEP | choke points, request construction, policy-set assembly, enforcement, audit emission | contain policy logic |
| **AESOP** | PDP | Cedar evaluation, schema validation, policy composition, `Ask` derivation | know or assign identity |
| **IAM** (Keycloak) | identity | authentication, federation, role/group claims | make authorization decisions |
| **IGA** | entitlement lifecycle | approval workflow, expiry, recertification, attestation audit | sit in the hot path |

The runtime runs on **every** tool call, so it must be fast and decision-free. It asks; it never decides.

### 4.1 Which side a grant falls on — decided by lifetime

- `Once` / `Session` / owner-bounded `Agent` grants: **no IGA**. No new authority was created; someone merely re-delegated what they already had.
- A grant **beyond the owner's authority**: an *entitlement request*. Needs an approver, an expiry, a recertification date, an audit record. This is IGA's job.

**Do not build approval workflow inside AESOP.** AESOP decides. The request/approve/expire/recertify lifecycle is IGA's, even when v1 of "IGA" is a table and a review page in our own console.

### 4.2 On adopting an external IGA

Keycloak is **not** IGA — it is IAM. It stores role assignments and has no view on whether they remain justified: no certification campaigns, no SoD engine, no approval chains, no entitlement catalog.

The credible open-source IGA is **midPoint** (Evolveum, Apache 2.0) — provisioning, role lifecycle, SoD, access requests, certification campaigns. **Apache Syncope** is the lighter alternative. Both model *human* identities with employment lifecycles, and will fight the shape of agent identities, which are ephemeral, numerous, and often session-scoped.

**Recommendation: do not adopt one yet.** Build v1 in-console, but keep the grant store's interface explicitly IGA-shaped (`request → approve → expire → recertify`) so midPoint can back it later if an enterprise customer demands campaigns and connectors. Designing to that interface now costs nothing.

---

## 5. Current state (verified)

### 5.1 Already working before this phase

- Two choke points: `dispatch_to_handler` (flow steps, `swf-runtime/src/executor/mod.rs`) and `authorize_tool_call` (agent/MCP tool calls, `flowr-mcp-core/src/tools.rs`).
- Layer 1 and Layer 2 both exist. `CedarHook::with_registry_policies` composes **baked baseline ⊕ attached registry policies**, and schema fragments deep-merge.
- Per-template attachment: a policy opts in by name via `metadata.a3t.policyRef`, or auto-attaches on a `tags` match. Proven by `tag_selector_scopes_the_forbid`.
- Flow capability ceiling `exposedTools ∩ enabledTools`, attenuate-only on subflow merge.
- Policy authoring UI: list / author / hot-reload / dry-run (`PoliciesPage.svelte`, `/security/policies*`).

### 5.2 Landed in Phase 1

**`PolicyDecision::Ask`** (`shared/flowr-policy/src/lib.rs`) with `GrantScope { Once, Session, Agent }`. `is_allow()` is false for `Ask`; `non_interactive()` collapses it to `Deny` preserving the reason, so the audit trail distinguishes *refused* from *nobody was there to ask*.

**Every decision consumer is now exhaustive.** Six sites used `if let ... Deny`, which would have let a new variant fall through **as allowed**. Sites that cannot reach a human call `non_interactive()`.

**Agent authority.** `Principal::Agent` gained `roles` and `owner`. Previously `on_behalf_of` carried the user's *name* but none of their *authority*, so an agent run by a Viewer and one run by an Admin were indistinguishable to policy and matched the same blanket permit. `Principal::agent_for()` derives them attenuate-only: caller's roles ∩ the agent's declared `role_ceiling`. Emitted to Cedar under the same attribute name a `User` uses, so one rule covers both.

**`Ask` derivation** in `CedarHook` via `approver_of()` (§3).

**Fixed: EE could not build at all.** `flowr-aesop/Cargo.toml` and `flowr-server/Cargo.toml` pointed at `../../../../aesop/crates/...`; the directory is `AESOP`, and the mount is case-sensitive.

| Suite | Result |
|---|---|
| `flowr-policy` | 25 passed (6 new) |
| `aesop-core --features cedar` | 9 passed (1 new) |
| `flowr-aesop` (EE, real baseline) | 36 passed (3 new) |
| `swf-runtime` policy suites | 6 passed |

The three EE tests that matter: an Admin's agent on a destructive tool → `Ask`; a **Viewer's** agent on the same tool → `Deny`, never asked; an ambient/triggered agent → `Deny`.

### 5.3 Known broken / misleading

- **Ownership does not exist.** `mcp_create_agent` takes no principal and stores no owner; `create_flow` likewise. `AgentsList.svelte:112` hardcodes `created_by: r.created_by ?? 'usr-superadmin'` — the UI displays provenance the backend never recorded. This blocks everything in §6.
- `flowr-storage-dragonfly` does not implement `settings()` on `StorageBackend`; `cargo check --workspace` is red for this pre-existing, unrelated reason.
- Keycloak tokens lack `realm_access`, so role canonicalization happens runtime-side.
- `flowr-worker` (`RedisExecutor`) is a second execution engine with **no choke point at all**. It never calls `policy_hook().check()`.

---

## 6. Phase 2 — Ownership (prerequisite)

Nothing in the owner-bounded grant model means anything until ownership is real.

1. Thread `principal` into `mcp_create_agent` and `create_flow`; stamp `owner` at creation. Follow the pattern already used for chat sessions in `call_agent.rs` (`session_index_key`, owner stamping).
2. Backfill existing agents/flows to an explicit `owner: null` rather than an invented one.
3. **Remove the `?? 'usr-superadmin'` fallback** in `AgentsList.svelte` and render "unowned" honestly.
4. Add `Resource::agent(...)` carrying `owner` so policy can express "the owner of this agent".

**Verify:** create an agent as two different users; each sees their own as owned. An unowned legacy agent shows as unowned, not as superadmin's.

---

## 7. Phase 3 — Grant store and the interactive loop (dev-time)

The smallest slice that makes the loop real. Ephemeral only: no durability, no IGA.

1. **`GrantStore`** — session-scoped, in-memory, TTL'd. Holds Cedar `permit` strings keyed by session. Composed into the policy set at decision time alongside baseline + registry policies, so grants evaluate through the **same** path.
2. **Approval broker** — reuse `InputManager` (`flowr-ce-agents/src/inputs.rs`): pending request + `oneshot`, resumed by `POST /inputs/:id`. Already routed.
3. **Chat approval card** — the agent loop calls `authorize_tool_call_decision` (added in Phase 1) instead of `authorize_tool_call`, and on `Ask` emits a `data-authz-request` stream part. The UI renders a card carrying tool name, arguments, and the offered scopes. Same data-part path as the existing artifact widgets.
4. **Minting** — an approval writes a `permit` narrowly scoped to the exact action/resource, never a blanket grant. Both the `Ask` and the answer are audited.

**Verify:** create an agent with no grants → run it in chat → it asks → "allow for session" → the same call succeeds for the rest of the session and is denied again in a new one. A Viewer must never see the card (`Deny`, per §3).

---

## 8. Phase 4 — Persisted agent grants

1. **`type_ref: agent-grant`** registry resource. Deliberately **not** `type: "cedar"`: the existing SoD forbid restricts `cedar` writes to SecurityAdmin/SuperAdmin, and a developer clicking "apply to agent" must not require InfoSec for something within their own authority. A separate type keeps separation of duties intact instead of relaxing it.
2. A rule bounding `agent-grant` writes to the agent's **owner**, and only for authority the owner holds. An `agent-grant` may never widen beyond the granter.
3. Cloning: "apply to agent" starts from the agent's current effective policy and adds the one permit — visible and diffable, not a hidden mutation.
4. Policy viewer: show an agent's effective grants on the agent widget, with provenance (who granted, when, which scope).

**Verify:** an owner grants within their authority → persists, survives restart. An owner attempting to grant beyond their authority → refused, and routed to Phase 5.

---

## 9. Phase 5 — Entitlement requests (the IGA seam)

For grants that exceed the owner's authority.

1. `GrantRequest` entity: requester, agent, requested permit, justification, status, approver, expiry, recertification date.
2. Review queue for SecurityAdmin. `EarlyAccessRequests.svelte` is a usable UI precedent.
3. On approval, mint a Phase 4 grant **with an expiry**. Entitlements without expiry are how access accretes.
4. Keep the interface IGA-shaped (§4.2) so midPoint can back it later.

**Verify:** a non-privileged owner requests a destructive-tool grant → appears in the SecurityAdmin queue → approval mints an expiring grant → the agent's next run succeeds → after expiry it asks again.

---

## 10. Explicitly out of scope (named, not designed)

- **Durable suspension for triggered runs.** "Ask an approver on behalf of the trigger" implies asynchronous approval: the run suspends, a request goes out, it resumes hours later or times out. `InputManager` holds pending requests in an in-process `RwLock<HashMap>` with a `oneshot` channel — a restart loses every pending approval and the suspended run never resumes. Fine for dev-time chat where the human is present; **not** fine for a triggered production run. This is real work and the natural boundary between now and later.
- **`flowr-worker` choke point** (§5.3). A structurally separate execution engine needing its own PEP design.
- **Adopting midPoint/Syncope** (§4.2).
- **Externalizing the policy-set assembly** into a distribution layer (e.g. OPAL) — plumbing, only once there are multiple runtimes to feed.
- **Non-human-identity governance as a product surface.** No mature open-source IGA models ephemeral agent identities; that gap is arguably A3T's opportunity rather than a dependency's. Noted, not scoped.

---

## 11. Open questions

1. **Revocation.** Grants expire, but an owner should be able to revoke one immediately. Registry hot-reload covers persisted grants; session grants need explicit invalidation.
2. **Grant visibility to the agent.** Should the LLM be told which tools it has been granted, or only discover it by being refused? Telling it is friendlier and risks prompting it to ask for more.
3. **`Ask` fatigue.** A chatty agent could prompt repeatedly. Per-session dedup by (action, resource) is probably enough, but the policy for "asked and denied" needs deciding — re-ask, or stay denied for the session?
4. **Recertification cadence** for approved entitlements, and who owns the campaign when the granting SecurityAdmin has left.
