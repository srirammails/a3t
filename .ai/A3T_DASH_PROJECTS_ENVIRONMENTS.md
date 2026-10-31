# Dash: projects, environments and filters (proposal, 31 Oct 2026)

Goal: make dash.dev.a3t.io work like a developer's delivery tool. You pick a
**project** and an **environment**, and every page shows only what belongs there. A
flow, template or agent is built once, then promoted dev → staging → prod. Platform
internals stay out of view unless you ask for them.

## 1. What the UI does today

- **No context.** Every page lists everything the user can reach.
  - `appItems.ts:38` hard-codes `NAMESPACES = ['default', 'a3t', 'system']` and calls
    `list_flows` / `list_resources` once per namespace.
  - `ResourcesList.svelte:381` and `AgentTemplateEditor.svelte:125` keep their own
    copies of that list.
  - A namespace someone creates (`team-a`) never shows up. The platform's own flows
    (`a3t-*`, `system/*`) sit next to user work.
- **Namespace is a group heading, not a filter.** `groupByNamespace` sorts `a3t`
  first. The only narrowing is the text box, which also matches the namespace.
- **Environment doesn't exist as data.**
  - The Cedar baseline already has prod rules: `flow:deploy` and pause/undeploy when
    `resource.label_tier != "prod"`, plus an extra check for prod. But nothing ever sets
    `label_tier`, so those rules never apply.
  - The canvas spec (§3–4) has `runtimeSelector.tier: prod` and runtime labels. Only
    runtimes carry labels (`canvas.rs:487`). Flows have `tags`, resources have
    `tags` (none set on vc-dev), and neither has labels.
- **The backend can't filter.** There's no `list_namespaces` in FlowR CE.
  `flowr-api` has `/namespaces`, but A3T doesn't use that server. List calls take one
  namespace and no label selector, so filtering would have to happen in the browser
  after fetching everything.

## 2. The model

```
Org (tenant)
 └─ Project  ──────────── = a namespace (1:1); owns definitions and members
     ├─ definitions        flows · templates · agents · tool flows       (environment-free, versioned)
     └─ Environment        dev · staging · prod  (per project, ordered)
         ├─ runtimes        labelled  a3t.io/env=<env>
         ├─ deployments     canvases pin a definition version to this env
         ├─ resource values  per-env config/secret for each logical resource
         ├─ triggers         enabled per env
         ├─ executions/logs  stamped with the env they ran in
         └─ memory           ADB namespace <project>/<agent>, scope env:<env>
Platform (a3t, system)     hidden; superadmin "Show platform" toggle
```

Decisions:

1. **Project = namespace, 1:1.** Namespaces already drive registry keys, Cedar
   resources, ADB `NAMESPACE`, canvas placement and the k8s namespace. A separate
   project concept would need mapping in every one of those. A **Project record** adds
   what a namespace lacks: display name, members and roles, the environment list, and
   the default runtime per environment.
2. **Definitions are environment-free; environment is a deployment concern.** A flow
   or template isn't copied per environment. It has versions, and a **deployment** (a
   canvas) pins one version to one environment. This matches how developers think
   ("prod runs v1.4, dev runs main") and avoids three diverging copies.
3. **Resources are logical names with per-environment values.**
   - A flow names `orders-db` or `llm`. What that means differs per environment (dev
     DB vs prod DB, a cheap model vs the real one).
   - The resource keeps one name, with an `envs: {dev: {...}, prod: {...}}` overlay
     on `config` and secret refs.
   - The executing runtime's environment picks the values. Flows never mention an
     environment.
4. **The runtime's environment label is the source of truth at run time.**
   - A runtime pool is labelled `a3t.io/env`.
   - An execution takes its environment from the runtime it ran on.
   - FlowR fills `label_tier` from that label, so the existing Cedar prod rules start
     to apply without changes.
5. **Platform namespaces are hidden, not special-cased in every page.**
   - `a3t` and `system` are marked `platform: true` in the namespace list.
   - Lists leave them out unless the superadmin turns on "Show platform".

## 3. Dash UI

### 3.1 The context bar

```
┌──────────────────────────────────────────────────────────────────────────────┐
│ A3T  │ ▣ orders-app ▾ │ ● dev ▾ │                       ⌘K   ◎ admin ▾      │
└──────────────────────────────────────────────────────────────────────────────┘
          project switcher   env switcher (colour per env: dev green, staging amber, prod red)
```

- **Project ▾** lists the projects you're a member of, plus "All projects" for
  cross-project views. "Show platform" appears at the bottom for superadmins only.
- **Env ▾** lists the project's environments in order. Prod is red, and when prod is
  selected the whole top bar gets a thin red edge, so you can't act on prod by
  accident.
- **Context lives in the URL**, as `/p/<project>/<env>/<app>/…`. A link reproduces
  exactly what you saw, and the browser's back button works. The last context is
  remembered per user (server-side preference) and in `localStorage` for a quick
  start.
- **⌘K** searches inside the current context first, then shows "in other projects…"
  hits.

### 3.2 What each page shows under a context

