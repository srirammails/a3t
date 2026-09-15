# A3T UI — Architecture

> Based on console core patterns. Key difference: sliding panel stack replaces fixed context sidebar.

## Core from Console (reuse)

| Pattern | Console Path | Reuse in A3T |
|---------|-------------|-------------|
| HTTP client | `core/connection/client.ts` | Same — fetch wrapper with retry, timeout, progress |
| Toast/messages | `core/status/messages.ts` | Same — success/error/warning/info with auto-dismiss |
| Progress bar | `core/status/progress.ts` | Same — indeterminate + percentage |
| StatusBar | `shell/StatusBar.svelte` | Same — 2px color bar + centered toast |
| Auth store | `stores/auth.ts` | Simplified — A3T may use API key auth or none for local |
| Page context | `stores/pageContext.svelte.ts` | Adapted — drives panel stack instead of context sidebar |
| Viewport | `shell/viewport.svelte.ts` | Same — breakpoints, responsive detection |
| Windmill service | `services/windmill.ts` | Replace with `services/clawdflow.ts` — MCP tool calls |

## Key Difference: Panel Stack

### Console layout (fixed context bar)

```
┌────────┬──────────────────────┬────────┐
│ Menu   │     Main Content     │Context │
│ Stack  │                      │ Stack  │
│ (left) │                      │(right) │
│  220px │      flex: 1         │ 280px  │
└────────┴──────────────────────┴────────┘
```

### A3T layout (sliding panel stack)

```
Click Templates → Click RTB Bidder → Click Agent instance → Click Logs

┌────────┬──────────┬──────────┬──────────┬──────────┐
│ Menu   │ Panel 0  │ Panel 1  │ Panel 2  │ Panel 3  │
│ Stack  │Templates │RTB Bidder│bidder-eu │  Logs    │
│        │  (list)  │ (detail) │(instance)│ (stream) │
│        │          │          │          │          │
│        │ cards    │ spec     │ metrics  │ terminal │
│        │          │ versions │ controls │          │
│        │          │ agents→  │ logs →   │          │
└────────┴──────────┴──────────┴──────────┴──────────┘

Close Panel 3 → Panel 2 expands
Close Panel 2 → Panel 1 expands
Back to Templates list → only Panel 0
```

Each click that would navigate to a detail opens a new panel sliding from right. Panels stack. User can close them right-to-left. The rightmost panel is always the widest (active focus). Earlier panels compress to a narrow strip showing just the title.

### Panel states

```
Active panel:    full width (flex: 1), all content visible
Compressed panel: 48px wide, rotated title text, click to expand
Hidden panel:    not rendered (scrolled off left)
```

Max visible panels: 4 on desktop, 2 on tablet, 1 on mobile (full screen).

## File Structure

```
flowr/ui/
├── package.json
├── svelte.config.js
├── vite.config.js
│
├── src/
│   ├── app.html
│   ├── app.css                     # Theme tokens (warm palette)
│   │
│   ├── lib/
│   │   ├── core/                   # ← FROM CONSOLE (adapted)
│   │   │   ├── connection/
│   │   │   │   └── client.ts       # HTTP client (fetch + retry + progress)
│   │   │   └── status/
│   │   │       ├── messages.ts     # Toast notifications
│   │   │       ├── progress.ts     # Progress bar
│   │   │       └── index.ts        # Re-exports
│   │   │
│   │   ├── services/               # Backend integration
│   │   │   └── a3t.ts              # ClawdFlow/FlowR API client
│   │   │                           # Replaces windmill.ts
│   │   │                           # Methods: agents, templates, resources,
│   │   │                           # triggers, inputs, flows, mcp()
│   │   │
│   │   ├── stores/                 # ← FROM CONSOLE (adapted)
│   │   │   ├── auth.ts             # Auth store (simplified)
│   │   │   ├── panels.svelte.ts    # Panel stack state (NEW — replaces pageContext)
│   │   │   └── app.svelte.ts       # Global app state (env, workspace)
│   │   │
│   │   ├── shell/                  # ← FROM CONSOLE (adapted)
│   │   │   ├── AppShell.svelte     # Layout: Menu + PanelStack
│   │   │   ├── MenuStack.svelte    # Left sidebar nav
│   │   │   ├── PanelStack.svelte   # Stacking panel container (NEW)
│   │   │   ├── Panel.svelte        # Single panel with header, close button (NEW)
│   │   │   ├── StatusBar.svelte    # Progress + toast (FROM CONSOLE)
│   │   │   ├── Topbar.svelte       # Logo, env switch, search, user
│   │   │   └── viewport.svelte.ts  # Responsive breakpoints (FROM CONSOLE)
│   │   │
│   │   ├── components/             # UI components
│   │   │   ├── cards/
│   │   │   │   ├── TemplateCard.svelte
│   │   │   │   ├── AgentCard.svelte
│   │   │   │   ├── ResourceCard.svelte
│   │   │   │   └── MetricCard.svelte
│   │   │   │
│   │   │   ├── agent/
│   │   │   │   ├── AgentHeader.svelte
│   │   │   │   ├── AgentMetrics.svelte
│   │   │   │   ├── AgentLogs.svelte
│   │   │   │   ├── AgentLifecycle.svelte
│   │   │   │   └── ProcessFacts.svelte
│   │   │   │
│   │   │   ├── canvas/
│   │   │   │   ├── TopologyGraph.svelte
│   │   │   │   ├── ReconcilePipeline.svelte
│   │   │   │   └── SpecPanel.svelte
│   │   │   │
│   │   │   ├── forms/
│   │   │   │   ├── SchemaForm.svelte
│   │   │   │   ├── YamlEditor.svelte
│   │   │   │   └── InputRequest.svelte
│   │   │   │
│   │   │   └── shared/
│   │   │       ├── Badge.svelte
│   │   │       ├── HealthDot.svelte
│   │   │       ├── Sparkline.svelte
│   │   │       ├── Modal.svelte
│   │   │       └── Tabs.svelte
│   │   │
│   │   └── types/
│   │       ├── agent.ts            # Agent, AgentMode, AgentStatus
│   │       ├── template.ts         # Template, TemplateSummary
│   │       ├── resource.ts         # Resource types
│   │       ├── trigger.ts          # Trigger, TriggerConfig
│   │       ├── input.ts            # InputRequest, InputResponse
│   │       └── canvas.ts           # Canvas CRD types
│   │
│   └── routes/
│       ├── +layout.svelte          # Root: auth guard + AppShell
│       └── +page.svelte            # Entry: redirects to /templates
│                                   # (routing is panel-based, not file-based)
│
└── static/
    └── favicon.ico
```

