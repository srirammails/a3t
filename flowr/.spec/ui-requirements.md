---
project: A3T UI
version: "0.1"
framework: SvelteKit
description: >
  A3T web console for the FlowR agent platform. Four primary views matching the
  information architecture: Templates (BUILD), Agents (BUILD/OPERATE), Canvas (BUILD/OPERATE),
  and Resources (AUGMENT). Plus operational views for Runtime, Reconciliation, Memory,
  Knowledge, and a developer workbench (AQL Console, Audit Log).
---

# A3T UI — Requirements & Architecture

## Information Architecture (from screenshots)

```
┌─────────────────────────────────────────────┐
│  Top Bar                                    │
│  Logo · Env switcher · Workspace · Search · │
│  Notifications · User                       │
├──────────┬──────────────────────────────────┤
│ Sidebar  │  Main Content                    │
│          │                                  │
│ BUILD    │  (varies by page)                │
│ Templates│                                  │
│ Agents   │                                  │
│ Canvas   │                                  │
│          │                                  │
│ OPERATE  │                                  │
│ Runtime  │                                  │
│ Reconcil.│                                  │
│          │                                  │
│ AUGMENT  │                                  │
│ Memory   │                                  │
│ Knowledge│                                  │
│ Resources│                                  │
│          │                                  │
│ WORKBENCH│                                  │
│ AQL Cons.│                                  │
│ Audit log│                                  │
│          │                                  │
│ VERTICALS│                                  │
│ (dynamic)│                                  │
│          │                                  │
│ Footer   │                                  │
│ version  │                                  │
│ env info │                                  │
└──────────┴──────────────────────────────────┘
```

## Pages

### 1. Templates Page (BUILD)

**Purpose**: Browse, create, and manage agent templates — the typed archetypes that define capability surfaces.

**Layout**:
- Hero banner: "Start with a template. Give it memory, knowledge, and tools. Wire it into the Canvas."
- Action buttons: "+ Create from template", "Import template", "New template"
- Template cards grouped by vertical/category (Real-time decisioning, etc.)
- Each card shows: icon (2-letter), name, description, tags, usage count, version

**Data sources**:
- GET /templates → list
- Template tags, verticals from template metadata

**Interactions**:
- Click card → template detail (spec editor, versions, agents using it)
- Create from template → parameter form → creates agent
- Import → YAML upload or paste

### 2. Agents Page (BUILD → OPERATE)

**Purpose**: View all running agent instances with their runtime status, metrics, and controls.

**Layout — List view**:
- Agent cards/rows with: name, kind badge, template ref, pod/node info
- Status indicator (Running/Paused/Error), uptime, replicas

**Layout — Detail view** (Agent → process):
- Header: agent name, kind badge, template ref, pod, node
- Info banner: "This agent is a running process..."
- Process facts sidebar: kind, name, pod, node, ip, image, pid, qos
- Instantiated from: template name, version, upgrade available
- Deployed in canvas: canvas ref with link
- Resources bound: list of bound resources

- Lifecycle controls: Pause all instances, Step one, Restart gracefully, Stop & evict
- Metrics row: Inflight, Completed/1h, p99 latency, Errors/1h (with sparklines)
- Container logs: live streaming, filterable, timestamped, level-colored

**Data sources**:
- GET /agents → list
- GET /agents/:id → detail
- GET /agents/:id/health → metrics
- GET /agents/:id/history → conversation/execution log
- GET /agents/:id/metrics → detailed metrics
- SSE /agents/:id/events → live log stream (future)

**Interactions**:
- Pause/Resume/Restart/Stop buttons → POST /agents/:id/pause|resume
- kubectl describe → shows raw K8s describe output
- Logs → live streaming with filter
- Exec shell → terminal (future)
- Attach debugger → debug session (future)

### 3. Canvas Page (BUILD → OPERATE)

**Purpose**: Declarative multi-agent orchestration view — topology graph, reconciliation status, spec/status/diff.

