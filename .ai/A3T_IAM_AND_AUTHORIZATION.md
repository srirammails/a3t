# A3T Identity, Access & Authorization (IAM + Cedar)

> Status: Design proposal · 2026-07-24
> Supersedes the standalone `CEDAR_POLICY.md` (folded in here).
> Scope: authentication (who you are), identity/role management (what you are), and authorization (what you may do) across ClawdFlow / FlowR / A3T.
> **Edition: the entire RBAC + Cedar + IAM-federation layer is FlowR Enterprise Edition (EE). Community Edition (CE) runs the same engine and UI but ignores this layer entirely — see §2.**

---

## 1. The three planes (and why they must stay separate)

Access control is three distinct problems. A3T keeps them cleanly layered so each can be sourced independently — and so the whole of Planes 2–3 can be switched off in CE.

```
┌── PLANE 1 · AUTHENTICATION (IAM) ──────────────────────────────┐
│  "Who are you?"  → a verified identity + claims               │
│  CE: email + Argon2 + JWT   |   EE: + OIDC/SAML (Entra…), MFA  │
└───────────────────────────────┬────────────────────────────────┘
                                 │ identity (+ role/group claims in EE)
┌── PLANE 2 · IDENTITY & ROLES (RBAC)  ─── EE only ──────────────┐
│  "What are you?" → users, roles, group memberships            │
│  CE: everyone is an implicit full editor (no roles)           │
└───────────────────────────────┬────────────────────────────────┘
                                 │ principal + attributes (role, org)
┌── PLANE 3 · AUTHORIZATION (Cedar)    ─── EE only ──────────────┐
│  "What may you do, on this resource, right now?"              │
│  CE: permissive — allow all. EE: Cedar decision, deny-default.│
└─────────────────────────────────────────────────────────────────┘
```

**Load-bearing principles**
- Plane 1/2 are commodities you can buy/federate; **Plane 3 (Cedar) is A3T's domain and always internal** (§8).
- **CE ships Plane 1 only** (simple accounts). Planes 2–3 are the EE layer, additive and skippable (§2).

---

## 2. Editions — Community (CE) vs Enterprise (EE)

Everything about roles, policy, and federated identity is an **EE layer**. CE is fully functional and simply *ignores* it: log in, everyone edits everything. The EE layer is **additive, license-gated, and non-destructive** — turning it off returns to CE behavior with data intact.

### 2.1 What's in each edition

| Capability | CE | EE |
|---|:--:|:--:|
| Workflow / agent engine (ClawdFlow), Console UI | ✓ | ✓ |
| **Simple user accounts + passwords** (login) | ✓ | ✓ |
| Multiple roles / RBAC | ✗ *(everyone = full edit)* | ✓ *(6 roles + custom)* |
| Cedar authorization (field / tool / runtime) | ✗ *(permissive: allow all)* | ✓ |
| Template policy enforcement (policy-as-resource) | ✗ *(metadata parsed, not enforced)* | ✓ |
| IAM federation (OIDC/SAML, MFA, SCIM) | ✗ | ✓ |
| Settings → Users | basic (add/remove, set password) | full (roles, invite, IdP) |
| Settings → Roles / Policies / Auth(IdP) / Audit | hidden / "Enterprise" badge | ✓ |
| Agent runtime capability enforcement | ✗ | ✓ |

### 2.2 CE ignores the EE layer (graceful degradation)

The system is built so the EE layer is one boolean at the decision boundary. In CE (or EE-with-enforcement-off):

- **AuthZ engine** → the `permissive` enforcer already in the codebase: every authenticated user may do everything. No policy 403s.
- **Roles** → none. Every logged-in user is effectively an editor/admin.
- **Unified slide-in editor** → no field locking; `editField` short-circuits to *Permit*, so all fields are always editable. (This is why CE "lets everyone edit what they want.")
- **Agent runtime** → no tool/memory/LLM capability checks.
- **Template policy metadata** → still *parses* (it is ordinary CNCF `metadata`), but is **not enforced**. A template authored in EE runs unchanged in CE — just without its guardrails. **Portability by design.**

> The single line that separates the editions: `if (!ee.enforcing) return Permit; else return cedar.authorize(...)`.

### 2.3 Two runtimes — CE and EE are separate images

There are **two runtime images**, from two source trees:

- **CE runtime** — public repo/image. ClawdFlow engine + console + **simple accounts & passwords** + **permissive** authorization. Contains **zero EE code** (the EE crates are private and never published to the CE GitHub).
- **EE runtime** — built from **private** crates (Cedar PDP, extended RBAC, IAM federation). Distributed as a **private image**.

CE is never "unlocked into" EE by a flag — because the EE code isn't in the CE image to unlock. Going EE means **running the EE image**.