## Panel Stack Architecture

### Panel store (`panels.svelte.ts`)

```typescript
// Panel definition
interface PanelDef {
  id: string;              // unique panel ID
  type: string;            // 'templates' | 'template-detail' | 'agents' | etc.
  title: string;           // displayed in compressed state
  icon?: string;           // icon for compressed state
  props: Record<string, any>;  // data passed to the panel content
}

// Panel stack state
class PanelStack {
  panels = $state<PanelDef[]>([]);

  // Push a new panel (slide in from right)
  push(panel: PanelDef) { ... }

  // Pop the rightmost panel (slide out to right)
  pop() { ... }

  // Pop all panels above a given index
  popTo(index: number) { ... }

  // Replace current panel content
  replace(panel: PanelDef) { ... }

  // Clear all panels (back to root)
  clear() { ... }

  // Get the active (rightmost) panel
  get active(): PanelDef | null { ... }
}
```

### Panel rendering (`PanelStack.svelte`)

```svelte
<div class="panel-stack">
  {#each panels as panel, i}
    <Panel
      {panel}
      active={i === panels.length - 1}
      compressed={i < panels.length - 1}
      onClose={() => panelStack.popTo(i)}
    >
      <!-- Dynamic component based on panel.type -->
      {#if panel.type === 'templates'}
        <TemplatesList {panel} />
      {:else if panel.type === 'template-detail'}
        <TemplateDetail name={panel.props.name} />
      {:else if panel.type === 'agents'}
        <AgentsList {panel} />
      {:else if panel.type === 'agent-detail'}
        <AgentDetail id={panel.props.id} />
      {:else if panel.type === 'agent-logs'}
        <AgentLogs id={panel.props.id} />
      {/if}
    </Panel>
  {/each}
</div>
```

### Navigation flow

```
User clicks "Templates" in sidebar
  → panelStack.clear()
  → panelStack.push({ type: 'templates', title: 'Templates' })
  → Templates list renders in Panel 0

User clicks "RTB Bidder" card
  → panelStack.push({ type: 'template-detail', title: 'RTB Bidder', props: { name: 'rtb-bidder' } })
  → Panel 0 compresses to 48px strip
  → Template detail renders in Panel 1

User clicks "bidder-emea" agent link
  → panelStack.push({ type: 'agent-detail', title: 'bidder-emea', props: { id: 'bidder-emea' } })
  → Panels 0,1 compress
  → Agent detail renders in Panel 2

User clicks "Logs" button
  → panelStack.push({ type: 'agent-logs', title: 'Logs', props: { id: 'bidder-emea' } })
  → Panels 0,1,2 compress
  → Log viewer renders in Panel 3

User clicks X on Panel 3
  → panelStack.pop()
  → Panel 2 expands to active

User clicks Panel 0's compressed strip
  → panelStack.popTo(0)
  → Panels 1,2 removed
  → Panel 0 expands to full
```

### Sidebar menu clicks

```
Clicking a sidebar menu item always:
1. panelStack.clear()
2. panelStack.push({ type: menuItem.type, title: menuItem.label })

This ensures the sidebar always resets to a fresh single-panel view.
```

## A3T Service (`services/a3t.ts`)

Replaces console's `windmill.ts`. Direct REST calls instead of flow execution.