**Layout**:
- Header: canvas name, kind badge (AgentCanvas), generation, observedGeneration, drift indicator
- Action buttons: kubectl explain, Plan, Rollback, Apply
- Info banner: "This canvas is a Kubernetes Custom Resource..."
- Reconciliation pipeline: spec parsed → diff computed → scaling → triggers bound → outputs bound
- Controller status: "Controller reconciling — drift on 1 of 3 agents"
- Pause controller, View events buttons

- Topology graph (main content):
  - Trigger nodes (left): HTTPTrigger with path, port, rps
  - Agent nodes (center): template, version, pod count, p99, drift status
  - Output nodes (right): sink references
  - Edges: traffic flow with percentage labels (canary routing)
  - Zoom controls

- Right panel (tabs): spec, status · drift, diff
  - Spec tab: YAML with syntax highlighting
  - Drift tab: shows differences between desired and actual
  - Diff tab: highlights changes (red/green)

**Data sources**:
- kubectl get agentcanvas → list (future, via API proxy)
- Agent CR status → from controller
- GET /agents → runtime agents for live metrics

**Interactions**:
- Apply → kubectl apply (or POST to controller API)
- Rollback → restore previous spec
- Plan → dry-run diff preview
- Click agent node → navigate to Agent detail
- Click trigger → show trigger config

### 4. Resources Page (AUGMENT)

**Purpose**: Manage typed resource bindings — MCP servers, databases, LLMs, ERPs, variables, flows, scripts.

**Layout**:
- Header stats: total resources, healthy count, warning/error count, consuming agent count
- Description: "Agents consume, Resources supply."
- Resource cards grouped by type:
  - MCP servers: card with name, URL, health dot, usage count, tool dots (d·s·p)
  - Databases: ADB, Neo4j, PostgreSQL, Qdrant with type labels
  - LLMs: provider, model, usage
  - ERPs: integration endpoints
  - Variables: key-value config
  - Flows: registered workflows
  - Scripts: Lua/JS scripts

- Sub-navigation in sidebar: MCP servers (11), Databases (6), LLMs (5), ERPs (2), Variables (8), Flows (6), Scripts (4)
- Action buttons: Import from repo, Browse registry, + New resource
- "+ Register MCP" card for adding new MCP servers

**Data sources**:
- GET /resources (MCP list_resources) → all resources
- Health check per resource (future)

**Interactions**:
- Click card → resource detail (config, bound agents, health)
- + New resource → type selector → config form
- Delete resource
- Register MCP → URL input → discovery

### 5. Runtime Page (OPERATE)

**Purpose**: Overview of all running processes, resource usage, execution queue.

**Data**: GET /agents, execution queue depth, system metrics

### 6. Reconciliation Page (OPERATE)

**Purpose**: Controller reconciliation events, drift history, corrective actions taken.

**Data**: Controller events stream

### 7. Memory Page (AUGMENT)

**Purpose**: Browse agent memory (ClawDB) — working, episodic, procedural, semantic, tools.

**Data**: ClawDB AQL queries

### 8. Knowledge Page (AUGMENT)

**Purpose**: Knowledge graphs, semantic indexes, shared knowledge bases.

### 9. AQL Console (WORKBENCH)

**Purpose**: Interactive AQL query editor — run queries against ClawDB.

**Layout**: Split pane — editor on left, results on right.

### 10. Audit Log (WORKBENCH)

**Purpose**: System-wide event log — who did what, when.

### 11. Human Input Page (OPERATE)

**Purpose**: List pending human input requests, render forms, submit responses.

**Data sources**:
- GET /inputs → pending requests
- GET /inputs/:id → request detail with schema
- POST /inputs/:id → submit response

## Svelte Architecture

