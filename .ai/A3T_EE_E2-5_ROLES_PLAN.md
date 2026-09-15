# E2 #5 — Roles-as-Cedar / retire Casbin — Migration Plan

> Status: plan for review · 2026-07-27 · consumer of `A3T_EE_AESOP_RECONCILIATION.md` §4/§5.
> Reviewer decision requested before execution.

## 1. The finding that reframes this work

**Casbin is installed but inert.** `RbacEnforcer` (`rbac.rs`) is constructed at startup and
attached as an axum `Extension` (`main.rs`), and it loads a role/permission policy from LanceDB
via `rbac_adapter.rs`. But **`rbac.check()` is never called by any handler or middleware** —
verified: `RbacEnforcer` appears only in `main.rs` (install) and `rbac.rs` (def); there is no
`rbac_middleware`, `require_role`, or `.check(` call in the request path.

Consequences:
- **No HTTP endpoint is RBAC-gated today.** Any authenticated JWT holder can call any endpoint.
  The bootstrap seeds a `superadmin` role + wildcard permission into LanceDB, but since nothing
  consults it, those rows are functionally dead.
- The **real** authorization that exists is the **Cedar policy hook at the execution choke
  points** (tool/flow/llm steps) — the model in §4 — plus the **SoD guardrail** on cedar-resource
  writes (already a Cedar policy checking `principal.roles`, wired at the registry handler in #2b).
- So "roles-as-Cedar" is **already the live pattern** for the one role check that exists. Casbin
  is dead weight.

**This means "retire Casbin" is a near-zero-risk deletion, and the real deliverable is
*implementing* role-based authorization via Cedar — which Casbin never actually did.**

## 2. The role model

Current vocabulary (from `config/rbac_model.conf` + bootstrap):
- **Roles** (JWT `role`, single lowercase string): `superadmin`, `admin`, `operator`, `viewer`
  (a hierarchy superadmin > admin > operator > viewer). The plan doc's "6 roles" (SuperAdmin,
  SecurityAdmin, Admin-Infra, Developer, KnowledgeAdmin, Viewer) is aspirational — not seeded.
- **Resource types**: template, agent, canvas, resource, flow, memory, settings, user, org.
- **Actions**: create, read, update, delete, execute, deploy.

Decision needed (§6): keep the current 4 lowercase roles as the canonical Cedar vocabulary, or
adopt the 6-role model. Recommendation: **canonicalize the 4 real roles** (`superadmin`→`SuperAdmin`,
etc.) and add `SecurityAdmin` as an alias/grant of SuperAdmin for the SoD seam; defer the full
6-role taxonomy until a product need appears (avoid inventing roles nothing checks).

## 3. Migration phases

### Phase A — Delete Casbin (no behavior change) — ✅ DONE (2026-07-27)
Because `check()` is never called, removing it is inert:
- Drop the `Extension(rbac)` layer + `RbacEnforcer::new/permissive` calls in `main.rs`.
- Delete `rbac.rs`, `rbac_adapter.rs`, `config/rbac_model.conf`; remove `casbin = "2.20"` from
  `Cargo.toml`.
- Keep the **role/permission LanceDB tables** and the bootstrap `seed_role`/`seed_permission` —
  they are the source of truth for *role assignment* (which user has which role), independent of
  the (now-deleted) Casbin engine. `FLOWR_RBAC_PROVIDER` / `rbac.provider` setting: keep as a
  no-op reserved key or repurpose to `security.rbac = "cedar"`.
- **Verification:** CE+EE build; login + all existing flows behave identically (nothing consulted
  Casbin, so nothing changes). This is the rollback-safe checkpoint.

**Done:** removed the `Extension(rbac)` layer + `RbacEnforcer` init from `main.rs`, the `rbac`
field/`RbacConfig`/`FLOWR_RBAC_PROVIDER` from `config.rs`, the `rbac.provider` setting seed;
deleted `rbac.rs`, `rbac_adapter.rs`, `config/rbac_model.conf`; dropped `casbin` from `Cargo.toml`.
EE builds; nothing consulted Casbin so behavior is unchanged. Role *assignment* rows + bootstrap
`seed_role`/`seed_permission` kept as the source of truth.

### Phase B — Canonical roles on the Principal (removes the D5 bridge) — ✅ DONE (2026-07-27)
- In `auth.rs`, when building `Principal::human`, **canonicalize** the JWT `role` → a stable Cedar
  role name (`superadmin`→`SuperAdmin`, `admin`→`Admin`, `operator`→`Operator`, `viewer`→`Viewer`;
  map `superadmin`→ also grant `SecurityAdmin` for the SoD seam). One small pure function.
- Update the baseline SoD `unless` clause to check only the canonical names (drop the lowercase
  `superadmin` bridge). Update the flowr-aesop test to the canonical role.
- **Verification:** SoD live demo still works (superadmin writes cedar policy → 201; viewer → 403);
  flowr-aesop tests green.

**Done:** canonical role vocabulary now lives in **CE's `flowr-policy::roles`** (constants
`SuperAdmin`/`SecurityAdmin`/`Admin`/`Operator`/`Viewer` + `ALL` + `canonicalize()`), per the
"roles are CE infrastructure" steer. `auth.rs` builds the Principal with
`roles::canonicalize(claims.role)` (`superadmin`→`SuperAdmin`+`SecurityAdmin`); unknown roles pass
through. Baseline SoD `unless` now checks only canonical `SecurityAdmin`/`SuperAdmin` — the
lowercase-`superadmin` bridge (D5) is **gone**. Tests: flowr-policy roles (2), flowr-aesop SoD
uses `canonicalize` (22 total). *Live verify pending in `.5` deploy.*

