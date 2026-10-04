<script lang="ts">
  import { messages } from '$lib/core/status/messages';
  import {
    RESOURCE_CATEGORIES as CATEGORIES, CAT_COLORS, categorize, catMeta,
  } from '$lib/core/resources/categories';
  import { resources as resourcesMcp, flows as flowsMcp, mcpCall, execFlow } from '$lib/services/api';
  import DocsPanel from '$lib/shell/DocsPanel.svelte';
  import SlidePanel from '$lib/components/shared/SlidePanel.svelte';
  import PageBanner from '$lib/components/shared/PageBanner.svelte';
  import AppHome from '$lib/shell/AppHome.svelte';
  import EntityCard from '$lib/components/shared/EntityCard.svelte';
  import PropertiesTree from '$lib/components/shared/PropertiesTree.svelte';
  import DeleteConfirm from '$lib/components/shared/DeleteConfirm.svelte';
  import FlowEditorPane from '$lib/components/editor/FlowEditorPane.svelte';
  import { navigateTo, getActivePanel } from '$lib/stores/panels.svelte.ts';
  import { can, currentUser } from '$lib/stores/auth';
  import { onMount } from 'svelte';
  import * as YAML from 'yaml';

  const canCreate = can('create', 'resource');
  const canUpdate = can('update', 'resource');

  let docsOpen = $state(false);
  let loading = $state(true);
  let error = $state('');

  // Raw data
  interface Resource {
    name: string; namespace: string; type: string; category: string;
    description: string; created_at: string; created_by: string;
    tags: string[]; healthy: boolean; config?: Record<string, any>;
  }

  let allResources: Resource[] = $state([]);

  // Category mapping






  // Active category filter
  // The shell's context pane lists these fixed categories and navigates to
  // /resources/<category>, so the URL selection seeds the filter. Falls back to
  // 'all' for an unknown key rather than filtering everything away.
  let activeCategory = $state(
    (() => {
      const sel = getActivePanel()?.props?.selectedId;
      return sel && CATEGORIES.some(c => c.key === sel) ? sel : 'all';
    })(),
  );

  // Filtered resources
  let filteredResources = $derived(
    activeCategory === 'all' ? allResources : allResources.filter(r => r.category === activeCategory)
  );

  // Count per category
  let categoryCounts = $derived.by(() => {
    const counts: Record<string, number> = { all: allResources.length };
    for (const r of allResources) {
      counts[r.category] = (counts[r.category] ?? 0) + 1;
    }
    return counts;
  });

  // ── Detail panel ──
  let selected = $state<Resource | null>(null);
  let panelTab = $state('overview');
  let testResult = $state('');
  let testing = $state(false);

  // ── LLM test state ──
  let llmInput = $state('');
  let llmMessages: { role: string; content: string }[] = $state([]);
  // ── Config (endpoint etc.) editing ──
  let llmCfg = $state<Record<string, any>>({});
  let cfgSaving = $state(false);
  let cfgMsg = $state('');

  // Secret resources selectable as an LLM's api_key_ref (namespace/name).
  let secretRefs = $derived(
    allResources
      .filter((r) => r.category === 'credentials')
      .map((r) => ({ ref: `${r.namespace}/${r.name}`, name: r.name }))
  );

  // Switch the LLM key between a secret reference (recommended) and an inline key.
  function setKeyMode(mode: string) {
    if (mode === 'secret') {
      delete llmCfg.api_key;
      if (llmCfg.api_key_ref == null) llmCfg.api_key_ref = '';
    } else {
      delete llmCfg.api_key_ref;
      if (llmCfg.api_key == null) llmCfg.api_key = '';
    }
    llmCfg = { ...llmCfg };
  }

  async function loadConfig(r: Resource) {
    cfgMsg = '';
    let cfg = r.config ?? {};
    if (!cfg || Object.keys(cfg).length === 0) {
      const full = await resourcesMcp.get(r.name, r.namespace);
      cfg = (full?.data?.config ?? full?.data ?? {}) as Record<string, any>;
    }
    if (selected && selected.name === r.name && selected.namespace === r.namespace) {
      selected.config = cfg;
      llmCfg = { ...cfg };
    }
  }

  async function saveConfig() {
    if (!selected) return;
    cfgSaving = true; cfgMsg = '';
    const cfg: Record<string, any> = { ...llmCfg };
    if (cfg.max_tokens != null && cfg.max_tokens !== '') cfg.max_tokens = Number(cfg.max_tokens);
    if (cfg.temperature != null && cfg.temperature !== '') cfg.temperature = Number(cfg.temperature);
    const res = await resourcesMcp.update(selected.namespace, selected.name, cfg);
    cfgSaving = false;
    if (res.ok) {
      cfgMsg = 'Saved';
      selected.config = cfg;
      allResources = allResources.map((r) => (r.namespace === selected!.namespace && r.name === selected!.name ? { ...r, config: cfg } : r));
    } else {
      cfgMsg = res.error || `Save failed (HTTP ${res.status})`;
    }
  }

  // ── DB test state ──
  let dbQuery = $state('');
  let dbOperation = $state('list_tables');

  // ── MCP test state ──
  let mcpTools: { name: string; description: string }[] = $state([]);
  let mcpSelectedTool = $state('');
  let mcpToolArgs = $state('{}');
  let mcpToolResult = $state('');

  // ── Flow test state ──
  let flowInput = $state('{}');

  // ── Secret test state ──
  let secretConfig: Record<string, any> | null = $state(null);
  let secretRevealed = $state(false);

  // ── Create form ──
  let showCreate = $state(false);
  let createForCategory = $state('');
  let createTab = $state('details');
  let newResource = $state({ name: '', namespace: 'a3t', type: '', config: '{}' });
  let creating = $state(false);

  const createTreeNodes = [
    { id: 'details', label: 'Details', icon: '<rect x="2" y="2" width="12" height="12" rx="2"/><path d="M5 6h6M5 9h4"/>' },
    { id: 'config', label: 'Configuration', icon: '<path d="M8 1v4M8 11v4M1 8h4M11 8h4"/><circle cx="8" cy="8" r="3"/>' },
    { id: 'review', label: 'Review & Create', icon: '<path d="M4 8l3 3 5-6"/><circle cx="8" cy="8" r="7" fill="none"/>' },
  ];

  // ── Delete ──
  let deleteTarget = $state<{ name: string; namespace: string } | null>(null);

  const detailTreeNodes = [
    { id: 'overview', label: 'Overview', icon: '<rect x="2" y="2" width="12" height="12" rx="2"/><path d="M5 6h6M5 9h4"/>' },
    { id: 'config', label: 'Configuration', icon: '<path d="M8 1v4M8 11v4M1 8h4M11 8h4"/><circle cx="8" cy="8" r="3"/>' },
    { id: 'test', label: 'Test / Execute', icon: '<polygon points="4,2 13,8 4,14"/>' },
    { id: 'logs', label: 'Execution Logs', icon: '<rect x="2" y="1" width="12" height="14" rx="1.5"/><path d="M5 5h6M5 8h6M5 11h3"/>' },
  ];

  // Flows: editable Overview form first, then Definition (graph + YAML)
  const flowDetailTreeNodes = [
    { id: 'overview', label: 'Overview', icon: '<rect x="2" y="2" width="12" height="12" rx="2"/><path d="M5 6h6M5 9h4"/>' },
    { id: 'definition', label: 'Definition', icon: '<path d="M5 3L1 8l4 5M11 3l4 5-4 5"/>' },
    { id: 'test', label: 'Test / Execute', icon: '<polygon points="4,2 13,8 4,14"/>' },
    { id: 'logs', label: 'Execution Logs', icon: '<rect x="2" y="1" width="12" height="14" rx="1.5"/><path d="M5 5h6M5 8h6M5 11h3"/>' },
  ];

  let detailNodes = $derived(selected?.category === 'flows' ? flowDetailTreeNodes : detailTreeNodes);

  // ── Flow definition (Overview form + graph/YAML) ──
  const DEFAULT_FLOW_YAML = `document:
  dsl: 1.0.3
  namespace: a3t
  name: new-flow
  version: 1.0.0
  metadata:
    description: ""
do:
  - step-1:
      set:
        hello: world
`;

  let flowYaml = $state('');
  let flowYamlLoading = $state(false);
  let flowDirty = $state(false);
  let flowDeploying = $state(false);
  let isNewFlow = $state(false);
  let flowMeta = $state({ name: '', namespace: 'a3t', version: '1.0.0', description: '' });
  let flowMetaDirty = $state(false);

  // ── Flow execution history (Logs tab) ──
  let flowExecutions = $state<any[]>([]);
  let execLoading = $state(false);

  async function loadExecutions(r: Resource) {
    execLoading = true;
    flowExecutions = [];
    try {
      const res = await mcpCall('list_executions', { namespace: r.namespace, name: r.name, limit: 25 });
      const rows = Array.isArray(res?.data) ? res.data : (res?.data?.rows ?? res?.data?.output ?? []);
      flowExecutions = Array.isArray(rows) ? rows : [];
    } catch (e) {
      flowExecutions = [];
    } finally {
      execLoading = false;
    }
  }

  function execDuration(e: any): string {
    if (e.duration_us != null) {
      const ms = e.duration_us / 1000;
      return ms < 1000 ? `${ms.toFixed(0)}ms` : `${(ms / 1000).toFixed(2)}s`;
    }
    return '—';
  }

  async function loadFlowDefinition(r: Resource) {
    flowYaml = '';
    flowDirty = false;
    flowYamlLoading = true;
    try {
      const res = await flowsMcp.get(r.name, r.namespace);
      const text = typeof res?.data === 'string' ? res.data : (res?.data?.text ?? '');
      const match = text.match(/```yaml\n([\s\S]*?)```/);
      flowYaml = match ? match[1].trim() : text;
    } catch (e) {
      flowYaml = `# Failed to load flow: ${e}`;
    } finally {
      flowYamlLoading = false;
      syncYamlToMeta();
    }
  }

  // Parse the header fields out of the YAML into the Overview form
  function syncYamlToMeta() {
    try {
      const doc = YAML.parse(flowYaml) ?? {};
      const d = doc.document ?? doc;
      flowMeta = {
        name: d.name ?? '',
        namespace: d.namespace ?? 'a3t',
        version: (d.version ?? '1.0.0') + '',
        description: (d.metadata?.description ?? doc.description ?? '') + '',
      };
      flowMetaDirty = false;
    } catch { /* leave form as-is on parse error */ }
  }

  // Write the Overview form fields back into the YAML header (preserves do/input body)
  function syncMetaToYaml() {
    let doc: any;
    try { doc = YAML.parse(flowYaml) ?? {}; } catch { doc = {}; }
    if (!doc.document) doc.document = {};
    const d = doc.document;
    d.dsl = d.dsl ?? '1.0.3';
    d.name = flowMeta.name;
    d.namespace = flowMeta.namespace;
    d.version = flowMeta.version;
    if (!d.metadata) d.metadata = {};
    d.metadata.description = flowMeta.description;
    flowYaml = YAML.stringify(doc);
    flowMetaDirty = false;
  }

  function newFlow() {
    isNewFlow = true;
    flowMeta = { name: '', namespace: 'a3t', version: '1.0.0', description: '' };
    flowMetaDirty = false;
    flowYaml = DEFAULT_FLOW_YAML;
    flowYamlLoading = false;
    flowDirty = false;
    panelTab = 'overview';
    resetTestState();
    selected = {
      name: 'new-flow', namespace: 'a3t', type: 'workflow', category: 'flows',
      description: '', created_at: '', created_by: '', tags: [], healthy: true,
    };
  }

  // Switching panel tabs — keep Overview form and YAML in sync as you navigate
  function selectPanelTab(id: string) {
    if (selected?.category === 'flows') {
      if (panelTab === 'overview' && id === 'definition' && flowMetaDirty) syncMetaToYaml();
      else if (panelTab === 'definition' && id === 'overview') syncYamlToMeta();
      if (id === 'logs' && !isNewFlow) loadExecutions(selected);
    }
    panelTab = id;
  }

  async function saveFlow() {
    if (selected?.category !== 'flows' || !flowMeta.name) return;
    if (flowMetaDirty) syncMetaToYaml();
    flowDeploying = true;
    try {
      await mcpCall('create_flow', { yaml: flowYaml });
      flowDirty = false;
      flowMetaDirty = false;
      if (isNewFlow) {
        isNewFlow = false;
        allResources = await fetchAllResources();
        const created = allResources.find(x => x.category === 'flows' && x.name === flowMeta.name && x.namespace === flowMeta.namespace);
        if (created) selected = created;
      }
    } catch (e) { error = String(e); }
    finally { flowDeploying = false; }
  }

  // ── Fetch ──
  async function fetchAllResources() {
    const namespaces = ['default', 'a3t', 'system'];
    const results: Resource[] = [];

    // Fetch resources
    for (const ns of namespaces) {
      const res = await resourcesMcp.list(ns);
      const data = res?.data;
      if (data?.resources) {
        for (const r of data.resources) {
          results.push({
            name: r.name, namespace: r.namespace ?? ns,
            type: r.type ?? 'unknown', category: categorize(r.type ?? ''),
            description: r.description ?? '', created_at: r.created_at ?? '',
            created_by: r.created_by ?? '', tags: r.tags ?? [], healthy: true,
            config: r.config ?? {},
          });
        }
      }
    }

    // Fetch workflows and show them under "flows" category
    for (const ns of namespaces) {
      const res = await flowsMcp.list(ns);
      const data = res?.data;
      if (data?.workflows) {
        for (const wf of data.workflows) {
          results.push({
            name: wf.name, namespace: wf.namespace ?? ns,
            type: 'workflow', category: 'flows',
            description: `v${wf.version} · ${wf.step_count} steps`,
            created_at: '', created_by: '',
            tags: [], healthy: wf.enabled !== false,
          });
        }
      }
    }

    return results;
  }

  onMount(async () => {
    try {
      allResources = await fetchAllResources();
    } catch (e) { error = String(e); }
    finally { loading = false; }
  });

  // ── Actions ──
  function selectResource(r: Resource) {
    selected = r;
    isNewFlow = false;
    panelTab = 'overview';
    resetTestState();
    if (r.category === 'flows') loadFlowDefinition(r);
    else loadConfig(r);
  }

  function closePanel() {
    selected = null;
    isNewFlow = false;
  }

  function openCreate(category: string) {
    if (category === 'flows') { newFlow(); return; }
    createForCategory = category;
    // Default type for the category
    const typeMap: Record<string, string> = {
      database: 'postgres', api: 'http', mcp: 'mcp',
      functions: 'function', scripts: 'script', variables: 'variable', credentials: 'credential',
    };
    newResource = { name: '', namespace: 'a3t', type: typeMap[category] ?? '', config: '{}' };
    createTab = 'details';
    showCreate = true;
  }

  async function createResource() {
    if (!newResource.name || !newResource.type) return;
    creating = true;
    try {
      let config: any;
      try { config = JSON.parse(newResource.config); } catch { config = {}; }
      await resourcesMcp.create(newResource.name, newResource.type, config, newResource.namespace);
      showCreate = false;
      allResources = await fetchAllResources();
    } catch (e) { error = String(e); }
    finally { creating = false; }
  }

  function resetTestState() {
    testResult = '';
    llmInput = '';
    llmMessages = [];
    dbQuery = '';
    dbOperation = 'list_tables';
    mcpTools = [];
    mcpSelectedTool = '';
    mcpToolArgs = '{}';
    mcpToolResult = '';
    flowInput = '{}';
    secretConfig = null;
    secretRevealed = false;
  }

  // ── LLM: chat-style test ──
  async function testLlm() {
    if (!selected || !llmInput.trim()) return;
    const msg = llmInput.trim();
    llmMessages = [...llmMessages, { role: 'user', content: msg }];
    llmInput = '';
    testing = true;
    try {
      const res = await execFlow('a3t-test-resource', 'a3t', {
        input: { test_type: 'llm', resource: `${selected.namespace}/${selected.name}`, message: msg }
      });
      const content = res.data?.content ?? res.data?.text ?? (typeof res.data === 'string' ? res.data : JSON.stringify(res.data));
      llmMessages = [...llmMessages, { role: 'assistant', content }];
    } catch (e: any) {
      llmMessages = [...llmMessages, { role: 'assistant', content: `Error: ${e.message ?? e}` }];
    } finally {
      testing = false;
    }
  }

  // ── DB: connection + query test ──
  async function testDb() {
    if (!selected) return;
    testing = true;
    testResult = '';
    try {
      const input: any = { test_type: 'db', resource: `${selected.namespace}/${selected.name}`, operation: dbOperation };
      if (dbOperation === 'query' && dbQuery) input.filter = dbQuery;
      if (dbOperation === 'query' && dbQuery) input.table = dbQuery.split(' ')[0];
      const res = await execFlow('a3t-test-resource', 'a3t', { input });
      testResult = typeof res.data === 'string' ? res.data : JSON.stringify(res.data, null, 2);
    } catch (e: any) {
      testResult = `Error: ${e.message ?? e}`;
    } finally {
      testing = false;
    }
  }

  // ── MCP: list tools ──
  async function testMcpListTools() {
    if (!selected) return;
    testing = true;
    mcpTools = [];
    testResult = '';
    try {
      const res = await execFlow('a3t-test-resource', 'a3t', {
        input: { test_type: 'mcp', resource: `${selected.namespace}/${selected.name}` }
      });
      const tools = res.data?.tools ?? res.data;
      if (Array.isArray(tools)) {
        mcpTools = tools.map((t: any) => ({ name: t.name ?? '', description: t.description ?? '' }));
      } else {
        testResult = typeof tools === 'string' ? tools : JSON.stringify(tools, null, 2);
      }
    } catch (e: any) {
      testResult = `Error: ${e.message ?? e}`;
    } finally {
      testing = false;
    }
  }

  // ── MCP: call selected tool ──
  async function testMcpCallTool() {
    if (!selected || !mcpSelectedTool) return;
    testing = true;
    mcpToolResult = '';
    try {
      let args: any = {};
      try { args = JSON.parse(mcpToolArgs); } catch {}
      const res = await execFlow('a3t-test-resource', 'a3t', {
        input: { test_type: 'mcp_call', resource: `${selected.namespace}/${selected.name}`, tool: mcpSelectedTool, arguments: args }
      });
      mcpToolResult = typeof res.data === 'string' ? res.data : JSON.stringify(res.data, null, 2);
    } catch (e: any) {
      mcpToolResult = `Error: ${e.message ?? e}`;
    } finally {
      testing = false;
    }
  }

  // ── Flow: execute with input ──
  async function testFlow() {
    if (!selected) return;
    testing = true;
    testResult = '';
    try {
      let input: any = {};
      try { input = JSON.parse(flowInput); } catch {}
      const res = await execFlow(selected.name, selected.namespace, input);
      testResult = typeof res.data === 'string' ? res.data : JSON.stringify(res.data, null, 2);
    } catch (e: any) {
      testResult = `Error: ${e.message ?? e}`;
    } finally {
      testing = false;
    }
  }

  // ── Secret: verify and show masked ──
  async function testSecret() {
    if (!selected) return;
    testing = true;
    testResult = '';
    secretConfig = null;
    try {
      const res = await mcpCall('get_resource', { name: selected.name, namespace: selected.namespace });
      const config = res?.data?.config ?? {};
      secretConfig = config;
      testResult = '';
    } catch (e: any) {
      testResult = `Error: ${e.message ?? e}`;
    } finally {
      testing = false;
    }
  }

  // ── Generic fallback test ──
  async function testGeneric() {
    if (!selected) return;
    testing = true;
    testResult = '';
    try {
      const res = await mcpCall('get_resource', { name: selected.name, namespace: selected.namespace });
      testResult = typeof res === 'string' ? res : JSON.stringify(res?.data ?? res, null, 2);
    } catch (e: any) {
      testResult = `Error: ${e.message ?? e}`;
    } finally {
      testing = false;
    }
  }

  function maskValue(val: string): string {
    if (!val || val.length < 8) return '••••••••';
    return val.slice(0, 6) + '••••••••' + val.slice(-4);
  }

  async function deleteResource() {
    if (!deleteTarget) return;
    try {
      let res = await resourcesMcp.remove(deleteTarget.namespace, deleteTarget.name);
      // A runtime is not refused: deleting it terminates it and reschedules what
      // ran there. The server answers 409 with that plan first — show it.
      if (res.status === 409 && res.detail?.impact) {
        const im = res.detail.impact;
        const lines = (im.plan ?? []).map((p: any) =>
          `• ${p.target} (${p.mode}) → ${p.action === 'move' ? `moves to ${p.to}` : `fresh ${p.to} recreated`}${p.warning ? '  ⚠ ' + p.warning : ''}`);
        const pinned = (im.pinnedCanvases ?? []).length ? `\nCanvases pinned to it: ${im.pinnedCanvases.join(', ')}` : '';
        const ok = confirm(`Delete runtime ${deleteTarget.name}?\n\nRuns on it are cancelled and what is deployed there is rescheduled:\n${lines.join('\n')}${pinned}`);
        if (!ok) { deleteTarget = null; return; }
        res = await resourcesMcp.remove(deleteTarget.namespace, deleteTarget.name, deleteTarget.name);
      }
      if (!res.ok) { messages.error(res.error ?? 'Delete failed'); deleteTarget = null; return; }
      if (selected?.name === deleteTarget.name) selected = null;
      deleteTarget = null;
      allResources = await fetchAllResources();
    } catch (e) { error = String(e); deleteTarget = null; }
  }

  // Types available for create form dropdown
  const TYPES_BY_CATEGORY: Record<string, { value: string; label: string }[]> = {
    database:    [{ value: 'postgres', label: 'PostgreSQL' }, { value: 'mysql', label: 'MySQL' }, { value: 'lancedb', label: 'LanceDB' }, { value: 'redis', label: 'Redis / Dragonfly' }, { value: 'qdrant', label: 'Qdrant' }, { value: 'clawdb', label: 'ClawDB' }],
    llm:         [{ value: 'openai', label: 'OpenAI' }, { value: 'anthropic', label: 'Anthropic' }, { value: 'groq', label: 'Groq' }, { value: 'ollama', label: 'Ollama' }],
    mcp:         [{ value: 'mcp', label: 'MCP Server' }],
    functions:   [{ value: 'function', label: 'Function' }],
    scripts:     [{ value: 'script', label: 'Script' }],
    variables:   [{ value: 'variable', label: 'Variable' }],
    credentials: [{ value: 'credential', label: 'Credential' }, { value: 'secret', label: 'Secret' }],
  };
