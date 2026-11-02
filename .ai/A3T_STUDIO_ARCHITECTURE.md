# A3T Studio — architecture (2 Nov 2026)

**Audience:** engineers and Claude Code working on the A3T UI and its VS Code extension.
**Status:** direction agreed 2 Nov 2026; open decisions in §15.
**Builds on:** `A3T_CANVAS_SPEC.md` (flow / runtime / canvas, connections §1a),
`FLOWR_CONSOLE_RUN_DEBUG_UI_SPEC.md` (run, debug, monitor, checkpoints; §14 is the
first Studio shell), `A3T_EDITOR_SPEC.md` (template/agent authoring; its rule
"the editor never hard-codes a resource type" applies here too),
`A3T_DASH_PROJECTS_ENVIRONMENTS.md` (projects and scope).

---

## 1. Why

The UI started as a set of section pages (Flows, Resources, Triggers, DB Console,
Runtime, Memory…), each with its own way to open, edit and test things. The value
is in giving developers an **editor**: one place where they see their flows and
resources, build and run, and watch what happens. Everything a developer does
fits in one studio. People who do not build things still need to see the
inventory and understand it, without an editor in the way.

So A3T has two UI **channels** over the same platform:

| Channel | For | Shape |
|---|---|---|
| **Console** | Everyone: business users, SRE, AppSec, and developers without write access | Section pages: banners, inventory lists, details, status. Read and light actions (pin, approve, pause). No editors. |
| **Studio** | Developers with write access to a project | An IDE: explorer, tabbed documents, panels. Author, compose, deploy, run, debug, query, attach. |

The Studio ships in two **hosts**: the web dash (`dash.a3t.io`, a section of the
Console) and a **VS Code extension**. Both run the same code (§4).

---

## 2. Who uses what

| Persona | Console | Studio |
|---|---|---|
| Business / non-developer | Inventory, chat, run history, dashboards | — |
| Developer, read-only on a project | Inventory of that project, runs, logs | Opens read-only (same views, no edit or deploy) |
| Developer, write | Inventory | Full Studio for that project |
| SRE | Runtimes, deployments, health | Canvas and runtime documents; attach to runs |
| AppSec | Policies, audit, approvals | Read-only |

Permissions come from the platform (roles + Cedar in EE). The Studio never decides
access itself: it asks the backend what the user may do with an item (§5.4) and
greys out what they may not. Every Console detail page offers **Open in Studio**
when the user may; every Studio document offers **Open in Console**.

### 2.1 Who works on which kind (user's guidelines, 2 Nov 2026)

Each kind serves these people; its views must answer their questions.
**No persona-specific UI for now** (user, 2 Nov): the views stay the shared ones —
a template is edited with the Template YAML form (the agent template editor: Agent,
LLM, Tools, Permissions, Memory, I/O, Test), as in the Templates pop-in. The table
says what a view must cover, not a separate screen. Governance (approvals, audit)
comes later.

| Kind | Who | What they need to see and decide |
|---|---|---|
| **Agent template** | Architects, lead developers | (a) the **hooks** — connect SecOps / governance / SRE flows or agents (any workflow or agent can be wired) and decide per hook whether agents may override it; (b) which **LLMs / gateways** and which **tools** are allowed or denied, and how an agent that adds tools gets approval for prod; (c) the **memory** allocated; (d) **resource limits** (max tokens, iterations, …); (e) which **agents and workflows inherit** the template |
| **Agent** | Developers delivering business value | (a) the **prompt**, and the **workflows** it adds — business or SRE sub-flows, connected agents; (b) **where and how exceptions are logged**; (c) **how memory is saved**; (d) its **tool access** and the **MCP servers** it may connect; (e) **where it deploys** — the template's canvas, and whether it may override to another runtime; (f) the **runtimes** it may use; (g) when testing: **debug, performance, checkpoints** |
| **Resources** | Infrastructure team | what is made available to agents and templates |
| **Resources → Workflows** | Developers | deterministic flows to connect to agents |
| **Resources → MCP** | SRE: how to connect; developers: what tools there are, each tool's **schema**, test them |
| **Resources → Databases** | Developers: what data agents have, what agents share (AQL), what my agents save; Security: logs, audit, incident triage |
| **Triggers** | Developers and infrastructure |

---

## 3. What a developer does in the Studio

**Author**
- See all flows and resources of the projects in scope in one Explorer.
- Create and edit flows: graph, code (YAML), form. Problems as they type.
- **AI**: ask the AI panel to generate or change a flow, or to draft a resource;
  the AI proposes, the user applies (diff), nothing deploys on its own.