| Page | Filtered by project | Filtered by env | Developer view |
|---|---|---|---|
| Flows / Templates / Agents | yes | no (definitions) | Version list; a **"Deployed in" chip per env** (`dev v1.5 · prod v1.4`) |
| Canvases (deployments) | yes | yes | This env's deployments; **Promote →** to the next env |
| Resources | yes | values | One row per logical resource; the value shown is this env's, with "not set in prod" warnings |
| Triggers | yes | yes | Enabled state per env |
| Executions / Logs | yes | yes | Defaults to this env; a failed-runs filter |
| Runtimes | yes | yes | The env's pools and their health |
| Chat | yes | yes | Talks to the agent deployed in this env (dev chat doesn't touch prod memory) |
| Memory | yes | yes | ADB browser scoped to `<project>/*`, scope `env:<env>` |
| Policies / Security / Users | org-wide | — | Plus a project's **Members** tab |

Each list also gets a small filter row with chips for type, status, owner ("mine") and
labels. The free-text box stays.

### 3.3 The promotion view

This is the delivery view. The project home shows which version of each definition is
deployed in each environment:

```
orders-app                          dev        staging     prod
─────────────────────────────────── ────────── ─────────── ───────────
flow   orders-intake                v1.5 ●     v1.5 ●      v1.4 ●   [Promote v1.5 → prod]
agent  support-bot (chat-assistant) v2.1 ●     v2.0 ●      v2.0 ●
flow   nightly-reconcile            v0.3 ⚠     —           —        (resource orders-db: no prod value)
```

- **Promote** creates or updates the next env's canvas pinned to that version. It runs
  the policy check first: prod needs an admin or an approval, which reuses
  `approval.rs`.
- **Diff** compares two envs: definition version, resource values (keys only, never
  secret values) and trigger state.
- The `⚠` checks catch problems before they surface at run time: a missing resource
  value in the target env, a runtime that doesn't match, a tool the policy doesn't
  allow.

### 3.4 UI implementation

- **A `context` store** (`stores/context.svelte.ts`) holds `{project, env,
  showPlatform}`, read from the route. It's the one place a page gets its scope.
- **`appItems.ts` takes the context** and asks the server for exactly that scope. The
  three hard-coded namespace lists go away.
- **The route becomes `routes/p/[project]/[env]/[...path]`.** The old `/[...path]`
  routes redirect to the last-used context.
- **Prod styling** comes from a single attribute, `data-env="prod"` on the shell, plus
  CSS tokens. No per-page logic is needed.

## 4. Backend (FlowR CE / EE)

| Change | Where | Why |
|---|---|---|
| `list_projects` (namespaces + Project records, with `platform` flag and your role) | MCP tool + `GET /projects` | Fills the switcher; the hard-coded list goes away |
| **Project record**: `type_ref: project` registry resource, one per namespace: `display_name`, `members: [{principal, role}]`, `environments: [{name, order, runtime_selector, requires_approval}]` | registry | Membership and environments without a new storage table |
| **`labels` map** on `WorkflowMetadata` and `Resource` (k8s style; keep `tags`) | `flowr-types` | Filter chips; `a3t.io/env` and `a3t.io/team` |
| List filters: `namespaces: []`, `labels: {k: v}`, `env`, `owner`, `status`, `limit/after` on `list_flows`, `list_resources`, `list_triggers`, `list_executions`, `list_canvases` | `lib.rs` tool handlers | Filter on the server; one call per page, not per namespace |
| Resource `envs` overlay, resolved by the executing runtime's `a3t.io/env` | resource resolution in the executor | One flow, per-env values |
| Execution record carries `env` and `runtime` | `flowr-ce-storage` | Filters on Executions and Logs |
| Fill `label_tier` (and `env`) on Cedar resources from the runtime or canvas | `flowr-aesop` entity building | The existing prod rules start working |
| Project-scoped roles: Cedar `principal in Project::"<ns>"::Role::"developer"` | `baseline.cedar` + schema | "Developer in orders-app, viewer elsewhere" |
| ADB: an agent's memory namespace is `<project>/<agent>`; FlowR passes `IN SCOPE "env:<env>"` | `agent_memory.rs` | Dev testing doesn't pollute prod memory. Fits the scope plan in the ADB flows doc §3.4. |

Existing data needs no rewrite:

- `default` becomes a project called "default" with a single `dev` environment.
- `a3t` and `system` are marked platform.
- Existing runtimes without an `a3t.io/env` label count as `dev`.
- A resource without `envs` uses its `config` in every environment.

## 5. Order of work

| Phase | Work | Visible result |
|---|---|---|
| **D0: filters now** (UI + one tool) | `list_projects` from the registry's distinct namespaces; context store and project switcher; platform namespaces hidden; the three hard-coded lists removed; URL context | Pick a project, see only it. Platform flows gone from view. |
| **D1: projects** | Project record, members and roles, project-scoped Cedar; "mine"/type/status chips; server-side list filters | Teams see their projects; lists are fast |
| **D2: environments** | Runtime `a3t.io/env`; execution env; env switcher; resource `envs` overlay; `label_tier` filled | Dev and prod side by side; prod policies apply |
| **D3: delivery** | Promotion matrix, Promote with approval, env diff, pre-flight `⚠` checks | Ship a version from dev to prod from the dash |
| **D4: memory per env** | `<project>/<agent>` namespaces, `env:` scope; Memory page scoped | Chat in dev doesn't touch prod memory |

D0 is mostly UI. It fits alongside A0 of the ADB flows doc, and both clean up what
lives in `a3t`/`system`.

## 6. Decisions needed

1. **Project = namespace (1:1)**, or projects that contain several namespaces?
   Proposed: 1:1, because everything already keys on namespace.
2. **Environments**: a fixed `dev / staging / prod` for every project, or a list per
   project? Proposed: per project, defaulting to those three.
3. **Agents**: is an agent a definition (environment-free, deployed per env like a
   flow), or created per environment? Proposed: a definition, with a deployment per
   env, so chat in dev uses the dev deployment.
4. **Resource values per environment**: an `envs` overlay on one resource (proposed),
   or separate resources per env bound by label?
5. **Platform visibility**: superadmin only, or also org admins?