### 2.4 Activating EE from CE (license → download → restart as EE)

The CE Settings menu shows the **full feature set**, including EE features (Roles, Policies, SSO), each marked and locked. Selecting one triggers activation:

- **Preferred (automated):** a small **WASM activation proxy** loaded in the CE console takes the **license key**, authenticates to the private registry, **downloads the EE engine image**, and **restarts the runtime as EE** (swap the deployment image → EE, carry the same data volumes and Secrets). The user experiences "enter license → it upgrades itself."
- **Fallback (simple):** if the in-console auto-upgrade is too complex to ship first, selecting an EE feature simply shows **"Enterprise feature — contact support"** with a CTA. Support/ops performs the image swap. *Ship this first; add the WASM auto-upgrade later.*

Either path is **non-destructive**: same users, templates, and data volumes; templates already carry policy `metadata` that CE ignored and EE now enforces. No re-auth beyond the restart.

### 2.5 The edition descriptor

A single backend descriptor drives all UI gating (works in both runtimes):

`GET /edition → { edition: "ce"|"ee", enforcing: bool, features: ["cedar","rbac","oidc","scim","mfa"] }`

- **CE image** returns `edition:"ce"`, `features:[]` → console shows EE tabs locked with the contact-support / activation CTA.
- **EE image** returns `edition:"ee"` with the licensed feature list → EE tabs enabled.

No edition logic scattered in the frontend — components gate on `features.includes('cedar')`, etc.

---

## 3. Where A3T is today

| Plane | Current | Gap |
|---|---|---|
| AuthN | JWT (HS256), Argon2, audience-bound (RFC 8707), MCP `/message` protected | No SSO/OIDC, no MFA *(both EE)* |
| Roles | Casbin, seeded in bootstrap | Roles hardcoded; only `superadmin` used; no UI *(EE)* |
| AuthZ | Casbin `sub×obj×act` | No attributes/ownership/org/per-field/runtime rules *(EE → Cedar)* |
| Editions | flowr-server always enforces | No CE/EE gate; no `permissive`-by-default path wired to a setting |

Note: the code already contains `RbacEnforcer::permissive()` — the CE path is largely a matter of defaulting to it and gating enforcement behind the EE setting.

---

## 4. Plane 2 — Users & Roles  *(EE; CE has simple accounts only)*

### 4.1 Roles are data; the engine is a library
- **Store** (users, roles, memberships) = *data* in LanceDB (or IdP-synced in EE). Managed in Settings. Never hardcoded.
- **Engine** = a library. **Recommendation: Cedar as the single engine** (it does RBAC *and* ABAC) — retire Casbin so there is one language and one audit trail. Roles become Cedar principal groups; RBAC rules become ordinary `permit` policies (§6).
- **CE**: no role engine at all — permissive. Users exist only for login.

### 4.2 The six default roles *(EE)*
Seeded when EE enforcement is enabled; editable afterward. Built around **separation of duties** — whoever runs infrastructure is *not* whoever writes the policy that constrains it.

| Role | Purpose | Edit Cedar policy? | Manage users/roles? |
|---|---|:--:|:--:|
| **SuperAdmin** | Break-glass platform owner | ✅ | ✅ |
| **SecurityAdmin** | Owns access & policy (IAM + Cedar), audit | ✅ | ✅ |
| **Admin (Infra)** | Deploy agents, DB/MCP/LLM resources, clusters | ❌ | ❌ |
| **Developer** | Build templates/agents/flows in their org | ❌ | ❌ |
| **KnowledgeAdmin** | Data steward: ClawDB memory, knowledge graph | ❌ | ❌ |
| **Viewer** | Read-only | ❌ | ❌ |

**Only SuperAdmin + SecurityAdmin edit Cedar policy** — enforced by the `forbid` guardrail (§6.4). Splitting Admin(Infra) from SecurityAdmin is the point: an infra admin must not rewrite the policy that limits them.

### 4.3 Default permission matrix *(EE)*
`C R U D`, `X`=execute/deploy, `—`=none, *own/org*=scoped.

| Resource → | Templates | Agents | Resources | Flows | Knowledge/Memory | Cedar Policies | Users & Roles | Auth/IdP | Audit |
|---|---|---|---|---|---|---|---|---|---|
| **SuperAdmin** | CRUD | CRUDX | CRUD | CRUDX | CRUD | CRUD | CRUD | CRUD | R |
| **SecurityAdmin** | R | R | R | R | R | **CRUD** | **CRUD** | **CRUD** | **R** |
| **Admin (Infra)** | R | CRUDX | **CRUD** | CRUD | R | R | R | R | R |
| **Developer** | CRUD *own/org* | CRUD *own* | R+use | CRUD *org* | R+use | R | — | — | R *own* |
| **KnowledgeAdmin** | R | R | R | R | **CRUD** | R | — | — | — |
| **Viewer** | R | R | R | R | R | R | — | — | — |