- Create a resource (LLM, DB, MCP, secret, …) from a dynamic config form.

**Compose**
- Drag flows and resources from the Explorer onto a flow's **Compose** view and
  connect them into **one deployable workflow**. A dropped flow becomes a
  *sub-flow* (in-process, bundled) or a *flow resource* (a running instance, bound
  by I/O); a dropped resource becomes a resource binding — exactly the three
  connection kinds of the Canvas spec §1a. The result is flow YAML; there is no
  separate composition format.

**Deploy**
- Open a **Canvas**, attach a flow, set placement (runtime labels, isolation),
  lifecycle and health, **Deploy**. See which runtime it landed in.

**Operate**
- See runtimes, what is deployed on each, and their state.
- See a flow's runs: steps, **timeline**, journal, **checkpoints**; re-run, re-run
  in debug, re-run from a checkpoint (as specified in the run/debug spec).

**Develop against a live runtime**
- **Connect a dev runtime** (their own: local FlowR CE, a codespace, or a dev pool
  in the cluster) and make it the target for Run and Debug.
- **Attach** to a running flow on it: live steps, **logs**, checkpoints,
  breakpoints when it is a debug run.

**Use resources directly**
- **Database**: write a query in the resource's language (SQL, Redis, AQL), see
  results; history and saved queries.
- **LLM**: chat with it.
- **MCP server**: list its tools, call one with arguments.
- **Trigger**: fire it with a payload; see the run it starts.

**Memory**
- Write **AQL** and see an agent's or flow's memory by tier — **working →
  episodic → semantic → procedural**, and the graph — as ADB holds it.

---

## 4. Layers

```
 Hosts         Web: SvelteKit dash (Console + Studio)      VS Code extension (+ webviews)
               ─────────────────────── StudioHost interface ───────────────────────
 UI            studio-ui     Svelte 5 components: Workbench, Explorer, views, parts
 Model         studio-core   plain TypeScript: items, kinds, workspace, inventory,
                             capabilities, adapters, live streams
 Transport     a3t-client    REST + MCP + SSE, auth via a token provider
 Platform      FlowR CE/EE (MCP tools, /registry, /ops, runs + journal), ADB (AQL)
```

**Rules (enforced by `scripts/check-layers.mjs`, §12):**
1. Dependencies point **down** only. `studio-core` imports nothing from Svelte, the
   DOM, SvelteKit or VS Code. `studio-ui` makes no network calls; it calls core.
2. **Kind knowledge lives in one place**: a kind module in `studio-core/kinds/`
   (and its views in `studio-ui/views/`). No `if (kind === …)` anywhere else.
3. **Host differences live in one place**: an implementation of `StudioHost` —
   including which projects are in scope. `studio-ui` imports no Console stores,
   shell or host code.
5. The Console uses the Studio only through its entry point (`$lib/studio-ui`),
   never its internal components; it may use `studio-core` adapters directly.
4. One way to do each thing: the adapters in `studio-core/adapters/` are the only
   code that queries a database, chats with an LLM, fires a trigger, etc. Console
   pages use them too.

`studio-core` is plain TS (no runes) so the VS Code extension host (Node) can use
it for native views. Its stateful objects (the Workspace) mutate a state object
the host gives them and also offer `subscribe(fn) → unsubscribe`: the web passes
Svelte `$state`, so components follow it; the extension subscribes.

### 4.1 How the Studio is hosted

The Studio UI is Svelte, compiled to one JavaScript bundle; hosts never see
Svelte. They use the Studio's **entry point** (`studio-ui/index.ts`, §12.2) and
`studio-core`:

```
dash.dev (browser, one page)
  Console (Svelte pages)
    └─ <Studio host config> / mountStudio(el, …)   ← the entry point
         └─ Studio UI (Svelte components)
              └─ studio-core (TS) ── a3t-client ──► FlowR / ADB

VS Code
  Extension host (Node, TS)                 Webview (a small browser page)
    ├─ studio-core (TS)                       └─ mountStudio(el, webviewHost)
    │    ├─ Explorer tree (native)                 └─ Studio UI (same bundle)
    │    ├─ a3t: files, commands                        └─ studio-core (TS)
    │    └─ a3t-client ──► FlowR / ADB                       │
    └─ ◄──────────── postMessage (requests, results) ─────────┘
```

Same in both: `studio-core`, the Studio UI bundle, `mountStudio`. Per host: the
`StudioHost` (§12.1) — the web host calls the backend directly; the webview host
forwards requests to the extension, which holds the token and makes every call.