</script>

<!-- Detail Slide Panel -->
<SlidePanel
  open={!!selected}
  onclose={closePanel}
  onsave={selected?.category === 'flows' ? saveFlow : undefined}
  onopenineditor={selected?.category === 'flows' && !isNewFlow ? () => navigateTo('flow-editor', 'Flow Editor') : undefined}
>
  {#if selected}
    <div class="panel-layout">
      <PropertiesTree
        icon={catMeta(selected.category).icon}
        color={CAT_COLORS[selected.category] ?? '#999'}
        name={selected.category === 'flows' && isNewFlow ? (flowMeta.name || 'New flow') : selected.name}
        meta="{selected.type} · {selected.namespace}"
        nodes={detailNodes}
        activeNode={panelTab}
        onselect={selectPanelTab}
      />
      <div class="panel-content" class:panel-content-flush={panelTab === 'definition'}>
        {#if panelTab === 'definition'}
          {#if flowYamlLoading}
            <div class="pf"><div class="pf-empty-state">Loading flow definition…</div></div>
          {:else if flowYaml}
            <div class="flow-def-pane">
              <FlowEditorPane initialYaml={flowYaml} onchange={(y) => { flowYaml = y; flowDirty = true; }} />
            </div>
          {:else}
            <div class="pf"><div class="pf-empty-state">No definition available for this flow.</div></div>
          {/if}

        {:else if panelTab === 'overview' && selected.category === 'flows'}
          <div class="pf">
            <h3>{isNewFlow ? 'New Flow' : 'Overview'}</h3>
            <p class="pf-hint">Edit the flow's properties. The full step graph is in the Definition tab.</p>
            <div class="pf-kv">
              <label class="cf-field"><span>Name</span><input type="text" bind:value={flowMeta.name} oninput={() => flowMetaDirty = true} placeholder="my-flow" /></label>
              <label class="cf-field"><span>Namespace</span><input type="text" bind:value={flowMeta.namespace} oninput={() => flowMetaDirty = true} /></label>
              <label class="cf-field"><span>Version</span><input type="text" bind:value={flowMeta.version} oninput={() => flowMetaDirty = true} /></label>
              <label class="cf-field full"><span>Description</span><textarea rows="3" bind:value={flowMeta.description} oninput={() => flowMetaDirty = true}></textarea></label>
            </div>
            <div class="create-actions">
              {#if isNewFlow}
                <button class="btn ghost" onclick={closePanel}>Cancel</button>
                <button class="btn spec" disabled={flowDeploying || !flowMeta.name} onclick={saveFlow}>{flowDeploying ? 'Creating…' : 'Create Flow'}</button>
              {:else}
                <button class="btn spec" disabled={flowDeploying || !flowMeta.name} onclick={saveFlow}>{flowDeploying ? 'Saving…' : 'Save Changes'}</button>
              {/if}
            </div>
          </div>

        {:else if panelTab === 'overview'}
          <div class="pf">
            <h3>Overview</h3>
            <div class="pf-kv">
              <div class="pf-row"><span class="pf-k">Name</span><span class="pf-v">{selected.name}</span></div>
              <div class="pf-row"><span class="pf-k">Namespace</span><span class="pf-v">{selected.namespace}</span></div>
              <div class="pf-row"><span class="pf-k">Type</span><span class="pf-v mono">{selected.type}</span></div>
              <div class="pf-row"><span class="pf-k">Category</span><span class="pf-v">{catMeta(selected.category).label}</span></div>
              {#if selected.description}
                <div class="pf-row"><span class="pf-k">Description</span><span class="pf-v">{selected.description}</span></div>
              {/if}
              {#if selected.created_at}
                <div class="pf-row"><span class="pf-k">Created</span><span class="pf-v">{new Date(selected.created_at).toLocaleString()}</span></div>
              {/if}
              {#if selected.created_by}
                <div class="pf-row"><span class="pf-k">Created by</span><span class="pf-v">{selected.created_by}</span></div>
              {/if}
              {#if selected.tags.length > 0}
                <div class="pf-row"><span class="pf-k">Tags</span><span class="pf-v">{selected.tags.join(', ')}</span></div>
              {/if}
            </div>
          </div>

        {:else if panelTab === 'config'}
          <div class="pf">
            <h3>Configuration</h3>
            {#if selected.category === 'llm'}
              <p class="pf-hint">Where this LLM's calls go. <strong>Endpoint</strong> is the API base URL the request is sent to.</p>
              <label class="cf-field"><span>Endpoint (API URL)</span>
                <input type="text" bind:value={llmCfg.api_url} placeholder="https://api.provider.com/v1" readonly={!$canUpdate} />
              </label>
              <label class="cf-field"><span>Model</span>
                <input type="text" bind:value={llmCfg.model} placeholder="model-id" readonly={!$canUpdate} />
              </label>
              <div class="cf-grid2">
                <label class="cf-field"><span>Provider</span><input type="text" bind:value={llmCfg.provider} placeholder="openai" readonly={!$canUpdate} /></label>
                <label class="cf-field"><span>Max tokens</span><input type="number" bind:value={llmCfg.max_tokens} readonly={!$canUpdate} /></label>
              </div>
              <div class="cf-grid2">
                <label class="cf-field"><span>Temperature</span><input type="number" step="0.1" bind:value={llmCfg.temperature} readonly={!$canUpdate} /></label>
                <label class="cf-field"><span>API key source</span>
                  <select value={llmCfg.api_key_ref != null ? 'secret' : 'inline'} onchange={(e) => setKeyMode(e.currentTarget.value)} disabled={!$canUpdate}>
                    <option value="secret">Secret reference (recommended)</option>
                    <option value="inline">Inline key</option>
                  </select>
                </label>
              </div>
              {#if llmCfg.api_key_ref != null}
                <label class="cf-field"><span>Secret</span>
                  <select bind:value={llmCfg.api_key_ref} disabled={!$canUpdate}>
                    <option value="">— select a secret —</option>
                    {#each secretRefs as s}<option value={s.ref}>{s.ref}</option>{/each}
                    {#if llmCfg.api_key_ref && !secretRefs.some((s) => s.ref === llmCfg.api_key_ref)}<option value={llmCfg.api_key_ref}>{llmCfg.api_key_ref} (missing)</option>{/if}
                  </select>
                  <span class="pf-hint" style="margin:4px 0 0;">Points at a <strong>Secret</strong> resource (its <span class="mono">config.value</span> holds the key). Pick the same secret your working LLMs use (e.g. <span class="mono">openrouter-key</span>).</span>
                </label>
              {:else}
                <label class="cf-field"><span>Inline key</span><input type="password" bind:value={llmCfg.api_key} placeholder="sk-…" readonly={!$canUpdate} /></label>
              {/if}
              {#if $canUpdate}
                <div class="cfg-actions">
                  <button class="btn spec sm" disabled={cfgSaving} onclick={saveConfig}>{cfgSaving ? 'Saving…' : 'Save'}</button>
                  {#if cfgMsg}<span class="cfg-msg" class:ok={cfgMsg === 'Saved'}>{cfgMsg}</span>{/if}
                </div>
              {/if}
              <details class="cfg-raw"><summary>Raw config JSON</summary><pre class="pf-code">{JSON.stringify(llmCfg, null, 2)}</pre></details>
            {:else}
              <p class="pf-hint">Resource configuration.</p>
              <div class="pf-kv">
                <div class="pf-row"><span class="pf-k">Type Ref</span><span class="pf-v mono">{selected.type}</span></div>
                <div class="pf-row"><span class="pf-k">Namespace</span><span class="pf-v mono">{selected.namespace}</span></div>
                {#each Object.entries(selected.config ?? {}) as [k, v]}
                  <div class="pf-row"><span class="pf-k">{k}</span><span class="pf-v mono">{typeof v === 'object' ? JSON.stringify(v) : String(v)}</span></div>
                {/each}
              </div>
              {#if !selected.config || Object.keys(selected.config).length === 0}<p class="pf-hint" style="margin-top:10px;">No config values.</p>{/if}
            {/if}
          </div>

        {:else if panelTab === 'test'}
          <div class="pf">
            {#if selected.category === 'llm'}
              <!-- ── LLM: Chat test ── -->
              <h3>Chat Test</h3>
              <p class="pf-hint">Send a message to test the LLM connection.</p>
              <div class="test-chat">
                <div class="chat-messages">
                  {#if llmMessages.length === 0}
                    <div class="chat-empty">Send a message to test this LLM resource</div>
                  {/if}
                  {#each llmMessages as msg}
                    <div class="chat-msg" class:chat-user={msg.role === 'user'} class:chat-assistant={msg.role === 'assistant'}>
                      <span class="chat-role">{msg.role}</span>
                      <div class="chat-content">{msg.content}</div>
                    </div>
                  {/each}
                  {#if testing}
                    <div class="chat-msg chat-assistant"><span class="chat-role">assistant</span><div class="chat-content chat-typing">Thinking...</div></div>
                  {/if}
                </div>
                <div class="chat-input-row">
                  <input type="text" class="chat-input" placeholder="Type a message..." bind:value={llmInput}
                    onkeydown={(e) => { if (e.key === 'Enter' && !testing) testLlm(); }} disabled={testing} />
                  <button class="btn spec sm" disabled={testing || !llmInput.trim()} onclick={testLlm}>Send</button>
                </div>
              </div>

            {:else if selected.category === 'database'}
              <!-- ── Database: Connection + Query ── -->
              <h3>Database Test</h3>
              <p class="pf-hint">Test connectivity and run queries.</p>
              <div class="test-db">
                <label class="cf-field">
                  <span>Operation</span>
                  <select bind:value={dbOperation}>
                    <option value="list_tables">List Tables</option>
                    <option value="query">Query</option>
                  </select>
                </label>
                {#if dbOperation === 'query'}
                  <label class="cf-field full">
                    <span>Table / Filter</span>
                    <input type="text" bind:value={dbQuery} placeholder="table_name" />
                  </label>
                {/if}
                <button class="btn spec sm" disabled={testing} onclick={testDb}>
                  {testing ? 'Running...' : 'Execute'}
                </button>
              </div>
              {#if testResult}
                <pre class="pf-code" style="margin-top: 12px;">{testResult}</pre>
              {/if}

            {:else if selected.category === 'mcp'}
              <!-- ── MCP: Tools list + call ── -->
              <h3>MCP Server Test</h3>
              <p class="pf-hint">Discover tools and test tool calls.</p>
              <button class="btn spec sm" disabled={testing} onclick={testMcpListTools} style="margin-bottom: 16px;">
                {testing && mcpTools.length === 0 ? 'Discovering...' : 'Discover Tools'}
              </button>
              {#if mcpTools.length > 0}
                <div class="mcp-tools">
                  <div class="mcp-tools-header">{mcpTools.length} tools available</div>
                  {#each mcpTools as tool}
                    <button class="mcp-tool-item" class:active={mcpSelectedTool === tool.name}
                      onclick={() => { mcpSelectedTool = tool.name; mcpToolArgs = '{}'; mcpToolResult = ''; }}>
                      <span class="mcp-tool-name">{tool.name}</span>
                      <span class="mcp-tool-desc">{tool.description}</span>
                    </button>
                  {/each}
                </div>
                {#if mcpSelectedTool}
                  <div class="mcp-call-section">
                    <div class="mcp-call-header">Call: {mcpSelectedTool}</div>
                    <label class="cf-field full">
                      <span>Arguments (JSON)</span>
                      <textarea rows="4" bind:value={mcpToolArgs}></textarea>
                    </label>
                    <button class="btn spec sm" disabled={testing} onclick={testMcpCallTool}>
                      {testing ? 'Calling...' : 'Call Tool'}
                    </button>
                    {#if mcpToolResult}
                      <pre class="pf-code" style="margin-top: 12px;">{mcpToolResult}</pre>
                    {/if}
                  </div>
                {/if}
              {/if}
              {#if testResult}
                <pre class="pf-code" style="margin-top: 12px;">{testResult}</pre>
              {/if}

            {:else if selected.category === 'credentials'}
              <!-- ── Secret: Verify + masked display ── -->
              <h3>Secret Verification</h3>
              <p class="pf-hint">Verify the secret exists and inspect its masked value.</p>
              <button class="btn spec sm" disabled={testing} onclick={testSecret} style="margin-bottom: 16px;">
                {testing ? 'Verifying...' : 'Verify Secret'}
              </button>
              {#if secretConfig}
                <div class="secret-display">
                  {#each Object.entries(secretConfig) as [key, val]}
                    <div class="pf-row">
                      <span class="pf-k">{key}</span>
                      <span class="pf-v mono">
                        {#if key === 'value' || key === 'token' || key === 'password' || key === 'api_key'}
                          {secretRevealed ? String(val) : maskValue(String(val))}
                        {:else}
                          {String(val)}
                        {/if}
                      </span>
                    </div>
                  {/each}
                  <button class="btn ghost sm" style="margin-top: 12px;" onclick={() => secretRevealed = !secretRevealed}>
                    {secretRevealed ? 'Hide Values' : 'Reveal Values'}
                  </button>
                </div>
              {/if}
              {#if testResult}
                <pre class="pf-code" style="margin-top: 12px;">{testResult}</pre>
              {/if}

            {:else if selected.category === 'flows'}
              <!-- ── Flow: Execute with input ── -->
              <h3>Execute Flow</h3>
              <p class="pf-hint">Run the workflow with custom input.</p>
              <label class="cf-field full">
                <span>Input (JSON)</span>
                <textarea rows="6" bind:value={flowInput}></textarea>
              </label>
              <button class="btn spec sm" disabled={testing} onclick={testFlow} style="margin-top: 8px;">
                {testing ? 'Running...' : 'Execute Flow'}
              </button>
              {#if testResult}
                <pre class="pf-code" style="margin-top: 12px;">{testResult}</pre>
              {/if}

            {:else}
              <!-- ── Generic fallback ── -->
              <h3>Test / Execute</h3>
              <p class="pf-hint">Test connectivity and inspect the resource state.</p>
              <button class="btn spec sm" disabled={testing} onclick={testGeneric}>
                {testing ? 'Testing...' : 'Run Test'}
              </button>
              {#if testResult}
                <pre class="pf-code" style="margin-top: 12px;">{testResult}</pre>
              {/if}
            {/if}
          </div>

        {:else if panelTab === 'logs' && selected.category === 'flows'}
          <div class="pf">
            <div class="logs-head">
              <h3>Execution Logs</h3>
              <button class="btn ghost sm" disabled={execLoading} onclick={() => loadExecutions(selected)}>{execLoading ? 'Loading…' : 'Refresh'}</button>
            </div>
            <p class="pf-hint">Recent runs of this flow, newest first.</p>
            {#if execLoading}
              <div class="pf-empty-state">Loading executions…</div>
            {:else if flowExecutions.length === 0}
              <div class="pf-empty-state">No executions yet. Run this flow from the Test tab.</div>
            {:else}
              <div class="exec-list">
                {#each flowExecutions as e}
                  <details class="exec-row">
                    <summary class="exec-summary">
                      <span class="exec-status {String(e.status).toLowerCase()}">{e.status}</span>
                      <span class="exec-time">{e.started_at ? new Date(e.started_at).toLocaleString() : '—'}</span>
                      <span class="exec-dur">{execDuration(e)}</span>
                      <span class="exec-id mono">{String(e.instance_id ?? '').slice(0, 8)}</span>
                    </summary>
                    <div class="exec-detail">
                      {#if e.error}<div class="exec-err">{e.error}</div>{/if}
                      <div class="exec-kv"><span>Input</span><pre class="pf-code">{JSON.stringify(e.input ?? {}, null, 2)}</pre></div>
                      {#if e.output != null}<div class="exec-kv"><span>Output</span><pre class="pf-code">{JSON.stringify(e.output, null, 2)}</pre></div>{/if}
                    </div>
                  </details>
                {/each}
              </div>
            {/if}
          </div>

        {:else if panelTab === 'logs'}
          <div class="pf">
            <h3>Execution Logs</h3>
            <p class="pf-hint">Recent operations and events for this resource.</p>
            <div class="pf-empty-state">No logs available yet</div>
          </div>
        {/if}
      </div>
    </div>
  {/if}
</SlidePanel>

<!-- Delete Confirm -->
<DeleteConfirm
  open={!!deleteTarget}
  entityType="Resource"
  entityName={deleteTarget?.name ?? ''}
  onconfirm={deleteResource}
  oncancel={() => deleteTarget = null}
/>

<!-- Create Slide Panel -->
<SlidePanel open={showCreate} onclose={() => showCreate = false}>
  <div class="panel-layout">
    <PropertiesTree
      icon="+"
      color={CAT_COLORS[createForCategory] ?? '#999'}
      name="New {catMeta(createForCategory).label.replace(/s$/, '')}"
      meta={createForCategory}
      nodes={createTreeNodes}
      activeNode={createTab}
      onselect={(id) => { createTab = id; }}
    />
    <div class="panel-content">
      {#if createTab === 'details'}
        <div class="pf">
          <h3>Details</h3>
          <div class="pf-kv">
            <label class="cf-field"><span>Name</span><input type="text" bind:value={newResource.name} placeholder="my-resource" /></label>
            <label class="cf-field"><span>Namespace</span><input type="text" bind:value={newResource.namespace} /></label>
            <label class="cf-field"><span>Type</span>
              <select bind:value={newResource.type}>
                {#each (TYPES_BY_CATEGORY[createForCategory] ?? []) as t}
                  <option value={t.value}>{t.label}</option>
                {/each}
              </select>
            </label>
          </div>
        </div>

      {:else if createTab === 'config'}
        <div class="pf">
          <h3>Configuration</h3>
          <p class="pf-hint">Provide the connection configuration as JSON.</p>
          <label class="cf-field full">
            <span>Config (JSON)</span>
            <textarea rows="10" bind:value={newResource.config}></textarea>
          </label>
        </div>

      {:else if createTab === 'review'}
        <div class="pf">
          <h3>Review & Create</h3>
          <div class="pf-kv">
            <div class="pf-row"><span class="pf-k">Name</span><span class="pf-v">{newResource.name || '—'}</span></div>
            <div class="pf-row"><span class="pf-k">Namespace</span><span class="pf-v">{newResource.namespace}</span></div>
            <div class="pf-row"><span class="pf-k">Type</span><span class="pf-v mono">{newResource.type || '—'}</span></div>
          </div>
          <pre class="pf-code" style="margin-top: 16px;">{newResource.config}</pre>
          <div class="create-actions">
            <button class="btn ghost" onclick={() => showCreate = false}>Cancel</button>
            <button class="btn spec" disabled={creating || !newResource.name || !newResource.type} onclick={createResource}>
              {creating ? 'Creating...' : 'Create Resource'}
            </button>
          </div>
        </div>
      {/if}
    </div>
  </div>
</SlidePanel>

<!-- Page Header -->
<div class="screen-header">
  <div class="breadcrumb">Resources <span class="sep">/</span> Registry</div>
  <div class="screen-title-row">
    <div class="title-block">
      <h1 class="screen-title">Resources</h1>
      <button class="help-btn" onclick={() => docsOpen = true} title="Learn more">
        <svg width="14" height="14" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5"><circle cx="8" cy="8" r="7"/><path d="M6 6a2 2 0 114 0c0 1-1.5 1.5-2 2M8 11.5v.5"/></svg>
      </button>
    </div>
    <div class="screen-actions">
      {#if $canCreate}<button class="btn spec" onclick={() => openCreate(activeCategory === 'all' ? 'database' : activeCategory)}>+ New resource</button>{/if}
    </div>
  </div>
</div>

<!-- Body only: the category menu is the shell's context pane now, so keeping one
     here put two identical column of categories side by side. -->
<div class="resources-layout">
  <div class="resources-body">
    {#if !loading}
      <PageBanner
        headline="Connect your resources."
        highlightWord="resources"
        subtitle="Databases, LLMs, MCP servers, memory — everything agents need."
        description="Resources are named connections to external systems. Register them once, reference by name in any workflow or agent configuration."
        accent="ephemeral"
        docsPath="/resources"
      />
      <!-- Pinned and latest, under the banner: the section page leads with what
           it is for, then the items you actually return to. -->
      <AppHome appType="resources" embedded />

      {#if activeCategory === 'all'}
        <!-- Show all categories as sections -->
        {#each CATEGORIES.filter(c => c.key !== 'all') as cat}
          {@const items = allResources.filter(r => r.category === cat.key)}
          <div class="section-group">
            <div class="section-header">
              <span class="cat-icon-sm" style={cat.iconStyle}>{cat.icon}</span>
              <h3>{cat.label}</h3>
              <span class="section-count">{items.length}</span>
              <span style="flex: 1;"></span>
              <button class="section-add" onclick={() => openCreate(cat.key)}>
                <svg width="12" height="12" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"><path d="M8 3v10M3 8h10"/></svg>
                + New
              </button>
            </div>
            {#if items.length > 0}
              <div class="card-grid">
                {#each items as r}
                  <EntityCard
                    icon={catMeta(r.category).icon}
                    color={CAT_COLORS[r.category] ?? '#999'}
                    name={r.name}
                    description={r.description || r.type}
                    tags={[r.type, r.namespace]}
                    meta={r.created_at ? new Date(r.created_at).toLocaleDateString() : ''}
                    actionLabel="Test Connection"
                    onclick={() => selectResource(r)}
                    onaction={() => { selectResource(r); panelTab = 'test'; }}
                    ondelete={() => deleteTarget = { name: r.name, namespace: r.namespace }}
                  />
                {/each}
              </div>
            {:else}
              <div class="empty-section-inline">No {cat.label.toLowerCase()} yet</div>
            {/if}
          </div>
        {/each}

      {:else}
        <!-- Single category view -->
        {@const items = filteredResources}
        <div class="section-group">
          <div class="section-header">
            <h3>{catMeta(activeCategory).label}</h3>
            <span class="section-count">{items.length}</span>
            <span style="flex: 1;"></span>
            <button class="section-add" onclick={() => openCreate(activeCategory)}>
              <svg width="12" height="12" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"><path d="M8 3v10M3 8h10"/></svg>
              + New
            </button>
          </div>
          {#if items.length > 0}
            <div class="card-grid">
              {#each items as r}
                <EntityCard
                  icon={catMeta(r.category).icon}
                  color={CAT_COLORS[r.category] ?? '#999'}
                  name={r.name}
                  description={r.description || r.type}
                  tags={[r.type, r.namespace]}
                  meta={r.created_at ? new Date(r.created_at).toLocaleDateString() : ''}
                  actionLabel="Test Connection"
                  onclick={() => selectResource(r)}
                  onaction={() => { selectResource(r); panelTab = 'test'; }}
                  ondelete={() => deleteTarget = { name: r.name, namespace: r.namespace }}
                />
              {/each}
            </div>
          {:else}
            <div class="empty-section">
              <p>No {catMeta(activeCategory).label.toLowerCase()} registered yet.</p>
              {#if $canCreate}
                <button class="btn spec sm" onclick={() => openCreate(activeCategory)}>+ Add {catMeta(activeCategory).label.replace(/s$/, '')}</button>
              {/if}
            </div>
          {/if}
        </div>
      {/if}
    {:else}
      <div class="loading-msg">Loading resources...</div>
    {/if}
  </div>
</div>

<DocsPanel open={docsOpen} pageKey="resources" onClose={() => docsOpen = false} />

<style>
  /* Header */
  .screen-header { padding: 18px 32px 12px; border-bottom: 1px solid var(--border); background: var(--bg-page); flex-shrink: 0; }
  .breadcrumb { font-size: 12px; color: var(--text-3); margin-bottom: 8px; font-family: var(--font-mono); }
  .breadcrumb .sep { color: var(--text-5); }
  .screen-title-row { display: flex; align-items: center; gap: 18px; }
  .title-block { flex: 1; display: flex; align-items: center; gap: 10px; }
  .screen-title { font-family: var(--font-serif); font-size: 24px; font-weight: 500; letter-spacing: -0.02em; }
  .help-btn { width: 24px; height: 24px; border: 1px solid var(--border); border-radius: 50%; background: var(--bg-surface); color: var(--text-4); display: grid; place-items: center; cursor: pointer; }
  .help-btn:hover { background: var(--spec-bg); border-color: var(--spec); color: var(--spec); }
  .screen-actions { display: flex; gap: 8px; }

  /* Layout: sidebar + body */
  .resources-layout { flex: 1; display: flex; overflow: hidden; }

  /* Category menu */

  /* Body */
  .resources-body { flex: 1; overflow-y: auto; padding: 20px 32px 40px; }

  /* Section groups */
  .section-group { margin-bottom: 28px; }
  .section-header {
    display: flex; align-items: center; gap: 10px;
    padding-bottom: 10px; margin-bottom: 12px; border-bottom: 1px solid var(--divider);
  }
  .section-header h3 { font-family: var(--font-serif); font-size: 16px; font-weight: 500; color: var(--text-1); }
  .section-count { font-family: var(--font-mono); font-size: 11px; color: var(--text-4); background: var(--bg-subtle); padding: 1px 6px; border-radius: 3px; }
  .cat-icon-sm {
    width: 20px; height: 20px; border-radius: 4px;
    display: inline-grid; place-items: center;
    font-size: 9px; font-weight: 700; font-family: var(--font-mono); flex-shrink: 0;
  }
  .section-add {
    display: flex; align-items: center; gap: 4px;
    padding: 4px 10px; border: 1px dashed var(--border); border-radius: var(--r-sm);
    background: none; color: var(--text-3); font-size: 11px;
    cursor: pointer; transition: all 0.12s;
  }
  .section-add:hover { background: var(--bg-subtle); color: var(--text-1); border-color: var(--border-strong); }

  /* Card grid */
  .card-grid { display: grid; grid-template-columns: repeat(auto-fill, minmax(280px, 1fr)); gap: 12px; }

  /* Empty categories */

  .empty-section {
    padding: 40px 20px; text-align: center;
    background: var(--bg-surface); border: 1px solid var(--border); border-radius: var(--r);
  }
  .empty-section p { color: var(--text-4); font-size: 13px; margin-bottom: 12px; }
  .empty-section-inline {
    padding: 14px 16px; font-size: 12px; color: var(--text-4);
    background: var(--bg-surface); border: 1px solid var(--border); border-radius: var(--r);
  }

  /* Create form fields */
  .create-actions { display: flex; gap: 8px; justify-content: flex-end; margin-top: 20px; padding-top: 16px; border-top: 1px solid var(--divider); }

  /* Panel layout (matches AgentsList) */
  .panel-layout { display: grid; grid-template-columns: 200px 1fr; height: 100%; }
  .panel-content { overflow-y: auto; }
  .panel-content-flush { overflow: hidden; }
  .flow-def-pane { height: 100%; overflow: hidden; }

  /* Execution logs */
  .logs-head { display: flex; align-items: center; justify-content: space-between; gap: 12px; }
  .logs-head h3 { margin-bottom: 0; }
  .exec-list { display: flex; flex-direction: column; gap: 6px; margin-top: 8px; }
  .exec-row { border: 1px solid var(--border); border-radius: var(--r-sm); background: var(--bg-surface); overflow: hidden; }
  .exec-summary { display: flex; align-items: center; gap: 12px; padding: 9px 12px; cursor: pointer; font-size: 12px; list-style: none; }
  .exec-summary::-webkit-details-marker { display: none; }
  .exec-status { font-size: 10px; font-weight: 600; padding: 2px 8px; border-radius: 3px; text-transform: capitalize; background: var(--bg-subtle); color: var(--text-3); flex-shrink: 0; }
  .exec-status.completed { background: var(--ok-bg, #e3f4ef); color: var(--ok, #23987f); }
  .exec-status.failed { background: var(--err-bg, #fde8e8); color: var(--err, #d44); }
  .exec-status.running { background: var(--spec-bg); color: var(--spec-ink); }
  .exec-time { color: var(--text-2); flex: 1; }
  .exec-dur { color: var(--text-4); font-family: var(--font-mono); font-size: 11px; }
  .exec-id { color: var(--text-4); font-size: 11px; }
  .exec-detail { padding: 10px 12px; border-top: 1px solid var(--divider); display: flex; flex-direction: column; gap: 8px; }
  .exec-err { font-size: 12px; color: var(--err, #d44); font-family: var(--font-mono); background: var(--err-bg, #fde8e8); padding: 8px 10px; border-radius: var(--r-sm); }
  .exec-kv { display: flex; flex-direction: column; gap: 3px; }
  .exec-kv > span { font-size: 10px; font-weight: 600; text-transform: uppercase; letter-spacing: 0.04em; color: var(--text-4); }
  .pf { padding: 24px 32px; max-width: 600px; }
  .pf h3 { font-family: var(--font-serif); font-size: 18px; font-weight: 500; margin-bottom: 16px; }
  .pf-kv { display: flex; flex-direction: column; }
  .pf-row { display: flex; justify-content: space-between; align-items: center; padding: 9px 0; border-bottom: 1px solid var(--divider); font-size: 13px; gap: 12px; }
  .pf-row:last-child { border-bottom: 0; }
  .pf-k { color: var(--text-3); flex-shrink: 0; }
  .pf-v { color: var(--text-1); text-align: right; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .pf-hint { font-size: 12px; color: var(--text-4); margin-bottom: 12px; }
  .pf-code { font-family: var(--font-mono); font-size: 12px; line-height: 1.6; padding: 12px 16px; margin: 0; background: var(--bg-subtle); border: 1px solid var(--border); border-radius: var(--r-sm); white-space: pre; overflow: auto; max-height: 300px; }
  /* LLM/resource config fields */
  .cf-grid2 { display: grid; grid-template-columns: 1fr 1fr; gap: 12px; }
  .cfg-actions { display: flex; align-items: center; gap: 10px; margin-top: 8px; }
  .cfg-msg { font-size: 12px; color: var(--text-4); }
  .cfg-msg.ok { color: var(--ok, #23987f); }
  .cfg-raw { margin-top: 16px; }
  .cfg-raw summary { font-size: 12px; color: var(--text-4); cursor: pointer; }
  .cfg-raw pre { margin-top: 8px; }
  .pf-empty-state { padding: 20px; text-align: center; color: var(--text-4); font-size: 13px; background: var(--bg-subtle); border-radius: var(--r-sm); }

  .loading-msg { padding: 40px; text-align: center; color: var(--text-4); font-size: 14px; }

  /* ── LLM Chat test ── */
  .test-chat { display: flex; flex-direction: column; gap: 12px; }
  .chat-messages {
    max-height: 320px; overflow-y: auto;
    border: 1px solid var(--border); border-radius: var(--r-sm);
    background: var(--bg-subtle); padding: 12px;
    display: flex; flex-direction: column; gap: 8px;
  }
  .chat-empty { color: var(--text-4); font-size: 12px; text-align: center; padding: 24px; }
  .chat-msg { display: flex; flex-direction: column; gap: 2px; }
  .chat-role { font-size: 10px; font-family: var(--font-mono); color: var(--text-4); text-transform: uppercase; letter-spacing: 0.05em; }
  .chat-content {
    font-size: 13px; line-height: 1.5; color: var(--text-1);
    padding: 8px 12px; border-radius: var(--r-sm);
    white-space: pre-wrap; word-break: break-word;
  }
  .chat-user .chat-content { background: var(--spec-bg); }
  .chat-assistant .chat-content { background: var(--bg-surface); border: 1px solid var(--border); }
  .chat-typing { color: var(--text-3); font-style: italic; }
  .chat-input-row { display: flex; gap: 8px; }
  .chat-input {
    flex: 1; padding: 8px 12px; border: 1px solid var(--border); border-radius: var(--r-sm);
    background: var(--bg-surface); color: var(--text-1); font-size: 13px;
    font-family: var(--font-sans);
  }
  .chat-input:focus { outline: none; border-color: var(--spec); }

  /* ── DB test ── */
  .test-db { display: flex; flex-direction: column; gap: 12px; }

  /* ── MCP tools ── */
  .mcp-tools {
    border: 1px solid var(--border); border-radius: var(--r-sm);
    background: var(--bg-surface); margin-bottom: 16px; max-height: 240px; overflow-y: auto;
  }
  .mcp-tools-header {
    padding: 8px 12px; font-size: 11px; color: var(--text-3);
    border-bottom: 1px solid var(--divider); font-family: var(--font-mono);
  }
  .mcp-tool-item {
    display: flex; flex-direction: column; gap: 2px; width: 100%;
    padding: 8px 12px; border: none; border-bottom: 1px solid var(--divider);
    background: none; text-align: left; cursor: pointer; transition: background 0.1s;
  }
  .mcp-tool-item:last-child { border-bottom: none; }
  .mcp-tool-item:hover { background: var(--bg-subtle); }
  .mcp-tool-item.active { background: var(--spec-bg); }
  .mcp-tool-name { font-size: 12px; font-family: var(--font-mono); color: var(--text-1); font-weight: 500; }
  .mcp-tool-desc { font-size: 11px; color: var(--text-3); }
  .mcp-call-section { margin-top: 4px; padding-top: 12px; border-top: 1px solid var(--divider); }
  .mcp-call-header { font-size: 12px; font-family: var(--font-mono); color: var(--spec); margin-bottom: 8px; }

  /* ── Secret display ── */
  .secret-display {
    border: 1px solid var(--border); border-radius: var(--r-sm);
    background: var(--bg-surface); padding: 12px 16px;
  }
</style>