```
flowr/ui/
├── package.json
├── svelte.config.js
├── vite.config.js
├── src/
│   ├── app.html               # Shell HTML
│   ├── app.css                 # Global styles (warm palette from screenshots)
│   │
│   ├── lib/
│   │   ├── api.js              # API client (fetch + error handling)
│   │   ├── sse.js              # SSE client for live events
│   │   ├── stores/
│   │   │   ├── agents.js       # Agent list + detail stores
│   │   │   ├── templates.js    # Template stores
│   │   │   ├── resources.js    # Resource stores
│   │   │   ├── triggers.js     # Trigger stores
│   │   │   ├── inputs.js       # Pending human inputs
│   │   │   └── ui.js           # UI state (sidebar, theme, env)
│   │   └── utils/
│   │       ├── format.js       # Duration, byte, number formatting
│   │       └── yaml.js         # YAML highlight/parse
│   │
│   ├── routes/
│   │   ├── +layout.svelte      # App shell (sidebar + topbar)
│   │   ├── +page.svelte        # Dashboard / redirect to templates
│   │   │
│   │   ├── templates/
│   │   │   ├── +page.svelte    # Template library (cards by vertical)
│   │   │   └── [name]/
│   │   │       └── +page.svelte # Template detail (spec, versions)
│   │   │
│   │   ├── agents/
│   │   │   ├── +page.svelte    # Agent list
│   │   │   └── [id]/
│   │   │       └── +page.svelte # Agent detail (metrics, logs, controls)
│   │   │
│   │   ├── canvas/
│   │   │   ├── +page.svelte    # Canvas list
│   │   │   └── [name]/
│   │   │       └── +page.svelte # Canvas topology + reconciliation
│   │   │
│   │   ├── resources/
│   │   │   ├── +page.svelte    # Resource grid by type
│   │   │   └── [name]/
│   │   │       └── +page.svelte # Resource detail
│   │   │
│   │   ├── runtime/
│   │   │   └── +page.svelte    # Runtime overview
│   │   │
│   │   ├── reconciliation/
│   │   │   └── +page.svelte    # Reconciliation events
│   │   │
│   │   ├── memory/
│   │   │   └── +page.svelte    # ClawDB browser
│   │   │
│   │   ├── knowledge/
│   │   │   └── +page.svelte    # Knowledge bases
│   │   │
│   │   ├── inputs/
│   │   │   ├── +page.svelte    # Pending human inputs
│   │   │   └── [id]/
│   │   │       └── +page.svelte # Input form (schema → form)
│   │   │
│   │   └── workbench/
│   │       ├── aql/
│   │       │   └── +page.svelte # AQL Console
│   │       └── audit/
│   │           └── +page.svelte # Audit log
│   │
│   └── components/
│       ├── layout/
│       │   ├── Sidebar.svelte          # Left navigation
│       │   ├── Topbar.svelte           # Top bar with env, search, user
│       │   ├── Breadcrumbs.svelte      # Path breadcrumbs
│       │   └── EnvSwitcher.svelte      # dev/staging/prod toggle
│       │
│       ├── cards/
│       │   ├── TemplateCard.svelte     # Template card with icon, tags
│       │   ├── AgentCard.svelte        # Agent status card
│       │   ├── ResourceCard.svelte     # Resource card with health dot
│       │   ├── CanvasCard.svelte       # Canvas summary card
│       │   └── MetricCard.svelte       # Stat card with sparkline
│       │
│       ├── agent/
│       │   ├── AgentHeader.svelte      # Name, kind badge, template ref
│       │   ├── AgentMetrics.svelte     # Inflight, completed, latency, errors
│       │   ├── AgentLogs.svelte        # Live container logs
│       │   ├── AgentLifecycle.svelte   # Pause/Step/Restart/Stop controls
│       │   ├── ProcessFacts.svelte     # Right sidebar process info
│       │   └── ConversationHistory.svelte # Turn-by-turn history
│       │
│       ├── canvas/
│       │   ├── TopologyGraph.svelte    # DAG graph (triggers → agents → outputs)
│       │   ├── ReconcilePipeline.svelte # Step pipeline indicator
│       │   ├── SpecPanel.svelte        # YAML viewer with tabs (spec/status/diff)
│       │   ├── DriftBadge.svelte       # Drift indicator
│       │   └── ControllerEvents.svelte # Live reconciliation events
│       │
│       ├── resources/
│       │   ├── ResourceGrid.svelte     # Cards grouped by type
│       │   ├── McpServerCard.svelte    # MCP-specific card with tool dots
│       │   ├── DatabaseCard.svelte     # DB card with type label
│       │   └── ResourceStats.svelte    # Header stats bar
│       │
│       ├── forms/
│       │   ├── SchemaForm.svelte       # JSON Schema → form renderer
│       │   ├── YamlEditor.svelte       # Syntax-highlighted YAML editor
│       │   ├── JsonEditor.svelte       # JSON editor
│       │   └── InputRequest.svelte     # Human-in-the-loop form
│       │
│       └── shared/
│           ├── Badge.svelte            # Colored badge (kind, mode, status)
│           ├── HealthDot.svelte        # Green/yellow/red dot
│           ├── Sparkline.svelte        # Tiny inline chart
│           ├── SearchBar.svelte        # Global search
│           ├── Modal.svelte            # Modal dialog
│           ├── Tabs.svelte             # Tab switcher
│           └── EmptyState.svelte       # "No items" placeholder
│
├── static/
│   └── favicon.ico
│
└── build/                              # Vite output → embedded in flowr binary
    └── (index.html, assets/)
```