**Not WebAssembly for the Studio.** The Studio is UI and API calls, native to
JavaScript; its libraries (xyflow, CodeMirror) are JavaScript; both hosts run JS.
WebAssembly fits **engine logic shared with FlowR** instead — the engine's own
flow validation, jq expression evaluation and AQL parsing compiled from Rust and
called from a `studio-core` adapter, so the editor and the engine agree exactly.
Optional, when validation (step 7) and the AQL editor (step 6) need it.

---

## 5. studio-core

### 5.1 Items and addresses

Everything the Studio shows is an **item** with one address, used as the route in
the web, the URI in VS Code, the tab id and the drag payload:

```
a3t://<project>/<kind>/<name>[#<fragment>]
  a3t://acme-support/flow/triage
  a3t://acme-support/resource/orders-db
  a3t://acme-support/trigger/intake            (id-keyed kinds resolve name → id)
  a3t://acme-support/run/019a…                 (runs, deployments: by id)
  a3t://acme-support/memory/agent:triage       (a memory scope)
```

```ts
type ItemRef = { project: string; kind: string; name: string; id?: string };
type Item = ItemRef & { type?: string; data?: unknown; status?: string };
```

### 5.2 Kinds

A **kind** is a module that knows one sort of item. Adding a resource type or a new
item sort means adding a kind; nothing else changes.

```ts
interface ItemKind {
  id: string;                         // 'flow', 'database', 'llm', 'mcp', 'trigger', …
  match(item: Item): boolean;         // e.g. database: resource whose type is postgres|mysql|redis|lancedb|qdrant|adb
  label: string; icon: string;
  folder?: string;                    // Explorer folder it lists under
  capabilities: Capability[];         // what can be done with it (§5.4)
  load(ref: ItemRef): Promise<Doc>;   // fetch what its document needs
  save?(doc: Doc): Promise<Result>;   // persist edits (config, YAML)
  remove?(ref: ItemRef): Promise<Result>;
  rename?(ref: ItemRef, to: string): Promise<Result>;
  connect?(ref: ItemRef): Promise<ConnectionStatus>;   // reachable? shown on the tab
  views: { center: string; right?: string[]; bottom?: string[] };   // view ids, rendered by studio-ui
}
```

Kinds at the start (the folder each lists under is from the Explorer today):

| Kind | Items | Center view | Right | Bottom |
|---|---|---|---|---|
| `flow` | workflows | Flow (graph / code / form), Compose | Form, AI | Runs, Problems |
| `database` | postgres, mysql, redis, lancedb, qdrant, adb | Query | Config | Results, History, Saved |
| `llm` | llm, openai, anthropic, ollama, groq | Chat | Config | — |
| `mcp` | mcp | Tools (list → call) | Config | Result |
| `trigger` | triggers | Fire (payload, target flows) | Config | Runs of the target flow |
| `canvas` | canvases | Canvas (attach flow, placement, lifecycle, health, Deploy) | Config | Deployments |
| `runtime` | runtimes | Runtime (deployments, activity, metrics) | — | Logs |
| `run` | a run | Run (steps, timeline, journal, checkpoints) | Step inspector | Logs |
| `memory` | a memory scope (agent / flow) | Memory (tiers + AQL) | — | Results |
| `template` | agent templates | Template editor (AgentTemplateEditor) | AI | — |
| `resource` | any other resource (fallback) | Config | — | — |

### 5.3 Workspace

The open documents and layout of one Studio:

- open docs (by address), the active one, dirty state, per-doc UI state (e.g. the
  run dialog of a flow);
- `open(ref)`, `close`, `rename`, `remove`, `newDoc(kind, project)`;
- persisted through `host.storage` (web: `localStorage`; VS Code: workspace state);
- it mutates a state object it is given: the web passes a Svelte `$state` object
  (`studio-ui/workspace.svelte.ts`), so views follow it; the VS Code extension
  passes a plain object and calls `subscribe`;
- the layout model (regions, sizes, minimized, maximized) — today's
  `layout.svelte.ts`, moved behind the same store interface.

### 5.4 Capabilities and permissions

`Capability = 'edit' | 'run' | 'debug' | 'query' | 'chat' | 'tools' | 'fire' |
'deploy' | 'attach' | 'connect' | 'memory' | 'delete' | 'rename'`.

What the user may do with an item = the kind's capabilities ∩ what the backend
allows for that item and user (`access` answer per project, cached). Views ask
`can(ref, 'edit')`; menus and buttons follow. Read-only users get the same Studio
with edit, deploy, fire and delete removed.

### 5.5 Inventory

The Explorer's data, built from the kinds: projects in scope → folders → items,
with counts, filter and remembered folder state. A Studio is configured with a
**scope**: which folders, which one is pinned on top and open (DB Console pins
Databases; a memory studio pins agents and flows). Loading never throws: failures
are listed, as the Explorer does today.