Defaults only — orgs may add custom roles in Settings (data, no code change). **In CE this whole table collapses to "everyone: CRUDX everything."**

---

## 5. Plane 1 — Identity & Authentication

**CE — internal, exists today:** email + Argon2 password, JWT with `aud`. Add (still CE): invite flow, password policy.
**EE — federated:** OIDC/SAML to the customer IdP (Entra/Okta/Google/Keycloak); A3T is an OIDC Relying Party. Group claims → A3T roles (mapping in Settings). MFA. SCIM for user lifecycle.

Below the identity+claims boundary, everything (roles → Cedar) is identical regardless of source.

---

## 6. Plane 3 — Authorization (Cedar) *(EE)*

### 6.1 Concerns unified
1. Platform RBAC (subsumes Casbin) · 2. Template→Agent **field editability** · 3. **Agent runtime capability** · 4. MCP tool authorization.
All are `(principal, action, resource, context) → Permit/Deny`, deny-by-default, `forbid` wins.

### 6.2 Model
Entities `User`, `Role`(hierarchical), `Org`, `Template`, `Agent`, `Resource`, `Flow`, `MemoryScope`, `Tool`. Hierarchy: `User in Org`, `User in Role`, `Agent in Template in Org`.

### 6.3 Policy is a **resource type**, attached to templates
Cedar policies live as first-class registry resources (`type_ref: cedar`), same shape as LLM/MCP resources:

```yaml
- name: agent-runtime-guardrails
  type_ref: cedar
  tags: [runtime, sandboxed]
  config:
    cedar: |
      permit(principal is Agent, action == Action::"invokeTool", resource is Tool)
      when  { resource.name in principal.template.exposedTools }
      unless{ principal.template.runtimePolicy == "sandboxed" && resource.name in ["shell","container"] };
```

Attach two ways (support both): **explicit ref** (`metadata.a3t.policyRef`) and **tag selector** (`appliesTo: tag=pii` auto-attaches to all templates tagged `pii`). Per-template **data** lives in `metadata` (§7); **rules** stay central in the policy resource — written once, reading those attributes.

### 6.4 Governance guardrail (SoD enforcer)
```cedar
forbid (principal, action in [Action::"updateResource", Action::"deleteResource"], resource is Resource)
when   { resource.type == "cedar" }
unless { principal in Role::"SecurityAdmin" || principal in Role::"SuperAdmin" };
```

### 6.5 Agent as a **principal** (runtime "what an agent can/can't do")
Capability = **template.exposed ∩ instance.enabled ∩ org guardrails**.
```cedar
permit (principal is Agent, action == Action::"invokeTool", resource is Tool)
when   { resource in principal.template.exposedTools && resource in principal.enabledTools };

// spawn:agent delegation — a sub-agent may never exceed its spawner (runtime confused-deputy
// guard; the executor analogue of the RFC 8707 audience binding already shipped on /message).
forbid (principal is Agent, action == Action::"spawnAgent", resource is Agent)
when   { !(resource.template.exposedTools.containsAll(principal.enabledTools)) };
```
Enforced in ClawdFlow's executor before each tool/memory/LLM call; a deny raises CNCF's own `.../errors/authorization` 403. **In CE the executor skips this — agents run unconstrained.**

---

## 7. CNCF conformance — where policy config lives

CNCF Serverless Workflow specifies **no** authorization construct (only a 403 error type + `use.authentications` for outbound auth). Policy *config* therefore goes in `document.metadata` — the spec's "implementation-specific key-value pairs." Split **data** (in template) from **rules** (central):

```yaml
document:
  dsl: '1.0.3'
  namespace: a3t
  name: rtb-bidder
  version: '2.1.0'
  metadata:
    a3t:
      policyRef: a3t/agent-runtime-guardrails       # rules (central resource)
      bindableFields: ["config.llm","config.system_prompt"]   # data
      lockedFields:   ["use.extensions","do"]
      exposedTools:   ["query","recall","http"]
      runtimePolicy:  "sandboxed"
```
Generic CNCF runtimes (and CE) ignore `metadata`; EE reads and enforces it. Fully conformant, and **the same template runs in both editions**.

---

## 8. Settings menu

Top-level **Settings**, itself access-gated. EE-only tabs badged "Enterprise Edition" and hidden/locked in CE.