## Design System (from screenshots)

### Colors
```
Primary accent:     #e67e22 (warm orange — buttons, active states)
Background:         #faf9f7 (warm off-white)
Sidebar bg:         #1a1a2e (dark navy)
Card bg:            #ffffff
Text primary:       #333333
Text secondary:     #888888
Text muted:         #bbbbbb
Success:            #27ae60 (green dots, running badge)
Warning:            #f39c12 (yellow/amber)
Error/Danger:       #e74c3c (red)
Drift:              #e67e22 (orange border on drift nodes)
Info banner agent:  #e8f4fd (light blue)
Info banner canvas: #fef3e0 (light amber)
Code/mono bg:       #1e1e2e (dark terminal)
```

### Typography
```
Headings:     System sans-serif, 600 weight
Body:         System sans-serif, 400 weight
Mono/code:    'JetBrains Mono' or system monospace
Badge text:   11px, uppercase, 600 weight
Stat values:  32px, 700 weight
```

### Component patterns
- **Cards**: White bg, subtle shadow, 8px radius, hover lift
- **Badges**: Rounded pill, colored bg, uppercase text
- **Health dots**: 8px circles, green/yellow/red
- **Sparklines**: Inline SVG, 60px wide, single stroke
- **Tool dots**: Small colored circles (d·s·p representing discovery, schema, performance)
- **Info banners**: Full-width, colored left border, icon, description text
- **Sidebar nav**: Dark bg, sections with uppercase labels, active item with left border accent
- **Top tabs**: Template | Agent | Canvas with underline active state

## Build & Embed Pipeline

```bash
# 1. Install (one-time)
cd flowr/ui && npm install

# 2. Dev mode (hot reload against running FlowR API)
npm run dev -- --proxy http://localhost:3002

# 3. Production build
npm run build
# Output: flowr/ui/build/

# 4. Embed in Rust binary
# flowr-server/src/main.rs uses rust-embed or include_dir!
# to serve flowr/ui/build/* at /ui/*

# 5. Build flowr binary
cd flowr && cargo build --release -p flowr-server
# Single binary with UI embedded
```

## API Mapping

| UI Page | API Endpoints |
|---------|--------------|
| Templates | GET /templates, POST /templates, GET /templates/:name |
| Agents list | GET /agents |
| Agent detail | GET /agents/:id, /health, /history, /metrics |
| Canvas | GET agent CRs via controller API (future) |
| Resources | POST /message (MCP list_resources, create_resource, etc.) |
| Triggers | GET /triggers, POST /triggers, attach/detach |
| Human inputs | GET /inputs, POST /inputs/:id |
| Runtime | GET /agents (aggregated), GET /health |
| AQL Console | POST to ClawDB /query endpoint |
| Workflows | POST /message (MCP execute_flow, get_execution_status) |

## Phase 1 Scope (MVP)

Build these pages first:
1. **Templates** — list with cards, create via YAML
2. **Agents** — list + detail with metrics, history, lifecycle controls
3. **Resources** — grid by type, create/delete
4. **Human Inputs** — pending requests, form renderer, submit
5. **Layout** — sidebar, topbar, routing

Defer to Phase 2:
- Canvas topology graph
- Reconciliation view
- Memory browser
- Knowledge page
- AQL Console
- Audit log
- Live log streaming (SSE)
- Env switcher (dev/staging/prod)