### 5.6 Adapters

One module per capability, each the only implementation in the codebase:

| Adapter | Does | Backend |
|---|---|---|
| `flows` | list, get YAML, save/deploy, delete, rename (copy + delete) | MCP `list_flows`/`get_flow`/`create_flow`, registry `DELETE` |
| `config` | read and write a resource / trigger / canvas config; masks secrets | `/registry/resources`, `update_trigger`, `update_canvas` |
| `query` | run a statement on a database resource → `{ columns, rows, raw, ms }` | `call_sql`, `call_redis`, `query_resource`, `adbSql` |
| `chat` | send messages to an LLM resource | `a3t-test-resource` (llm) |
| `mcp` | list tools, call a tool | `a3t-test-resource` (mcp, mcp_call) |
| `trigger` | fire with a payload → run id | `/webhook/<path>`; other types: `start_run` / `send_event` |
| `runs` | list, get, journal, start, debug, re-run, cancel; **subscribe** | `list_runs`, `get_run`, `get_run_journal`, … ; SSE `run:<id>` |
| `ops` | runtimes, deployments, placement, deploy, pause, terminate; **logs** | `/ops/*` |
| `canvas` | get, attach flow, update, deploy | `canvasApi`, `/ops/deploy` |
| `memory` | AQL over a scope; tiers; promotions | ADB (`call:adb` / `query_resource`) |
| `ai` | generate / edit a flow or resource draft from a prompt + context | the designer chat flow |

Adapters return `Result<T> = { ok: true, data } | { ok: false, error }` and never
throw, the convention of `services/api.ts` today.

### 5.7 Live streams

`runs.subscribe(runId)` and `ops.logs(deploymentId, { follow })` return
async-iterable streams over SSE. Views render them; the host decides nothing.

---

## 6. studio-ui

### 6.1 Workbench

`Workbench` = `StudioShell` (left / center / right / bottom regions, as today) +
Explorer + document tabs. The active document's kind supplies the center view and
the right and bottom panels; panels that the active kind does not have are hidden,
and the region remembers its last panel per kind.

A Studio is configuration, not a page — a `StudioConfig` (app id, home kind,
kinds "+ New" creates, layout defaults, the home banner):

```svelte
<Studio host={webHost} config={STUDIOS.flows} />
<Studio host={webHost} config={STUDIOS.db} />       <!-- step 3 -->
```

With nothing open the Studio shows its **home**: the banner, "+ New …", and the
items opened recently (kept by the Workspace through the host's storage).

On the web the Console's menu has **one Studio entry** (2 Nov 2026: "Studio",
`/studio`; `/flow-editor` still resolves) — the former Flow Editor, which opens
every kind. The Query Console has no menu entry: databases open in the Studio;
`/db-console` still opens the Studio pinned to Databases for old links. In VS Code
there is one Studio; scopes become Explorer views.

### 6.2 Views (center) and panels