```typescript
class A3TService {
  private http: HttpClient;

  // Agents
  async listAgents(): Promise<Agent[]> { return this.http.get('/agents'); }
  async getAgent(id: string): Promise<Agent> { return this.http.get(`/agents/${id}`); }
  async getAgentHealth(id: string): Promise<AgentHealth> { return this.http.get(`/agents/${id}/health`); }
  async getAgentHistory(id: string, limit = 50): Promise<AgentTurn[]> { ... }
  async getAgentMetrics(id: string): Promise<AgentMetrics> { ... }
  async pauseAgent(id: string): Promise<void> { ... }
  async resumeAgent(id: string): Promise<void> { ... }

  // Templates
  async listTemplates(): Promise<TemplateSummary[]> { ... }
  async getTemplate(name: string): Promise<Template> { ... }
  async createTemplate(template: Template): Promise<void> { ... }
  async activateVersion(name: string, version: string): Promise<void> { ... }

  // Resources (via MCP)
  async listResources(): Promise<Resource[]> { return this.mcp('list_resources', {}); }
  async createResource(name: string, type: string, config: any): Promise<void> { ... }
  async deleteResource(name: string): Promise<void> { ... }

  // Triggers
  async listTriggers(): Promise<Trigger[]> { ... }
  async createTrigger(trigger: TriggerConfig): Promise<void> { ... }
  async attachTarget(triggerId: string, target: TriggerTarget): Promise<void> { ... }
  async fireWebhook(path: string, body: any): Promise<void> { ... }

  // Human inputs
  async listInputs(): Promise<InputRequest[]> { ... }
  async getInput(id: string): Promise<InputRequest> { ... }
  async submitInput(id: string, data: any): Promise<void> { ... }

  // Workflows (via MCP)
  async listFlows(): Promise<Flow[]> { return this.mcp('list_flows', {}); }
  async executeFlow(name: string, input: any): Promise<string> { ... }
  async getExecutionStatus(instanceId: string): Promise<ExecutionStatus> { ... }

  // Raw MCP call
  private async mcp(tool: string, args: any): Promise<any> {
    const r = await this.http.post('/message', {
      jsonrpc: '2.0', method: 'tools/call', id: 1,
      params: { name: tool, arguments: args }
    });
    return JSON.parse(r.result.content[0].text);
  }
}
```

## Sidebar Menu Structure

```typescript
const menuGroups = [
  {
    label: 'BUILD',
    items: [
      { type: 'templates', label: 'Templates', icon: 'grid', badge: null },
      { type: 'agents', label: 'Agents', icon: 'cpu', badge: 'pods' },
      { type: 'canvas', label: 'Canvas', icon: 'layers', badge: 'crd' },
    ]
  },
  {
    label: 'OPERATE',
    items: [
      { type: 'runtime', label: 'Runtime', icon: 'play', badge: null },
      { type: 'reconciliation', label: 'Reconciliation', icon: 'refresh', badge: null },
    ]
  },
  {
    label: 'AUGMENT',
    items: [
      { type: 'memory', label: 'Memory', icon: 'database', badge: null },
      { type: 'knowledge', label: 'Knowledge', icon: 'book', badge: '8' },
      { type: 'resources', label: 'Resources', icon: 'box', badge: '42' },
    ]
  },
  {
    label: 'WORKBENCH',
    items: [
      { type: 'aql-console', label: 'AQL Console', icon: 'terminal', badge: null },
      { type: 'audit-log', label: 'Audit log', icon: 'list', badge: null },
    ]
  },
];
```

## What to copy from Console vs write new

| Component | Source | Action |
|-----------|--------|--------|
| `core/connection/client.ts` | Console | Copy, remove Windmill-specific headers |
| `core/status/messages.ts` | Console | Copy as-is |
| `core/status/progress.ts` | Console | Copy as-is |
| `shell/StatusBar.svelte` | Console | Copy, update colors to A3T palette |
| `shell/viewport.svelte.ts` | Console | Copy as-is |
| `shell/MenuStack.svelte` | Console | Adapt — different menu groups, A3T styling |
| `shell/AppShell.svelte` | Console | Rewrite — PanelStack replaces ContextStack |
| `shell/PanelStack.svelte` | New | Write from scratch — stacking panel system |
| `shell/Panel.svelte` | New | Write from scratch — single panel with compress/expand |
| `stores/panels.svelte.ts` | New | Write from scratch — panel stack state management |
| `services/a3t.ts` | New | Write from scratch — replaces windmill.ts |
| All page components | New | Write from scratch |
| All card components | New | Write from scratch |

## Build & Embed

```
Dev:   cd flowr/ui && npm run dev (proxy to localhost:3002)
Build: npm run build → flowr/ui/build/
Embed: flowr-server includes build/ via rust-embed or include_dir!
Serve: GET /ui/* → static files from embedded build
```

No SvelteKit server-side rendering — pure SPA (adapter-static). All data fetched client-side from the same origin API.
