<script lang="ts">
  import { namespacesFor } from '$lib/stores/context.svelte.ts';
  import SubmenuToggle from '$lib/components/shared/SubmenuToggle.svelte';
  import NewButton from '$lib/components/shared/NewButton.svelte';
  import { execFlow, mcpCall, canvasApi } from '$lib/services/api';
  import { discoverServerTools, allToolNames, toolSections, isDestructive, serverRef, type ServerTools } from '$lib/services/mcpTools';
  import { navigateTo, navigateToDetail, getActivePanel } from '$lib/stores/panels.svelte.ts';
  import { can } from '$lib/stores/auth';
  import { onMount } from 'svelte';
  import * as YAML from 'yaml';
  import SlidePanel from '$lib/components/shared/SlidePanel.svelte';
  import PageBanner from '$lib/components/shared/PageBanner.svelte';
  import AppHome from '$lib/shell/AppHome.svelte';
  import EntityCard from '$lib/components/shared/EntityCard.svelte';
  import FoldSection from '$lib/components/shared/FoldSection.svelte';
  import { currentUser } from '$lib/stores/auth';
  import { getProject, inContext } from '$lib/stores/context.svelte.ts';
  import PropertiesTree from '$lib/components/shared/PropertiesTree.svelte';
  import DeleteConfirm from '$lib/components/shared/DeleteConfirm.svelte';
  import FlowEditorPane from '$lib/components/editor/FlowEditorPane.svelte';
  import ResourceSelect from '$lib/components/editor/ResourceSelect.svelte';

  const canCreate = can('create', 'agent');

  let loading = $state(true);
  let error = $state('');
  let selected = $state('');
  let propNode = $state('detail');

  let allAgents: any[] = $state([]);
  /// In view for the project; split by who created them.
  const visibleAgents = $derived(allAgents.filter((a) => inContext(a.namespace, 'shared')));
  const myAgents = $derived(visibleAgents.filter((a) => a.created_by === $currentUser?.id));
  const sharedAgents = $derived(visibleAgents.filter((a) => a.created_by !== $currentUser?.id));
  /// New agents go into the selected project (`default` under "All projects").
  const newNamespace = () => getProject() || 'default';
  let templates: { id: string; name: string; version: string; spec_yaml: string }[] = $state([]);

  // Default target flow (path) for an agent, derived from its template's spec document.
  // Agents must target a CHAT WRAPPER that assembles the `config` object (system_prompt,
  // llm_resource, mcp_servers) from the flat webhook body. The raw system flows
  // (system/agent-engine, system/agent-template) expect a full `config` and would receive
  // null → "invalid agent config: invalid type: null, expected struct AgentSettings".
  function deriveTarget(specYaml: string): string {
    // An agent ALWAYS runs through a chat WRAPPER flow (a registered workflow that
    // assembles the config + reasoning loop). A template is NOT a workflow, so the
    // target must never be the template's own name (→ "Workflow not found").
    try {
      const doc = YAML.parse(specYaml) ?? {};
      const d = doc.document ?? doc;
      const name = d?.name;
      if (name === 'agent-engine') return 'a3t/a3t-chat-engine';   // native engine wrapper
      if (name === 'agent-template') return 'a3t/a3t-chat-agent';  // SWF template wrapper
      // Custom template: pick the wrapper by ENGINE TYPE (native engine → engine
      // wrapper), else the SWF wrapper. Never the template name.
      if (/a3t:agent|agent-engine|call:\s*a3t/i.test(specYaml)) return 'a3t/a3t-chat-engine';
    } catch {}
    return 'a3t/a3t-chat-agent';
  }

  function onTemplateChange() {
    const t = templates.find(t => t.id === newAgent.template_id);
    if (t) {
      newAgent.template_version = t.version;
      newAgent.target = deriveTarget(t.spec_yaml);
      const cfg = templateConfig(t.spec_yaml);
      if (cfg.llm_resource) newAgent.llm = String(cfg.llm_resource).includes('/') ? cfg.llm_resource : `a3t/${cfg.llm_resource}`;
      if (Array.isArray(cfg.mcp_servers)) newAgent.mcp_servers = cfg.mcp_servers;
      // Inherit the template's capability ceiling as the new agent's default.
      newAgent.enabled_tools = Array.isArray(cfg.enabled_tools) ? [...cfg.enabled_tools] : null;
    }
  }

  // When true, the slide-in panel is creating a not-yet-persisted agent. The
  // same panel serves both editing and creation; `agent` reflects the draft
  // (from newAgent) in create mode so the tree header updates as you type.
  let creatingNew = $state(false);
  let agent = $derived(
    creatingNew
      ? { ...newAgent, id: '(new)', isNew: true, status: 'draft', created_by: '' }
      : (selected ? allAgents.find(a => a.id === selected) : null)
  );

  // Delete state
  let deleteTarget = $state<{ id: string; name: string } | null>(null);

  // Create form — all references are selections (no loose strings)
  let newAgent = $state({
    name: '', namespace: newNamespace(), template_id: '', template_version: '1.0.0',
    mode: 'session', target: '', ep_path: '',
    llm: 'a3t/claude-cli-max', mcp_servers: [] as string[], enabled_tools: null as string[] | null,
  });
  let creating = $state(false);

  function newAgentPanel() {
    selected = '';
    newAgent = {
      name: '', namespace: newNamespace(), template_id: '', template_version: '1.0.0',
      mode: 'session', target: '', ep_path: '', llm: 'a3t/claude-cli-max', mcp_servers: [],
    };
    creatingNew = true;
    propNode = 'detail';
  }

  // Tree nodes
  const treeNodes = [
    { id: 'detail', label: 'Agent Detail', icon: '<rect x="3" y="1" width="10" height="7" rx="2"/><path d="M5 8v2a3 3 0 006 0V8"/>' },
    { id: 'config', label: 'Configuration', icon: '<path d="M8 1v4M8 11v4M1 8h4M11 8h4"/><circle cx="8" cy="8" r="3"/>' },
    { id: 'designer', label: 'Designer', icon: '<rect x="1" y="1" width="5" height="5" rx="1"/><rect x="10" y="10" width="5" height="5" rx="1"/><path d="M6 3.5h4M8 3.5v9"/>' },
    { id: 'template', label: 'Template', icon: '<rect x="1" y="2" width="5" height="4" rx="1"/><rect x="10" y="2" width="5" height="4" rx="1"/><rect x="5.5" y="10" width="5" height="4" rx="1"/><path d="M3.5 6v2a2 2 0 002 2h5a2 2 0 002-2V6"/><path d="M8 10V8"/>' },
    { id: 'canvas', label: 'Canvas', icon: '<path d="M8 1v9M4.5 6.5L8 10l3.5-3.5M2 13h12"/>' },
    { id: 'policy', label: 'Permissions', icon: '<path d="M8 1L2 4v4c0 4 3 6.5 6 7.5 3-1 6-3.5 6-7.5V4z"/>' },
    { id: 'ownership', label: 'Ownership & Access', icon: '<circle cx="8" cy="5" r="3"/><path d="M2 14a6 6 0 0112 0"/>' },
  ];

  async function loadAgents() {
    loading = true; error = '';
    const res = await execFlow('a3t-agents-list', 'a3t', { org_id: 'org-default' });
    if (res.ok) {
      allAgents = (Array.isArray(res.data) ? res.data : (res.data?.rows ?? res.data?.output ?? [])).map((r: any) => ({
        id: r.id, name: r.name, namespace: r.namespace ?? 'a3t',
        mode: r.mode ?? 'session', status: r.status ?? 'draft',
        template_id: r.template_id, template_version: r.template_version ?? '1.0.0',
        variables: r.variables, resource_bindings: r.resource_bindings,
        created_at: r.created_at, created_by: r.owner ?? r.created_by ?? '',
      }));
    } else { error = res.error ?? 'Failed to load agents'; }
    loading = false;
  }

  async function loadTemplates() {
    const res = await execFlow('a3t-templates-list', 'a3t', { org_id: 'org-default' });
    if (res.ok) {
      templates = (Array.isArray(res.data) ? res.data : (res.data?.rows ?? res.data?.output ?? [])).map((r: any) => ({ id: r.id, name: r.name, version: r.version ?? '1.0.0', spec_yaml: r.spec_yaml ?? '' }));
    }
  }

  // ── Registry: everything selectable, no loose strings ──
  const LLM_TYPES = ['openai', 'anthropic', 'groq', 'ollama'];
  let llmResources = $state<{ value: string; label: string }[]>([]);
  let mcpResources = $state<{ value: string; label: string }[]>([]);
  let flowOptions = $state<{ value: string; label: string }[]>([]);
  let triggerOptions = $state<{ value: string; label: string }[]>([]);

  async function loadRegistry() {
    const llm: any[] = [], mcp: any[] = [], fl: any[] = [];
    for (const ns of namespacesFor('shared')) {
      const r = await mcpCall('list_resources', { namespace: ns });
      for (const res of (r.data?.resources ?? [])) {
        const ref = `${res.namespace ?? ns}/${res.name}`;
        if (LLM_TYPES.includes(res.type)) llm.push({ value: `${res.namespace ?? ns}/${res.name}`, label: `${res.name} (${res.type})` });
        if (res.type === 'mcp') mcp.push({ value: ref, label: res.name });
      }
      const f = await mcpCall('list_flows', { namespace: ns });
      for (const w of (f.data?.workflows ?? [])) fl.push({ value: `${w.namespace ?? ns}/${w.name}`, label: `${w.namespace ?? ns}/${w.name}` });
    }
    llmResources = llm; mcpResources = mcp; flowOptions = fl;
    await loadTriggerOptions();
  }

  async function loadTriggerOptions() {
    const t = await mcpCall('list_triggers', {});
    let d: any = t.data; if (typeof d === 'string') { try { d = JSON.parse(d); } catch { d = []; } }
    const list = Array.isArray(d) ? d : (d?.triggers ?? []);
    triggerOptions = list
      .filter((x: any) => x?.config?.path)
      .map((x: any) => ({ value: x.config.path, label: `${x.config.path} → ${x.targets?.[0]?.name ?? '?'}` }));
  }

  function selectAgent(id: string) {
    selected = id;
    propNode = 'detail';
    navigateToDetail('agents', 'Agents', id);
  }

  function closePanel() {
    selected = '';
    creatingNew = false;
    navigateTo('agents', 'Agents');
  }

  async function createAgent() {
    if (!newAgent.name || !newAgent.template_id) return;
    creating = true;
    const id = `agt-${crypto.randomUUID().slice(0, 8)}`;
    const target = newAgent.target || 'a3t/chat-agent';
    // Endpoint: selected existing trigger path, else auto /agent/{id}/chat (created below).
    const usingExisting = !!newAgent.ep_path && triggerOptions.some(t => t.value === newAgent.ep_path);
    const epPath = newAgent.ep_path || `/agent/${id}/chat`;
    const vars = { target, ep_path: epPath, mcp_servers: newAgent.mcp_servers, enabled_tools: newAgent.enabled_tools ?? null };
    const resource_bindings = JSON.stringify({ llm: newAgent.llm });
    const res = await execFlow('a3t-agents-create', 'a3t', {
      org_id: 'org-default', user_id: $currentUser?.id ?? '', timestamp: new Date().toISOString(),
      data: {
        id, name: newAgent.name, namespace: newAgent.namespace,
        template_id: newAgent.template_id, template_version: newAgent.template_version,
        mode: newAgent.mode, resource_bindings,
        variables: JSON.stringify(vars),
      },
    });
    if (res.ok) {
      // Deploy the agent's endpoint (webhook trigger → target flow) unless reusing an existing trigger.
      if (!usingExisting) {
        const [tns, tname] = target.includes('/') ? [target.split('/')[0], target.split('/').slice(1).join('/')] : ['a3t', target];
        await mcpCall('create_trigger', {
          name: `agent-${id}-chat`, namespace: 'a3t',
          config: { type: 'webhook', path: epPath },
          targets: [{ type: 'workflow', namespace: tns, name: tname }],
          enabled: true,
        });
        await loadTriggerOptions();
      }
      creatingNew = false;
      newAgent = { name: '', namespace: newNamespace(), template_id: '', template_version: '1.0.0', mode: 'session', target: '', ep_path: '', llm: 'a3t/claude-cli-max', mcp_servers: [], enabled_tools: null };
      await loadAgents();
      selectAgent(id);
    } else { error = res.error ?? 'Create failed'; }
    creating = false;
  }

  async function deleteAgent() {
    if (!deleteTarget) return;
    const agentId = deleteTarget.id;
    const res = await execFlow('a3t-agents-delete', 'a3t', { id: agentId, org_id: 'org-default' });
    if (res.ok) {
      // Remove the agent's endpoint (webhook trigger), if any
      try {
        const tr = await mcpCall('list_triggers', { namespace: 'a3t' });
        const list = Array.isArray(tr.data) ? tr.data : (tr.data?.triggers ?? []);
        const trig = list.find((t: any) => t?.config?.path === `/agent/${agentId}/chat`);
        if (trig?.id) await mcpCall('delete_trigger', { id: trig.id });
      } catch {}
      selected = ''; deleteTarget = null; await loadAgents();
    }
    else { error = res.error ?? 'Delete failed'; deleteTarget = null; }
  }

  function parseJson(s: string): any {
    try { return JSON.parse(s); } catch { return s; }
  }

  // ── Agent bindings (the "allowed to change" overrides) + read-only designer ──
  let agentTemplate = $derived(agent ? templates.find(t => t.id === agent.template_id) : null);

  // Pull the agent config block out of a template's spec (do[].*.with.config)
  function templateConfig(specYaml: string): any {
    try {
      const doc = YAML.parse(specYaml) ?? {};
      for (const step of (doc.do ?? [])) {
        const k = Object.keys(step)[0];
        const cfg = step?.[k]?.with?.config;
        if (cfg) return cfg;
      }
    } catch {}
    return {};
  }

  let bindings = $state<any>({ llm: '', target: '', ep_path: '', mcp_servers: [], system_prompt: '', max_iterations: 10, temperature: 0.7 });

  // Seed the read-only Designer form with THIS agent's resolved config, since the
  // agent-engine template's `with.config` is a runtime expression (no literal values).
  // The template's LLM dropdown keys on the bare resource name, so strip the namespace.
  let agentConfigSeed = $derived({
    llm_resource: bindings.llm ? String(bindings.llm).split('/').pop() : '',
    system_prompt: bindings.system_prompt ?? '',
    mcp_servers: bindings.mcp_servers ?? [],
    enabled_tools: bindings.enabled_tools ?? undefined,
    max_iterations: bindings.max_iterations,
    temperature: bindings.temperature,
  });
  let savingBind = $state(false);
  let saveBindMsg = $state('');
  let _lastBindId = '';

  function initBindings() {
    if (!agent) return;
    const tpl = agentTemplate?.spec_yaml ? templateConfig(agentTemplate.spec_yaml) : {};
    const rb = parseJson(agent.resource_bindings) || {};
    const vars = parseJson(agent.variables) || {};
    let llm = rb.llm ?? tpl.llm_resource ?? '';
    if (llm && !String(llm).includes('/')) llm = `a3t/${llm}`;
    const mcpRaw = vars.mcp_servers ?? tpl.mcp_servers ?? [];
    const mcp = (Array.isArray(mcpRaw) ? mcpRaw : String(mcpRaw).split(',').map(s => s.trim()).filter(Boolean))
      .map((s: string) => serverRef(s, agent?.namespace || 'a3t'));   // older agents saved bare names
    bindings = {
      llm,
      system_prompt: vars.system_prompt ?? String(tpl.system_prompt ?? '').trim(),
      max_iterations: vars.max_iterations ?? tpl.max_iterations ?? 10,
      temperature: vars.temperature ?? tpl.temperature ?? 0.7,
      mcp_servers: mcp,
      target: vars.target ?? '',
      ep_path: vars.ep_path ?? '',
      // Capability ceiling: an allow-list of tool names this agent may invoke.
      // null = unrestricted (every tool its MCP servers expose); a list, even an
      // empty one, = only those tools — the runtime denies the rest.
      enabled_tools: Array.isArray(vars.enabled_tools) ? [...vars.enabled_tools] : null,
    };
  }

  // The tools of the agent's MCP servers, discovered with list_mcp_tools whenever the
  // selection changes (FlowR's own tools in-process, other servers via tools/list).
  // enabled_tools null = unrestricted; a list (even empty) = the only tools the agent may use.
  let serverTools = $state<ServerTools[]>([]);
  let toolsLoading = $state(false);
  let _toolsKey = '';
  $effect(() => {
    const servers: string[] = (bindings.mcp_servers ?? []).map(String);
    const key = servers.join('|');
    if (key === _toolsKey) return;
    _toolsKey = key;
    if (servers.length === 0) { serverTools = []; toolsLoading = false; return; }
    toolsLoading = true;
    discoverServerTools(servers, agent?.namespace || 'a3t').then(found => {
      if (_toolsKey !== key) return;   // the selection changed meanwhile
      serverTools = found;
      toolsLoading = false;
    });
  });
  let restrictTools = $derived(Array.isArray(bindings.enabled_tools));
  const toolAllowed = (t: string) => !restrictTools || (bindings.enabled_tools ?? []).includes(t);
  // Every tool allowed again → null (unrestricted) rather than the full list.
  function setAllowed(s: Set<string>) {
    bindings.enabled_tools = allToolNames(serverTools).every(t => s.has(t)) ? null : [...s];
  }
  function toggleTool(name: string) {
    // Unrestricted → materialise "everything" before removing one tool.
    const s = new Set<string>(restrictTools ? bindings.enabled_tools : allToolNames(serverTools));
    s.has(name) ? s.delete(name) : s.add(name);
    setAllowed(s);
  }
  function setGroup(tools: string[], on: boolean) {
    const s = new Set<string>(restrictTools ? bindings.enabled_tools : allToolNames(serverTools));
    for (const t of tools) { on ? s.add(t) : s.delete(t); }
    setAllowed(s);
  }

  // Re-init the form once per agent selection (after templates have loaded)
  $effect(() => {
    // Re-init the form when the selected agent changes, once its config actually
    // arrives (agents load async — the panel may open pre-selected before then),
    // and again once templates load. Tracking `agent` (not just `selected`) is
    // what makes it re-run after loadAgents() populates the config.
    const a = agent;
    // Key on the ACTUAL config content (not just its presence), so a re-init
    // fires whenever the binding data lands or changes — even if the async list
    // arrives after the panel is already open. The old `!!present` key could
    // latch empty if the string was present-but-updated. (Deliberately NOT keyed
    // on propNode, so tabbing between nodes never clobbers unsaved edits.)
    const key = a
      ? `${a.id}:${a.resource_bindings ?? ''}:${a.variables ?? ''}:${templates.length > 0}`
      : '';
    if (key && key !== _lastBindId) {
      _lastBindId = key;
      initBindings();
    }
  });

  async function saveBindings() {
    if (!agent) return;
    savingBind = true; saveBindMsg = '';
    const rb = parseJson(agent.resource_bindings) || {};
    rb.llm = bindings.llm;
    const vars = parseJson(agent.variables) || {};
    vars.system_prompt = bindings.system_prompt;
    vars.max_iterations = Number(bindings.max_iterations);
    vars.temperature = Number(bindings.temperature);
    vars.mcp_servers = bindings.mcp_servers;
    vars.target = bindings.target;
    vars.ep_path = bindings.ep_path;
    vars.enabled_tools = bindings.enabled_tools ?? null;
    const res = await execFlow('a3t-agents-update', 'a3t', {
      id: agent.id, org_id: 'org-default', user_id: 'usr-superadmin', timestamp: new Date().toISOString(),
      data: {
        name: agent.name, template_id: agent.template_id, template_version: agent.template_version,
        mode: agent.mode, namespace: agent.namespace, status: agent.status,
        resource_bindings: JSON.stringify(rb), variables: JSON.stringify(vars),
      },
    });
    savingBind = false;
    if (res.ok) { saveBindMsg = 'Saved'; await loadAgents(); setTimeout(() => saveBindMsg = '', 2000); }
    else { saveBindMsg = res.error ?? 'Save failed'; }
  }

  // Inline "+ New endpoint" — create a webhook trigger for this agent → its target flow.
  async function createEndpointForAgent() {
    if (!agent) return;
    const path = `/agent/${agent.id}/chat`;
    const target = bindings.target || 'a3t/chat-agent';
    const [tns, tname] = target.includes('/') ? [target.split('/')[0], target.split('/').slice(1).join('/')] : ['a3t', target];
    await mcpCall('create_trigger', {
      name: `agent-${agent.id}-chat`, namespace: 'a3t',
      config: { type: 'webhook', path },
      targets: [{ type: 'workflow', namespace: tns, name: tname }],
      enabled: true,
    });
    await loadTriggerOptions();
    bindings.ep_path = path;
  }

  // ── Agent → Canvas (deploy): pick a profile preset, clone it bound to this
  //    agent (two-way refs), review/edit in the designer, deploy. ──────────────
  let canvases = $state<any[]>([]);
  let canvasBusy = $state('');
  let canvasMsg = $state('');
  let pickedPreset = $state('');

  let presets = $derived(canvases.filter((c) => !(c.config?.flowRef)));
  let boundCanvas = $derived.by(() => {
    if (!agent) return null;
    const ref = parseJson(agent.variables)?.canvasRef;
    const byRef = ref ? canvases.find((c) => `a3t/${c.name}` === ref || c.name === String(ref).split('/').pop()) : null;
    return byRef ?? canvases.find((c) => c.config?.agentRef === `a3t/${agent.id}`) ?? null;
  });

  async function loadCanvases() {
    const res = await canvasApi.list();
    canvases = (Array.isArray(res.data) ? res.data : (res.data?.resources ?? [])).map((r: any) => ({ name: r.name, namespace: r.namespace, config: r.config ?? {} }));
  }

  function agentTargetFlow(): string {
    return bindings.target || parseJson(agent?.variables)?.target || 'a3t/a3t-chat-agent';
  }

  function openAgentCanvas(id: string) {
    selectAgent(id);
    propNode = 'canvas';
    pickedPreset = ''; canvasMsg = '';
    loadCanvases();
  }

  // Edit = clone the chosen preset into a canvas bound to this agent (agentRef),
  // and write canvasRef back on the agent (two-way). Stays in the Canvas tab.
  async function cloneCanvasForAgent(presetName: string) {
    if (!agent) return;
    const preset = canvases.find((c) => c.name === presetName);
    if (!preset) return;
    canvasBusy = 'cloning'; canvasMsg = '';
    const cname = `${agent.name}-canvas`.toLowerCase().replace(/[^a-z0-9-]/g, '-');
    const cfg = {
      ...structuredClone(preset.config),
      flowRef: agentTargetFlow(),
      agentRef: `a3t/${agent.id}`,
      description: `${agent.name} — from ${preset.name.replace('preset-', '')}`,
    };
    const exists = canvases.some((c) => c.name === cname);
    const res = exists ? await canvasApi.update('a3t', cname, cfg) : await canvasApi.create('a3t', cname, cfg);
    if (!res.ok) { canvasBusy = ''; canvasMsg = res.error || 'clone failed'; return; }
    await setAgentCanvasRef(`a3t/${cname}`);
    await loadCanvases();
    canvasBusy = '';
    canvasMsg = 'Canvas created & bound to this agent.';
  }

  async function setAgentCanvasRef(ref: string) {
    if (!agent) return;
    const rb = parseJson(agent.resource_bindings) || {};
    const vars = parseJson(agent.variables) || {};
    if (ref) vars.canvasRef = ref; else delete vars.canvasRef;
    await execFlow('a3t-agents-update', 'a3t', {
      id: agent.id, org_id: 'org-default', user_id: 'usr-superadmin', timestamp: new Date().toISOString(),
      data: {
        name: agent.name, template_id: agent.template_id, template_version: agent.template_version,
        mode: agent.mode, namespace: agent.namespace, status: agent.status,
        resource_bindings: JSON.stringify(rb), variables: JSON.stringify(vars),
      },
    });
    await loadAgents();
  }

  async function deployBoundCanvas() {
    if (!boundCanvas) { canvasMsg = 'Pick a profile and create the canvas first.'; return; }
    canvasBusy = 'deploying'; canvasMsg = '';
    const res = await canvasApi.deployResource('a3t', boundCanvas.name);
    canvasBusy = '';
    canvasMsg = res.ok && res.data?.deployed
      ? `Deployed → sandbox for ${boundCanvas.config.flowRef}`
      : (res.data?.error || res.error || 'deploy failed');
  }

  onMount(async () => {
    loadAgents();
    loadRegistry();
    loadCanvases();
    await loadTemplates();
    const panel = getActivePanel();
    if (panel?.props?.createFromTemplate) {
      // Deep-linked from a Template's "+ Create Agent": open the create pop-in
      // pre-selected to that template (one template → many agents).
      newAgentPanel();
      newAgent.template_id = panel.props.createFromTemplate;
      onTemplateChange();
    } else if (panel?.props?.selectedId) {
      selected = panel.props.selectedId;
    }
  });