**The center is for acting on an item; its settings are in the Form panel**
(right region, decided 2 Nov 2026). Flows: graph / code, Run, Debug, Deploy in
the center; Form and AI on the right. Databases: the query editor; LLMs: chat;
MCP servers: tools; triggers: Fire (their target flow's runs below). Items with
no action of their own yet (other resources, canvases) show their config as code
in the center and as a form in the Form panel — two editors of one document;
templates show their spec. Every kind's Form tab has the id `form`, so it stays
the open tab as you move between kinds; a `PanelDef`'s `component` picks what
renders it (the flow form for flows, `config` for the rest). Save (Ctrl/⌘+S) sits
wherever the document is edited; its state is on the document.


Views are registered by id and receive `{ doc, workspace, host }`:
`FlowView` (graph / code / form; today's FlowDocument + FlowEditorPane),
`ComposeView`, `QueryView`, `ChatView`, `ToolsView`, `FireView`, `CanvasView`,
`RuntimeView`, `RunView` (today's RunView), `MemoryView`, `ConfigView`,
`TemplateView` (AgentTemplateEditor).

**Feedback round, 2 Nov 2026** (decided by the user, built on `a3t-studio`):
- *Explorer* per project: Agent templates, Agents, **Resources → Workflows** (flows
  live under Resources), Databases, LLM, MCP Servers, Secrets, Functions, Scripts,
  Variables, then Triggers, Canvases, Runtimes. Every folder that can create has `+`.
- *Open vs edit:* click opens an item's own view; **double-click or Edit opens it
  as code** in the center (`views.code`, `doc.ui.view = 'code'`), "← back" returns.
  The Form panel is rebuilt per item.
- *New everything:* "+ New ▾" and folder `+` create any kind; a new item opens as
  code (named, and typed for resources) with its Form, and is created on Save.
  Starting configs per resource type: `studio-core/resourceTypes.ts`.
- *AI* is a right-region tab for every kind (the document's text).
- *Context menu:* the Studio's own everywhere (tabs, panels, the center, text
  fields); no browser menu inside the Studio.
- *Panels move:* drag a panel's tab to another region or onto the editor tabs (it
  becomes a tab there); closing it there sends it home, minimized. Kept per Studio
  (`layout.placement`).
- *Triggers* open a **designer** (adopted from the Triggers page): identity,
  channel, the target flows as a graph (drop flows from its list or the Explorer),
  and a Test area that fires it; runs below.
- **Agents and agent templates** (2 Nov, AGENT_TEMPLATE_SPEC §2): a template is a
  workflow; each field of its agent step's `config` has a visibility —
  `required` (the template's value is a placeholder for testing; every agent sets
  its own), `optional` (a default agents may override), `locked` (final, shown
  read-only), `hidden` (final, not shown) — kept in the template's own metadata,
  `document.metadata.a3t.fields`; no marker = `optional`. The template opens as
  its workflow (graph / code, saved with a confirm when agents use it) with Form,
  **Agent fields** (set visibilities) and AI; an agent opens as its template's
  workflow with its values, and its **Form** is generated from the template.
  Model: `studio-core/agent/model.ts`. Agents are created (+ on Agents: pick the
  template in the Form; Save creates it and its chat endpoint) and deleted in the
  Studio, and tested there: **Test** (chat through its endpoint) and **Runs** (its
  chat flow's runs — debug, timeline, checkpoints; shared by the agents that flow
  serves until runs can be filtered by agent).

### 6.3 Shared parts

- **ConfigForm** — built from the config's own keys and values, no per-type field
  lists: strings, numbers, booleans; nested objects as sections; arrays as editable
  lists. Masked secrets are read-only with **Replace**, so saving never writes the
  mask back. Read-only mode for Console and read-only users.
- **ResultTable** — columns, rows, raw JSON, copy, column hiding for secrets.
- **ChatView**, **CodeEditor** (CodeMirror: YAML, SQL dialects, AQL, JSON),
  **RunPanel** (runs list + RunView, resizable), **Problems**.
- Icons, tooltips, context menus, splitters, dialogs: one implementation each
  (today some are copied per component).

### 6.4 Drag and drop

Drag payload: MIME `application/x-a3t-item` carrying the address. Sources: the
Explorer (web: HTML5 drag; VS Code: `TreeDragAndDropController` into the webview).
Targets: ComposeView (flow → sub-flow or flow resource, resource → binding),
CanvasView (flow → attach), QueryView (table → `SELECT`), chat (item → context).

---

## 7. Compose: one deployable workflow

The Compose view of a flow is a graph of its connections, not a second format:

| Drop | Becomes | In the flow YAML |
|---|---|---|
| a flow, as *sub-flow* | in-process call, bundled with the flow | `run: workflow` task |
| a flow, as *flow resource* | bind to a running instance | resource reference to the flow's endpoint/queue |
| a resource | binding | `call: llm` / `call: mcp` / `call: adb` … `with: resource:` |

Edges are data flow (output → input). Saving writes the flow YAML; the Canvas
(§8) deploys it, dependency-deploying flow resources (Canvas spec §1a).
Validation is the SWF schema plus the slot contracts of the editor spec.

---

## 8. Deploy and operate

- **Canvas** documents: attach a flow (drop or pick), placement, lifecycle,
  health, **Deploy**. Shows where it landed and its deployments.
- **Runtime** documents: runtime state, deployments, live activity, metrics; Logs
  in the bottom region.
- **Run** documents and the Runs panel: steps, timeline, journal, checkpoints,
  re-run / debug / from checkpoint — the run/debug spec, unchanged.
- From any of them: open the flow, the canvas, the runtime, the run (addresses).

---

## 9. Dev runtimes and attach

A **dev runtime** is a FlowR runtime that belongs to one developer
(labels `env=dev`, `owner=<user>`): local FlowR CE, a codespace, or a pod in a dev
pool. The Studio:

1. **Connect** — register it (endpoint + token, or pick one from `ops.runtimes`
   that the user owns). It becomes a target in the Run/Debug dialog and the
   default target while connected. A status dot shows it in the status bar.
2. **Run / Debug on it** — the flow's current text, not only the deployed one, so
   a developer iterates without deploying to a shared runtime.
3. **Attach** to a running flow — on a dev runtime or any runtime the user may
   see: live steps (journal subscription), **logs** (follow), checkpoints;
   breakpoints and stepping only for the user's own debug runs.

Backend needed: a per-deployment log stream (§14), runtime registration for
user-owned runtimes, running unsaved flow text on a dev runtime.

---

## 10. Memory

`MemoryView` for a scope (an agent or a flow):

- tier tabs — **working, episodic, semantic, procedural**, and **graph** — each a
  list with time, key, value, source run;
- an **AQL** editor prefilled for the scope and tier, results in ResultTable;
- per entry: the run that wrote it (open as a run), promotions between tiers.

Scopes appear in the Explorer under each agent and flow (*Memory*). Today's
Memory page renders mock data; it is replaced by this view on the `memory`
adapter. Backend needed: listing memory scopes and tiers per agent/flow from ADB.

---

## 11. AI in the Studio

The AI panel (today's DesignerChat) gets the active document and selection as
context and can: generate a flow, change the open flow, draft a resource config,
explain a run or a failed step. Its output is always a **proposal** (a diff on the
document, or a prefilled ConfigForm); applying and deploying stay user actions.
In VS Code the same panel is a webview; the extension may also expose the A3T MCP
tools to the editor's own assistant.

---

## 12. Code layout

Today everything is in one SvelteKit app (`a3t/ui`). The seams are introduced
in-tree first and extracted to packages when the VS Code work starts:

```
a3t/ui/src/lib/
  client/          → a3t-client     (from services/api.ts)
  studio-core/     → studio-core    (items, kinds/, workspace, inventory, adapters/, streams)
  studio-ui/       → studio-ui      (Workbench, Explorer, views/, parts/)
  host/web.ts      web StudioHost
  components/pages, shell/   Console (section pages) — use studio-core adapters and studio-ui parts
a3t/vscode/        the extension (later): host/vscode.ts, tree views, file system provider, webviews
```

Later: an npm workspace with `packages/a3t-client`, `packages/studio-core`,
`packages/studio-ui`; `a3t/ui` and `a3t/vscode` depend on them. The layer
rules of §4 are checked by `a3t/ui/scripts/check-layers.mjs`, which the Vite build
runs first (a violation fails the build; also `npm run check:layers`).

### 12.1 StudioHost

```ts
interface StudioHost {
  storage: { get(key: string): unknown; set(key: string, v: unknown): void };
  confirm(msg: string): Promise<boolean>;
  clipboard(text: string): Promise<void>;
  openConsole(section: string, title: string): void;     // a Console section page
  scope: {                                                // which projects the Studio shows
    projects(): string[];                                 // web: the project switcher; VS Code: settings
    current(): string | null;                             // where "+ New" creates
    onChange(fn: () => void): () => void;
  };
  // added with the steps that need them:
  token(): Promise<string | null>;              // web: session; VS Code: SecretStorage
  open(ref: ItemRef, opts?: { side?: boolean }): void;   // web: route; VS Code: editor tab
  openExternal(url: string): void;
  notify(msg: string, level: 'info' | 'warn' | 'error'): void;
  theme(): 'light' | 'dark';
}
```

Today: `studio-core/host.ts` has the first five; the web host is `host/web.svelte.ts`.

### 12.2 Entry point

`studio-ui/index.ts` is all a host imports from studio-ui:

```ts
export { default as Studio } from './Studio.svelte';   // Svelte hosts (the Console)
export function mountStudio(target: Element, opts: { host: StudioHost; config: StudioConfig; open?: Item[] })
  : { open(item: Item): void; dispose(): void };       // any host (VS Code webview)
export { STUDIOS, type StudioConfig } from './studios';
```

`<Studio>` takes one optional snippet, `lead` (controls the host puts at the start
of the tab bar — the Console's submenu toggle); `mountStudio` has no snippets.

---

## 13. VS Code extension

| VS Code surface | A3T | Built from |
|---|---|---|
| Activity bar view: **A3T Explorer** (TreeView) | projects → flows, resources, triggers, canvases, runtimes, memory | `studio-core` inventory (native tree, no webview) |
| File system provider, scheme `a3t:` | flows as YAML files (`a3t://project/flow/name.yaml`): native editing, search, diff, git | `flows` adapter |
| Custom editor ("Open With → A3T Flow") | graph / form / compose for a flow | `studio-ui` views in a webview |
| Webview panels | Query, Chat, Tools, Fire, Canvas, Runtime, Run, Memory | `studio-ui` views |
| Commands | Sign in, Connect runtime, Run, Debug, Deploy, Attach, New flow, New resource | adapters |
| Status bar | signed-in user, project, connected dev runtime | workspace |
| Output channel | logs of an attached run / deployment | `ops.logs` |
| Debugger (later) | breakpoints in the YAML gutter, step, variables = step context | a Debug Adapter over `runs.debug` |

Webviews talk to the extension over `postMessage` RPC; the extension owns the
token and makes every backend call (webviews never hold credentials). Saving an
`a3t:` file saves a draft; **Deploy** is an explicit command, as on the web.

---

## 14. Backend gaps

| Need | For | Today |
|---|---|---|
| Per-item permission answer (`can user X do Y on item Z`) | §5.4 | roles client-side; Cedar decides at the call |
| Deployment / run log stream (SSE, follow) | §8, §9, VS Code output | no log endpoint; LogsPage shows the decision log |
| Run subscription (journal + status over SSE) | live runs, attach | polling (`list_runs`, `get_run`) |
| User-owned dev runtime registration | §9 | runtimes come from the platform only |
| Run unsaved flow text on a dev runtime | §9 | runs use the deployed flow |
| Multi-turn LLM chat (`messages`) | Chat view | `a3t-test-resource` sends one `prompt` |
| Enforce template field visibility on save (reject an agent value for a locked or hidden field) | Agents | only the Studio enforces it today (`a3t-agents-update` stores what it is given) |
| Queries on LanceDB resources | Query view | `query_resource` answers "does not support 'lancedb' resources yet" |
| Memory scopes and tiers per agent/flow | §10 | Memory page is mock |
| Rename for flows / resources | Explorer | copy + delete (runs stay under the old name) |
| Fire for cron / event triggers | Fire view | webhook only |

---


### Lifecycle terms (decided 2 Nov 2026)

One vocabulary everywhere in the Studio (buttons, menus, messages):

| Term | Meaning | Backend |
|---|---|---|
| **Save** (Ctrl/⌘+S) | keep the edits as a draft, locally (the host's storage); survives a reload, nobody else sees it; a new item comes back still new | none — `Workspace.saveDraft` / `discardDraft` |
| **Sync ▾** (like Run ▾: Sync / Versions / Merge / Push) | Sync checks the registry: unchanged since the edits started → push; newer → the **split view** (yours, editable, beside the registry's; versions under each). Merge and Push are enabled only then | `studio-ui/sync.ts`, `Workspace.remoteChanged` |
| **Push** (Sync ▾) | send the item to the registry, over its copy; **Push as a new version** bumps `document.version` (patch / minor / major) and keeps the old one beside it | `create_flow`, template / agent / resource / trigger updates — `ItemKind.push` |
| **Merge** (Sync ▾) | three-way merge (`studio-core/merge.ts`) by lines first (untouched lines stay byte for byte), by structure (keys, named tasks / cases / extensions) when lines clash and the structure does not. Clean → done, Sync pushes the result. Conflicts → the split view to finish by hand: arrows take the registry's side of a change, or edit; conflicts named by key path and line | client-side; `@codemirror/merge` 6.12.2 for the split view |
| **Deploy** | put a pushed flow on a runtime, where its canvas places it | `POST /canvas/flows/<ns>/<name>/deploy` |
| **Deploy / Run / Debug** (Run ▾) | Deploy as above; execute the pushed flow | runs API |
| **Stop** | stop a run | `cancel_run` (and the debugger's stop) |
| **Terminate** | kill a runtime: its runs stop, it is removed, what was deployed there is rescheduled | `POST /ops/runtimes/<n>/terminate` |
| **Hibernate** | snapshot a runtime (gVisor checkpoint) and stop it; Resume restores it | `POST /ops/runtimes/<n>` hibernate |

Run, Debug and Deploy use the registry's copy, so they wait for a Push. A draft whose registry copy changed since it was made says so ("registry changed"); pushing replaces the registry copy. Drafts are per browser until the backend has a draft store (gap: server-side drafts, shared with the team).

Versions: the **Versions** panel (bottom, beside Runs) lists the versions the registry keeps (flows: `document.version`, side by side, one active) and the editor's text: Load, Make active (roll back / forward), Push as a new version (patch / minor / major), and **Compare** any two (or one and the editor) in the split view, read-only.

**Registry versioning the Studio needs (backend gap, adb-integration):** today pushing a flow's same `document.version` overwrites it silently, and templates, agents and resources have no versions. Needed for every item kind: (1) a **revision** per push (monotonic, with who and when); (2) **read by revision** and a history, so Merge fetches the exact base instead of the copy kept in the draft; (3) **conditional push** — push carries the revision it started from, the server answers 409 with the current revision when it moved on (Push as overwrite sends none). Until then the Studio checks the registry just before pushing, which leaves a small race window.

## 15. Plan

Each step ships on dash.dev and is checked there before the next.

| # | Step | Result |
|---|---|---|
| 1 | `studio-core` seams in-tree: items, kinds, workspace, inventory; `Workbench`; Flow Editor moved onto it | no visible change; lint rules on — **done 2 Nov 2026** (branch `a3t-studio`) |
| 1b | Entry point (`<Studio>`, `mountStudio`), Studio home with recent items, project scope from the host, Console imports only the entry point | **done 2 Nov 2026** |
| 2 | `config` adapter + ConfigForm; every non-flow item opens as a config form | JSON previews gone — **done 2 Nov 2026**: resources, canvases and triggers edit and save (Form or JSON, Ctrl/⌘+S); masked values stay as stored unless replaced; templates read-only until their own view (saving one changes its agents) |
| 3 | `query` adapter + `database` kind; DB Console = Workbench pinned to Databases (history, saved queries kept) | DB Console in the Studio — **done 2 Nov 2026**: Query view (config in the Form panel), Results / History / Saved panels, ADB AQL or SQL; the previous console one release behind "Classic console"; its AI pane was a placeholder and is not carried over |
| 4 | `chat`, `mcp`, `trigger` kinds | act on every resource — **done 2 Nov 2026**: LLMs (Chat), MCP servers (Tools: list, form from the tool's input schema, call), triggers (Fire: webhook POST, CloudEvent of the trigger's `source` type, or start the workflow targets; Runs of the target below); config in the Form panel |
| 5 | `canvas`, `runtime`, `run` kinds; Deploy from the Studio | operate from the Studio — **done 2 Nov 2026**: canvas (attach a flow by pick or drag from the Explorer, placement dry-run, live deployment, Deploy / Pause / Resume / Restart / Undeploy, + on Canvases creates one), runtime (state, host, deployments with their actions; Runtimes folder); runs stay in the Runs panels (a separate run tab was tried and removed, 2 Nov); logs wait for the platform's log stream (§14) |
| 6 | `memory` kind + AQL view (needs ADB scopes) | real memory view |
| 7 | Compose view + drag and drop | one deployable workflow — **first cut 2 Nov 2026**: a **Steps** palette (CNCF tasks and platform calls, beside the Explorer; drag or click) and Explorer items drop on a flow — a resource becomes the step that uses it (`with._resource`), a flow a `run: workflow` sub-flow, an agent a call to its chat flow, a trigger is wired to start the flow; each new step is added at the end of `do` and opens in the Form. 2 Nov (later): the designer has its own palette rail on the graph (Control — switch, fork, for, try… — Calls, Run, Events, State, and a Library of the flow's `use.functions`). The code view is **schema-aware**: a CNCF DSL outline beside the text (`studio-core/flow/schema.ts`, words and examples from the blog's reference tree, copied in `flow/swfDocs.ts`) lists every key the DSL allows at each place — set keys in full, the rest greyed and one click from added; lists end in +; problems red/amber in the outline, on the text's lines and in Problems. FlowR variants (do/while inside for, try as a map, http url, bare JSON Schema input) are accepted with a warning. Agents draw as a square block with name and version on a label above: opened on their own (templates) with input, output, each `use.errors` entry and each extension as nodes; inside a flow as one unit with in/out/err/ext connectors. Next: drop between steps, connections drawn on the graph, schema-aware completion in the text |
| 8 | Dev runtime connect + attach + logs (needs backend §14) | develop against a live runtime |
| 9 | Extract packages; VS Code extension: tree, `a3t:` files, webviews, commands | Studio in VS Code |
| 10 | Debug adapter for VS Code | native breakpoints |
| — | Console pages switch to the adapters and parts as each lands; old copies deleted | one implementation |

### Decisions

Resolved (2 Nov 2026): two channels, Console and Studio; one Studio codebase for
web and VS Code; the layer and kind architecture above.

Open — defaults in brackets, used unless decided otherwise:
1. LLM chat: ~~single-turn until the flow takes `messages`?~~ The Studio sends the conversation; `a3t-test-resource` passes it as `messages` in the repo (2 Nov) — live once that flow is redeployed.
2. ~~Firing a cron trigger / an event trigger?~~ Done as proposed (step 4).
3. Old DB Console: removed in step 3, or kept behind a link until the new one is tried? [kept one release]
4. Read-only developers: Studio read-only (§2), or Console only? [Studio read-only]
5. ~~One web Studio with scopes, or separate studios?~~ One Studio in the menu (2 Nov); `/db-console` kept for old links.
6. Package extraction: at step 9, or earlier? [step 9]