| Tab | Edition | Who | Contents |
|---|---|---|---|
| **General / Enterprise Authorization** | CE | SuperAdmin | The EE enablement toggle + license entry (§2.3). |
| **Users** | CE (basic) / EE (full) | CE: SuperAdmin · EE: Security/SuperAdmin | CE: add/remove, set password. EE: roles, invite, IdP-managed badge. |
| **Roles** | **EE** | Security/SuperAdmin | 6 defaults + custom; grants; hierarchy. |
| **Authentication** | **EE** | Security/SuperAdmin | Internal vs OIDC/SAML IdP, MFA, SCIM, group→role mapping. |
| **Policies** | **EE** | Security/SuperAdmin | Cedar policy *resources*: edit/version/**test** (dry-run a decision); attach by ref/tag. |
| **Audit** | **EE** | SecurityAdmin (full) | Unified authN+authZ decision log; export. |
| **Infrastructure** | CE | Admin(Infra)+ | Clusters, data dirs, registry, resource defaults. |

Each tab's visibility/write access is itself a Cedar decision in EE (permissive in CE).

---

## 9. Build vs Buy — internal or plug into Entra / Saviynt / SailPoint?  *(EE concern)*

**Neither extreme — split by plane.** The external categories solve different planes and none solves Plane 3.

| System | Category | Solves | Doesn't do |
|---|---|---|---|
| **Entra ID / Okta / Auth0 / Keycloak** | **IdP** | Plane 1: SSO, MFA, OIDC/SAML, directory | Plane 3 authZ; app-specific roles |
| **Saviynt / SailPoint** | **IGA** | Plane 2 governance: SCIM provisioning, access requests/certification, SoD, compliance | Runtime authN; fine-grained authZ |

| Plane | Standalone | Enterprise | Build/Buy |
|---|---|---|---|
| 1 · AuthN | internal (shipped) | federate via **OIDC** | both — internal baseline + one OIDC integration covers every IdP |
| 2 · Users/Roles | internal store + Settings | sync from IdP groups / **SCIM** from IGA | build store+mapping; integrate SCIM; **do not rebuild IGA** |
| 3 · AuthZ (Cedar) | internal | internal | **build, always** — no external equivalent |

- **Fully build ✗** — enterprises mandate SSO/MFA via their IdP; rebuilding IGA (certification/SoD/attestation) is a multi-year detour.
- **Fully buy ✗** — standalone/air-gapped users have no Entra/Saviynt; and agent authZ has no product to buy.
- **Sequence:** internal IAM now → one **OIDC** integration (unlocks all IdPs) → **SCIM** (unlocks IGA vendors) → Cedar internal throughout.

---

## 10. Request flow (all planes)
```
1. AuthN → verify JWT (CE) or OIDC token (EE) → identity + claims
2. Roles → resolve roles/org  (EE: store or IdP groups;  CE: none)
3. Gate  → if edition==CE or !enforcing → Permit          ← the CE/EE line
          else Cedar.authorize(principal, action, resource, context)
4. Deny  → 403 (.../errors/authorization) + audit;  Permit → proceed
```
One decision point serves console CRUD, `/message` MCP calls, the editor's per-field lock, and the ClawdFlow runtime.

---

## 11. Phasing (each shippable; EE-gated from the start)

0. **Edition gate** — `GET /edition`, the "Enterprise Authorization" setting + license check, default **permissive** (CE). UI reads `/edition` to badge/hide EE. *(Wires the CE/EE line before any EE feature exists.)*
1. **Roles as data + Settings (Users/Roles)** — seed the 6 roles behind the EE flag.
2. **Cedar in shadow mode** — evaluate alongside permissive/Casbin, log divergence.
3. **Flip EE authZ to Cedar**; retire Casbin. Add ABAC (ownership, org-scope, published-immutability, policy-edit SoD).
4. **Field editability** (`bindableFields`/`lockedFields` + `editField` + `/authz/batch`) — unblocks the unified slide-in editor (locks only when EE-enforcing; fully editable in CE).
5. **Agent runtime capability** — `invokeTool`/`accessMemory` in the executor (behind shadow mode; skipped in CE).
6. **OIDC federation** → enterprise SSO. 7. **SCIM** → IGA provisioning.

---

## Appendix — Casbin vs Cedar
| | Casbin (today) | Cedar (proposed, EE) |
|---|---|---|
| Model | RBAC only | RBAC + ABAC + hierarchy |
| Attributes / ownership / org | ✗ | ✓ |
| Negative overrides (`forbid`) | ✗ | ✓ (SoD, immutability, edition gate) |
| Per-field / per-tool / context | ✗ | ✓ |
| Schema validation + static analysis | ✗ | ✓ |
| Fits A3T's planes | Plane 2 only | Planes 2 **and** 3 |