**Live-verified (`.5`):** superadmin (`superadmin`→canonical) POST cedar resource = 201; viewer
(`Viewer`) = 403. Phase A neutral (CE/EE `/edition` unchanged, login OK).

### Phase C — Real RBAC as Cedar policies (the actual new capability) — 🔨 in progress (2026-07-27)

**Increment 1 DONE + DEPLOYED + DEMOED (`.6`, live):** superadmin (`→Admin` via hierarchy) POST
llm resource = **201**; viewer (`→Viewer`) = **403** (`cedar denied resource:create`, deny-by-default).
The first role-based rule the platform actually enforces.

> **Test hygiene lesson:** a Phase-B test wrote a cedar policy with body `permit(principal, action,
> resource);` — a **wildcard permit** that, once loaded from the registry, allowed *everything* and
> masked the Phase-C deny. Use *narrow* test policies and delete them after. Cleaned up + restarted.

- **Role hierarchy** expanded in `flowr-policy::roles::canonicalize` — `SuperAdmin ⊃ Admin ⊃
  Operator ⊃ Viewer` (a superadmin holds `Admin`), so Cedar policies stay flat membership checks.
- **Registry resource writes now require `Admin` (or `SecurityAdmin`)** — the baseline permit was
  `principal is User`; now gated on the role. Viewers/operators cannot create/update/delete
  registry resources; the cedar-type SoD forbid still narrows policy resources to InfoSec. This is
  **the first role-based rule the platform actually enforces** (Casbin never did). Test
  `resource_management_requires_admin_role`.

**Increment 2 DONE + DEPLOYED + DEMOED (`.7`, live):** closed a real **security gap** — the
agent-tool dispatch (`create_flow`, `create_agent`, `create_agent_template`, `delete_agent`)
**bypassed the policy hook entirely**; now routed through `policy_hook().check()` + audit. Added
an authoring-RBAC forbid: a human `User` needs `Admin` to author (reads stay open; agents/services
governed by their ceiling). Live: superadmin `create_flow` allowed, viewer denied
(`cedar denied mcp:call on Tool::create_flow`). Test `authoring_tools_require_admin_but_reads_are_open`.

**Increment scoping note:** there is **no settings-write endpoint** in the deployed server
(settings are startup-seeded/read-only), so that planned increment is moot. Remaining optional
increments (flow update/trigger mutations, user/org management) follow the same pattern; deferred.
Phase C's mechanism is proven across two surfaces (registry + authoring). Eventually expressed via
aesop annotations (E3).

**Gotcha (recurring):** after a rollout, `get pod ...Running` can return the *terminating* old
pod for a few seconds — a demo's `sleep 3` hit the `.6` pod and showed a stale (ungated) result.
Wait for old pods to fully terminate (`kubectl wait --for=delete` / `--field-selector`) before
asserting a policy result.
This is what Casbin never delivered: role-gated operations, enforced at the choke points via the
policy hook. Author baseline (or registry) Cedar policies for the operations that should be
role-restricted, each checking `principal.roles`:
- **Registry writes** — already done for `type=cedar` (SoD). Extend: only `Admin`+ may
  create/update/delete non-cedar resources (LLM/MCP/etc.); `Viewer` read-only.
- **Settings writes** — only `Admin`+ (or `SecurityAdmin` for `security.*`).
- **Flow/template deploy** — `Operator`+ may `execute_flow`; `Developer/Admin` may create/update
  flows; `Viewer` read-only.
- **User/org management** — `Admin`+.

Each becomes a Cedar `permit`/`forbid` on the relevant action/resource, evaluated where that
operation already crosses the policy hook (registry handler; and — new — a thin role check at the
HTTP handlers that mutate settings/users/flows, calling `policy_hook().check()` with an
`action`+`Resource` like the registry write path). No separate engine; one audit trail.

**Scope decision (§6):** Phase C is where the effort is. Recommend starting with registry +
settings (the security-relevant writes) and expanding per need, rather than gating every endpoint
at once.

## 4. What does NOT change
- The execution-layer model (§4) — steps/principal/ceiling/composition — is untouched; it already
  uses `principal.roles` where relevant (none yet beyond SoD).
- Role *assignment* storage (who has which role) stays in LanceDB + bootstrap.
- JWT issuance/validation (HS256 today) — that's E2 #4 (OIDC), separate.

## 5. Rollback & risk
- Phase A is behavior-neutral (inert code removed) — lowest risk; the natural first commit + deploy.
- Phase B touches `auth.rs` principal construction — small, covered by the SoD live demo.
- Phase C **adds** enforcement that doesn't exist today — the risk is *over*-restricting (a role
  can't do something it should). Mitigate: author permits generously first (deny-by-default only
  for the security-sensitive writes), demo each on vc-dev before widening, keep
  `security.policy_hook=allow_all` as the break-glass (flips the whole EE enforcement off).

## 6. Decisions requested before execution
1. **Role vocabulary** — canonicalize the existing 4 (recommended) vs adopt the 6-role taxonomy?
2. **Phase C scope** — start with registry+settings writes (recommended) vs gate all mutating
   endpoints now?
3. **`rbac.provider` setting** — retire it, or repurpose to `security.rbac`?

## 7. Test plan
- Phase A: build both editions; `/edition` unchanged; login + a flow run + the §4 demos unchanged.
- Phase B: flowr-aesop SoD tests (canonical roles); live SoD demo (superadmin 201 / viewer 403).
- Phase C: per policy — a live allow/deny pair per role×operation (e.g. `viewer` POST
  /registry/resources → 403; `admin` → 201), visible in `/security/audit`.