</script>

<!-- Slide Panel -->
<SlidePanel open={!!agent} onclose={closePanel} onsave={() => creatingNew ? createAgent() : saveBindings()} onopenineditor={() => navigateTo('flow-editor', 'Flow Editor')}>
  {#if agent}
    <div class="panel-layout">
      <PropertiesTree
        icon={agent.name.slice(0, 2).toUpperCase() || 'NEW'}
        color="#2980b9"
        name={agent.name || 'New Agent'}
        meta="{agent.mode} · {agent.status}"
        nodes={agent.isNew ? treeNodes.filter(n => n.id === 'detail') : treeNodes}
        activeNode={propNode}
        onselect={(id) => propNode = id}
      />
      <div class="panel-content" class:panel-content-flush={propNode === 'designer'}>
        {#if propNode === 'detail'}
          {#if agent.isNew}
          <div class="pf">
            <div class="pf-head">
              <h3>New Agent</h3>
              <button class="btn spec sm" disabled={creating || !newAgent.name || !newAgent.template_id} onclick={createAgent}>{creating ? 'Creating…' : 'Create Agent'}</button>
            </div>
            <p class="pf-hint">Pick a template, set the mode and bindings, then create. A chat endpoint is auto-created unless you select an existing one.</p>
            <div class="pf-form">
              <label class="cf-field"><span>Name</span><input type="text" bind:value={newAgent.name} placeholder="my-agent" /></label>
              <label class="cf-field"><span>Namespace</span><input type="text" bind:value={newAgent.namespace} /></label>
              <label class="cf-field"><span>Template</span>
                <select bind:value={newAgent.template_id} onchange={onTemplateChange}>
                  <option value="">-- select template --</option>
                  {#each templates as t}<option value={t.id}>{t.name} (v{t.version})</option>{/each}
                </select>
              </label>
              <label class="cf-field"><span>Mode</span>
                <select bind:value={newAgent.mode}>
                  <option value="ephemeral">Ephemeral</option>
                  <option value="session">Session</option>
                  <option value="persistent">Persistent</option>
                  <option value="reactive">Reactive</option>
                </select>
              </label>
              <label class="cf-field"><span>LLM</span>
                <ResourceSelect value={newAgent.llm} options={llmResources} placeholder="— select LLM —" onchange={(v) => newAgent.llm = v} />
              </label>
              <label class="cf-field"><span>Target flow</span>
                <ResourceSelect value={newAgent.target} options={flowOptions} placeholder="— select flow —" onchange={(v) => newAgent.target = v} />
              </label>
              <label class="cf-field"><span>Endpoint</span>
                <ResourceSelect value={newAgent.ep_path} options={triggerOptions} placeholder="+ New endpoint (auto-created)" allowNew={false} onchange={(v) => newAgent.ep_path = v} />
              </label>
              <!-- Checkboxes, not a multi-select: a grey highlight read as "listed", not "selected". -->
              <div class="cf-field full"><span>MCP servers</span>
                <div class="cf-checks">
                  {#each mcpResources as m}
                    <label class="cf-check"><input type="checkbox" value={m.value} bind:group={newAgent.mcp_servers} /> {m.label}</label>
                  {:else}
                    <span class="cf-none">No MCP server resources</span>
                  {/each}
                </div>
              </div>
            </div>
          </div>
          {:else}
          <div class="pf">
            <h3>Agent Detail</h3>
            <div class="pf-kv">
              <div class="pf-row"><span class="pf-k">ID</span><span class="pf-v mono">{agent.id}</span></div>
              <div class="pf-row"><span class="pf-k">Name</span><span class="pf-v">{agent.name}</span></div>
              <div class="pf-row"><span class="pf-k">Namespace</span><span class="pf-v">{agent.namespace}</span></div>
              <div class="pf-row"><span class="pf-k">Mode</span><span class="pf-v">{agent.mode}</span></div>
              <div class="pf-row"><span class="pf-k">Status</span><span class="pf-v">{agent.status}</span></div>
              <div class="pf-row"><span class="pf-k">Template</span><span class="pf-v mono">{agent.template_id}:{agent.template_version}</span></div>
            </div>
          </div>
          {/if}

        {:else if propNode === 'config'}
          <div class="pf">
            <div class="pf-head">
              <h3>Configuration</h3>
              <button class="btn spec sm" disabled={savingBind} onclick={saveBindings}>{savingBind ? 'Saving…' : (saveBindMsg || 'Save')}</button>
            </div>
            <p class="pf-hint">Editable overrides for this agent (LLM, prompt, routing). The full structure is locked in the <strong>Designer</strong> tab, prefilled from the template.</p>
            <div class="pf-form">
              <label class="cf-field"><span>LLM</span>
                <ResourceSelect value={bindings.llm} options={llmResources} placeholder="— select LLM —" onchange={(v) => bindings.llm = v} />
              </label>
              <label class="cf-field"><span>Target flow</span>
                <ResourceSelect value={bindings.target} options={flowOptions} placeholder="— select flow —" onchange={(v) => bindings.target = v} />
              </label>
              <label class="cf-field"><span>Endpoint (trigger)</span>
                <ResourceSelect value={bindings.ep_path} options={triggerOptions} placeholder="— select endpoint —" allowNew={true} newLabel="+ New endpoint" onchange={(v) => bindings.ep_path = v} onnew={createEndpointForAgent} />
              </label>
              <!-- Checkboxes, not a multi-select: a grey highlight read as "listed", not "selected". -->
              <div class="cf-field"><span>MCP servers</span>
                <div class="cf-checks">
                  {#each mcpResources as m}
                    <label class="cf-check"><input type="checkbox" value={m.value} bind:group={bindings.mcp_servers} /> {m.label}</label>
                  {:else}
                    <span class="cf-none">No MCP server resources</span>
                  {/each}
                </div>
              </div>
              <label class="cf-field"><span>Max iterations</span><input type="number" bind:value={bindings.max_iterations} /></label>
              <label class="cf-field"><span>Temperature</span><input type="number" step="0.1" bind:value={bindings.temperature} /></label>
              <label class="cf-field full"><span>System prompt (override)</span><textarea rows="6" bind:value={bindings.system_prompt}></textarea></label>
            </div>
          </div>

        {:else if propNode === 'designer'}
          {#if agentTemplate?.spec_yaml}
            <div class="agt-designer">
              <FlowEditorPane aiKind="agent" aiName={agent?.name ?? ''} initialYaml={agentTemplate.spec_yaml} readOnly configSeed={agentConfigSeed} />
            </div>
          {:else}
            <div class="pf"><p class="pf-hint">No template spec available for this agent.</p></div>
          {/if}

        {:else if propNode === 'template'}
          <div class="pf">
            <h3>Template</h3>
            <div class="pf-kv">
              <div class="pf-row"><span class="pf-k">Template ID</span><span class="pf-v mono">{agent.template_id}</span></div>
              <div class="pf-row"><span class="pf-k">Version</span><span class="pf-v">{agent.template_version}</span></div>
            </div>
            <button class="btn ghost sm" style="margin-top: 12px;" onclick={() => navigateTo('templates', 'Templates')}>View Template</button>
          </div>

        {:else if propNode === 'canvas'}
          <div class="pf">
            <div class="pf-head">
              <h3>Canvas — deploy this agent</h3>
              {#if boundCanvas}<button class="btn spec sm" disabled={canvasBusy === 'deploying'} onclick={deployBoundCanvas}>{canvasBusy === 'deploying' ? 'Deploying…' : 'Deploy'}</button>{/if}
            </div>
            {#if boundCanvas}
              <p class="pf-hint">This agent has its own canvas, cloned from a profile and bound with an <span class="mono">agentRef</span>. Review below, edit in the designer, or deploy.</p>
              <div class="pf-kv">
                <div class="pf-row"><span class="pf-k">Canvas</span><span class="pf-v mono">a3t/{boundCanvas.name}</span></div>
                <div class="pf-row"><span class="pf-k">Flow</span><span class="pf-v mono">{boundCanvas.config.flowRef}</span></div>
                <div class="pf-row"><span class="pf-k">Placement</span><span class="pf-v">{boundCanvas.config.placement?.mode} · {boundCanvas.config.lifecycle?.session}</span></div>
                <div class="pf-row"><span class="pf-k">Agent ref</span><span class="pf-v mono">{boundCanvas.config.agentRef}</span></div>
              </div>
              <div class="perm-actions">
                <button class="btn ghost sm" onclick={() => navigateTo('canvas', 'Canvas', { editCanvas: boundCanvas.name })}>Edit in designer</button>
                <button class="btn ghost sm" onclick={() => setAgentCanvasRef('')}>Change profile</button>
                {#if canvasMsg}<span class="perm-msg">{canvasMsg}</span>{/if}
              </div>
            {:else}
              <p class="pf-hint">Pick a <strong>deployment profile</strong> (a reusable canvas preset). <strong>Edit</strong> clones it into a canvas bound to this agent, which you then refine and deploy.</p>
              <div class="preset-grid">
                {#each presets as p}
                  <button class="preset-card" class:sel={pickedPreset === p.name} onclick={() => (pickedPreset = p.name)}>
                    <strong>{p.name.replace('preset-', '')}</strong>
                    <span class="preset-pills"><span class="ec-tag">{p.config.placement?.mode}</span><span class="ec-tag">{p.config.lifecycle?.session}</span></span>
                    <span class="preset-desc">{p.config.description}</span>
                  </button>
                {/each}
                {#if presets.length === 0}<p class="pf-hint">No canvas profiles found. Create presets on the Canvas page.</p>{/if}
              </div>
              <div class="perm-actions">
                <button class="btn spec sm" disabled={!pickedPreset || canvasBusy === 'cloning'} onclick={() => cloneCanvasForAgent(pickedPreset)}>{canvasBusy === 'cloning' ? 'Creating…' : 'Edit → clone for this agent'}</button>
                {#if canvasMsg}<span class="perm-msg">{canvasMsg}</span>{/if}
              </div>
            {/if}
          </div>

        {:else if propNode === 'policy'}
          <div class="pf">
            <h3>Permissions</h3>
            <p class="pf-hint">Which tools this agent is allowed to use. The policy engine blocks any tool that isn't allowed — even if the model tries to call it.</p>

            <label class="perm-toggle">
              <input type="checkbox" checked={restrictTools}
                onchange={(e) => { bindings.enabled_tools = (e.currentTarget as HTMLInputElement).checked ? allToolNames(serverTools).filter(t => !isDestructive(t)) : null; }} />
              <span><strong>Restrict tools</strong> — {restrictTools ? `${(bindings.enabled_tools ?? []).length} of ${allToolNames(serverTools).length} allowed` : 'off · agent may use every tool below'}</span>
            </label>

            {#if (bindings.mcp_servers ?? []).length === 0}
              <p class="pf-hint">No MCP servers selected. Pick them in <strong>Configuration</strong>; their tools appear here.</p>
            {:else if toolsLoading}
              <p class="pf-hint">Discovering tools from {bindings.mcp_servers.length} MCP server{bindings.mcp_servers.length === 1 ? '' : 's'}…</p>
            {:else}
              <div class="perm-groups">
                {#each serverTools as srv (srv.server)}
                  {#if srv.tools.length === 0}
                    <div class="perm-group">
                      <div class="perm-group-head"><span class="perm-group-name">{srv.name}</span></div>
                      <p class="pf-hint perm-err">No tools listed: {srv.error}</p>
                    </div>
                  {/if}
                  {#each toolSections(srv) as g (g.group)}
                    {@const allOn = g.tools.every(t => toolAllowed(t))}
                    <div class="perm-group">
                      <div class="perm-group-head">
                        <span class="perm-group-name" class:danger={g.group.endsWith('Destructive')}>{g.group} <span class="perm-count">{g.tools.length}</span></span>
                        <button class="perm-all" onclick={() => setGroup(g.tools, !allOn)}>{allOn ? 'Clear' : 'All'}</button>
                      </div>
                      <div class="perm-tools">
                        {#each g.tools as t (t)}
                          <label class="perm-chk" class:on={toolAllowed(t)}>
                            <input type="checkbox" checked={toolAllowed(t)} onchange={() => toggleTool(t)} />
                            <span>{t}</span>
                          </label>
                        {/each}
                      </div>
                    </div>
                  {/each}
                {/each}
              </div>
              <p class="pf-hint" style="margin-top:10px">Unticking a tool turns on the restriction. Destructive tools are additionally blocked by the platform baseline for everyone — allowing one here still won't override that.</p>
            {/if}

            <div class="perm-actions">
              <button class="btn runtime sm" onclick={saveBindings} disabled={savingBind}>{savingBind ? 'Saving…' : 'Save permissions'}</button>
              {#if saveBindMsg}<span class="perm-msg">{saveBindMsg}</span>{/if}
            </div>
          </div>

        {:else if propNode === 'ownership'}
          <div class="pf">
            <h3>Ownership & Access</h3>
            <div class="pf-kv">
              <!-- Ownership is stamped server-side from the authenticated caller.
                   An agent created before that existed genuinely has no owner —
                   say so, rather than substituting a plausible-looking identity.
                   The owner bounds what the agent may be granted, so a wrong one
                   is worse than an absent one. -->
              <div class="pf-row">
                <span class="pf-k">Created by</span>
                <span class="pf-v" class:pf-unset={!agent.created_by}>
                  {agent.created_by || (agent.isNew ? 'you, once saved' : 'unowned')}
                </span>
              </div>
              <div class="pf-row"><span class="pf-k">Org</span><span class="pf-v">org-default</span></div>
              {#if agent.created_at}<div class="pf-row"><span class="pf-k">Created</span><span class="pf-v">{new Date(agent.created_at).toLocaleString()}</span></div>{/if}
              <div class="pf-row"><span class="pf-k">Namespace</span><span class="pf-v">{agent.namespace}</span></div>
            </div>
          </div>
        {/if}
      </div>
    </div>
  {/if}
</SlidePanel>

<!-- Delete Confirmation -->
<DeleteConfirm
  open={!!deleteTarget}
  entityType="Agent"
  entityName={deleteTarget?.name ?? ''}
  onconfirm={deleteAgent}
  oncancel={() => deleteTarget = null}
/>

<!-- Page Header -->
<div class="screen-header">
  <div class="breadcrumb">Agents <span class="sep">/</span> All</div>
  <div class="screen-title-row">
    <div class="title-block">
      <SubmenuToggle appType="agents" />
      <h1 class="screen-title">Agents</h1>
    </div>
    <div class="screen-actions">
      {#if $canCreate}<NewButton label="New agent" accent="runtime" onclick={newAgentPanel} />{/if}
    </div>
  </div>
</div>

<!-- Page Body -->
<div class="screen-body">
  <PageBanner
    headline="Configure an agent."
    highlightWord="agent"
    subtitle="Bind a template to resources. Set mode, memory, and tools."
    description="An agent is a template with frozen inputs — which LLM to use, which memory store, which MCP servers. Configure once, deploy to any Canvas."
    accent="runtime"
    docsPath="/agents"
  />
  <!-- Pinned and latest, under the banner: the section page leads with what
       it is for, then the items you actually return to. -->
  <AppHome appType="agents" embedded />


  <!-- New agents now open in the edit slide-in panel (see newAgentPanel). -->

  {#snippet agentCards(list: any[])}
    <div class="card-grid">
      {#each list as a (a.id)}
        <EntityCard
          icon={a.name.slice(0, 2).toUpperCase()}
          color="#1a7158"
          name={a.name}
          description="from {a.template_id ?? 'no template'}"
          tags={[a.mode, a.status]}
          meta="v{a.template_version}"
          project={a.namespace}
          actionLabel="Deploy to Canvas"
          onclick={() => selectAgent(a.id)}
          onaction={() => openAgentCanvas(a.id)}
          ondelete={() => deleteTarget = { id: a.id, name: a.name }}
        />
      {/each}
    </div>
  {/snippet}

  <FoldSection title="My Agents" count={myAgents.length} hint="Created by you">
    {@render agentCards(myAgents)}
  </FoldSection>
  <FoldSection title="Shared Agents" count={sharedAgents.length} hint="Created by others, in projects you can see">
    {@render agentCards(sharedAgents)}
  </FoldSection>
</div>

<style>
  .screen-header { padding: 18px 32px 12px; border-bottom: 1px solid var(--border); background: var(--bg-page); }
  .breadcrumb { font-size: 12px; color: var(--text-3); margin-bottom: 8px; font-family: var(--font-mono); }
  .breadcrumb .sep { color: var(--text-5); }
  .screen-title-row { display: flex; align-items: center; gap: 18px; }
  .title-block { flex: 1; display: flex; align-items: center; gap: 10px; }
  .screen-title { font-family: var(--font-serif); font-size: 24px; font-weight: 500; letter-spacing: -0.02em; }
  .screen-actions { display: flex; gap: 8px; }
  .screen-body { flex: 1; overflow-y: auto; padding: 20px 32px 40px; }

  .status-msg { padding: 20px; text-align: center; color: var(--text-4); font-size: 14px; }
  .status-msg.err { color: var(--err, #d44); font-family: var(--font-mono); font-size: 12px; }

  .card-grid { display: grid; grid-template-columns: repeat(auto-fill, minmax(300px, 1fr)); gap: 12px; }


  /* Create form */
  .create-form { background: var(--bg-surface); border: 1px solid var(--border); border-radius: var(--r); padding: 20px 24px; margin-bottom: 20px; }
  .create-form h3 { font-family: var(--font-serif); font-size: 16px; font-weight: 500; margin-bottom: 14px; }
  .cf-actions { display: flex; gap: 8px; justify-content: flex-end; margin-top: 16px; }

  /* Panel layout */
  .panel-layout { display: grid; grid-template-columns: 200px 1fr; height: 100%; }
  .panel-content { overflow-y: auto; }
  .panel-content-flush { overflow: hidden; }
  .agt-designer { height: 100%; overflow: hidden; }
  .pf-head { display: flex; align-items: center; justify-content: space-between; gap: 12px; }
  .pf-head h3 { margin-bottom: 0; }
  .pf-form { display: flex; flex-direction: column; gap: 12px; margin-top: 14px; }
  .cf-checks { display: flex; flex-direction: column; gap: 4px; padding: 6px 8px; border: 1px solid var(--border); border-radius: var(--r-sm); background: var(--bg-surface); }
  .cf-check { display: flex; align-items: center; gap: 8px; font-size: 12px; font-family: var(--font-mono); color: var(--text-1); cursor: pointer; }
  .cf-none { font-size: 12px; color: var(--text-3, var(--text-2)); }

  /* Panel forms */
  .pf { padding: 24px 32px; max-width: 600px; }
  .pf h3 { font-family: var(--font-serif); font-size: 18px; font-weight: 500; margin-bottom: 16px; }
  .pf-kv { display: flex; flex-direction: column; }
  .pf-row { display: flex; justify-content: space-between; align-items: center; padding: 9px 0; border-bottom: 1px solid var(--divider); font-size: 13px; gap: 12px; }
  .pf-row:last-child { border-bottom: 0; }
  .pf-k { color: var(--text-3); flex-shrink: 0; }
  .pf-v { color: var(--text-1); text-align: right; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  /* An absent value reads as absent, not as data. */
  .pf-v.pf-unset { color: var(--text-4); font-style: italic; }
  .pf-hint { font-size: 12px; color: var(--text-4); margin-bottom: 12px; }
  .pf-select { width: auto; }

  /* Permissions tab */
  .perm-count { font-weight: 400; opacity: 0.6; margin-left: 4px; }
  .perm-err { color: var(--danger, #b42318); margin: 4px 0 0; }
  .perm-toggle { display: flex; align-items: center; gap: 10px; font-size: 13px; color: var(--text-2);
    padding: 10px 12px; border: 1px solid var(--border); border-radius: var(--r-sm); background: var(--bg-surface); cursor: pointer; }
  .perm-toggle input { width: 15px; height: 15px; accent-color: var(--spec-ink, #5530a8); }
  .perm-toggle strong { color: var(--text-1); }
  .perm-groups { display: flex; flex-direction: column; gap: 14px; margin-top: 14px; }
  .perm-group-head { display: flex; align-items: center; justify-content: space-between; margin-bottom: 8px; }
  .perm-group-name { font-size: 10.5px; font-weight: 600; letter-spacing: 0.08em; text-transform: uppercase; color: var(--text-4); }
  .perm-group-name.danger { color: var(--err, #d44); }
  .perm-all { font-size: 11px; border: none; background: none; color: var(--spec-ink, #5530a8); cursor: pointer; font-weight: 500; }
  .perm-all:hover { text-decoration: underline; }
  .perm-tools { display: flex; flex-wrap: wrap; gap: 6px; }
  .perm-chk { display: inline-flex; align-items: center; gap: 6px; font-family: var(--font-mono); font-size: 11.5px;
    padding: 4px 9px; border: 1px solid var(--border); border-radius: 100px; background: var(--bg-surface);
    color: var(--text-3); cursor: pointer; user-select: none; }
  .perm-chk input { display: none; }
  .perm-chk.on { background: var(--spec-bg, #efeafc); color: var(--spec-ink, #5530a8); border-color: var(--spec-ink, #5530a8); }
  .perm-chk:hover { border-color: var(--border-strong); }
  .perm-actions { display: flex; align-items: center; gap: 12px; margin-top: 18px; }
  .perm-msg { font-size: 12px; color: var(--ok, #1a7158); }

  /* Canvas profile picker */
  .preset-grid { display: grid; grid-template-columns: 1fr; gap: 8px; margin: 8px 0 4px; }
  .preset-card { display: flex; flex-direction: column; gap: 6px; text-align: left; padding: 12px 14px; border: 1px solid var(--border); border-radius: var(--r-sm, 8px); background: var(--bg-surface); cursor: pointer; transition: all 0.12s; }
  .preset-card:hover { border-color: var(--border-strong); }
  .preset-card.sel { border-color: var(--spec, #c47028); background: var(--spec-bg, #fbf0e0); }
  .preset-card strong { font-size: 13px; }
  .preset-pills { display: flex; gap: 4px; }
  .preset-card .ec-tag { font-family: var(--font-mono); font-size: 10px; padding: 1px 6px; border-radius: 3px; background: var(--bg-subtle); color: var(--text-3); }
  .preset-desc { font-size: 11.5px; color: var(--text-3); line-height: 1.45; }
  .pf-section-label { font-size: 10px; font-weight: 600; color: var(--text-2); text-transform: uppercase; letter-spacing: 0.05em; margin: 16px 0 6px; }
  .pf-code { font-family: var(--font-mono); font-size: 12px; line-height: 1.6; padding: 12px 16px; margin: 0; background: var(--bg-subtle); border: 1px solid var(--border); border-radius: var(--r-sm); white-space: pre; overflow: auto; max-height: 200px; }
</style>
