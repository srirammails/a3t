<script lang="ts">
  import SubmenuToggle from '$lib/components/shared/SubmenuToggle.svelte';
  import NewButton from '$lib/components/shared/NewButton.svelte';
  import { execFlow, mcpCall } from '$lib/services/api';
  import DocsPanel from '$lib/shell/DocsPanel.svelte';
  import FlowEditorPane from '$lib/components/editor/FlowEditorPane.svelte';
  import AgentTemplateEditor from '$lib/components/editor/AgentTemplateEditor.svelte';
  import TriggersList from '$lib/components/pages/TriggersList.svelte';
  import McpResourceForm from '$lib/components/editor/McpResourceForm.svelte';
  import SlidePanel from '$lib/components/shared/SlidePanel.svelte';
  import PageBanner from '$lib/components/shared/PageBanner.svelte';
  import AppHome from '$lib/shell/AppHome.svelte';

  // Stacked sub-editors (unlimited depth)
  interface SubEditor { type: string; data?: any; closing?: boolean }
  let subStack = $state<SubEditor[]>([]);

  // Resource creation form state
  let resName = $state('');
  let resType = $state('openai');
  let resNs = $state('default');
  let resConfig = $state('{}');
  let resSaving = $state(false);

  const RES_TYPE_TEMPLATES: Record<string, string> = {
    openai: '{\n  "provider": "openai",\n  "model": "gpt-4o-mini",\n  "api_key": ""\n}',
    anthropic: '{\n  "provider": "anthropic",\n  "model": "claude-sonnet-4-20250514",\n  "api_key": ""\n}',
    groq: '{\n  "provider": "groq",\n  "model": "llama-3.3-70b-versatile",\n  "api_key": "",\n  "api_url": "https://api.groq.com/openai/v1"\n}',
    ollama: '{\n  "provider": "ollama",\n  "model": "llama3",\n  "api_url": "http://localhost:11434/v1"\n}',
    mcp: '{\n  "transport": "sse",\n  "url": "http://localhost:3001/mcp"\n}',
    adb: '{\n  "url": "http://adb:3000"\n}',
  };

  function handleOpenSub(type: string, data?: any) {
    if (type === 'resource-new' && data?.type) {
      resType = data.type;
      resName = '';
      resConfig = RES_TYPE_TEMPLATES[data.type] ?? '{}';
    }
    subStack = [...subStack, { type, data }];
  }

  function closeTopSub() {
    subStack = subStack.slice(0, -1);
  }

  function animateCloseTopSub() {
    if (subStack.length === 0) return;
    const updated = [...subStack];
    updated[updated.length - 1] = { ...updated[updated.length - 1], closing: true };
    subStack = updated;
    setTimeout(closeTopSub, 200);
  }

  // For MCP form — structured config object
  let mcpConfig = $state<Record<string, any>>({});

  async function saveResource() {
    resSaving = true;
    try {
      let config: any;
      if (resType === 'mcp') {
        config = mcpConfig;
      } else {
        try { config = JSON.parse(resConfig); } catch { resSaving = false; return; }
      }
      await mcpCall('create_resource', { name: resName, namespace: resNs, type_ref: resType, config });
      animateCloseTopSub();
    } catch {}
    resSaving = false;
  }
  import { can, canAny } from '$lib/stores/auth';
  import { navigateTo, navigateToDetail, getActivePanel } from '$lib/stores/panels.svelte.ts';
  import { onMount } from 'svelte';

  const canCreate = can('create', 'template');
  // A viewer may open a template read-only but must not save it.
  const canEditTpl = canAny(['create', 'update'], 'template');

  function defaultSpecYaml(name: string): string {
    const n = name || 'my-agent';
    return `document:
  dsl: '1.0.3'
  namespace: a3t
  name: ${n}
  version: '1.0.0'

use:
  extensions:
    - on_turn_start:
        extend: all
        when: "\${ \$task.name == \\"init_turn\\" }"
        before: []
        after: []
    - before_llm:
        extend: call
        before: []
        after: []
    - on_tool_call:
        extend: all
        when: "\${ \$task.name == \\"dispatch_tools\\" }"
        before: []
        after: []

do:
  - run_agent:
      call: a3t:agent
      with:
        config:
          system_prompt: "You are a helpful assistant."
          max_iterations: 10
          max_tokens: 4096
          temperature: 0.7
          tools: []
          tool_flows: {}
          mcp_servers: []
          memory_namespace: "${n}"
          ttl: 1800
        session_id: "\${ .session_id // \\"default\\" }"
        message: "\${ .message }"
`;
  }

  let docsOpen = $state(false);
  let selected = $state('');
  let loading = $state(true);
  let error = $state('');
  let showDetails = $state(false);

  let allTemplates: any[] = $state([]);


  async function loadTemplates() {
    loading = true;
    error = '';
    try {
      console.log('[TemplatesList] loadTemplates() called');
      const res = await execFlow('a3t-templates-list', 'a3t', { org_id: 'org-default' });
      console.log('[TemplatesList] execFlow result:', res.ok, typeof res.data, Array.isArray(res.data) ? `array(${res.data.length})` : res.data);
      if (res.ok) {
        const rows = Array.isArray(res.data) ? res.data : (res.data?.rows ?? res.data?.output ?? []);
        console.log('[TemplatesList] rows to apply:', rows.length);
        applyRows(rows);
      } else {
        error = res.error ?? 'Failed to load templates';
        console.error('[TemplatesList] error:', error);
      }
    } catch (e) {
      error = String(e);
      console.error('[TemplatesList] exception:', e);
    }
    loading = false;
  }

  function applyRows(rows: any[]) {
    allTemplates = rows.map((r: any) => ({
      name: r.name,
      version: r.version ?? 'v1.0',
      activeVersion: r.version ?? 'v1.0',
      versions: [r.version ?? 'v1.0'],
      description: r.spec_yaml ? `Template: ${r.name}` : '',
      icon: r.name.slice(0, 2).toUpperCase(),
      color: '#' + Math.floor(Math.abs(Math.sin(r.name.length) * 0xffffff)).toString(16).padStart(6, '0'),
      tags: r.tags ? (typeof r.tags === 'string' ? r.tags.split(',').map((t: string) => t.trim()).filter(Boolean) : r.tags) : [],
      vertical: r.vertical ?? '',
      usedBy: 0,
      available: true,
      source: ({'system': 'platform', 'platform': 'platform', 'my': 'my', 'training': 'training', 'community': 'community'}[r.source] ?? 'platform') as 'my' | 'training' | 'community' | 'platform',
      id: r.id,
      published: r.published,
      spec_yaml: r.spec_yaml,
      created_at: r.created_at,
    }));
  }


  // Delete confirmation
  let deleteTarget = $state<{ id: string; name: string } | null>(null);
  let deleteConfirmName = $state('');
  let deleting = $state(false);

  /// Agents built from the template being deleted. A template cannot be
  /// deleted while any exist — the server refuses too; this says so up front.
  let deleteUsers = $state<any[] | null>(null);
  async function requestDelete(id: string, name: string) {
    deleteTarget = { id, name };
    deleteConfirmName = '';
    deleteUsers = null;
    const res = await execFlow('a3t-agents-list', 'a3t', { org_id: 'org-default' }, { silent: true });
    const all = res.ok ? (Array.isArray(res.data) ? res.data : (res.data?.rows ?? res.data?.output ?? [])) : [];
    deleteUsers = all.filter((a: any) => a.template_id === id);
  }

  // Agents for the selected template
  let templateAgents: any[] = $state([]);
  let agentsLoading = $state(false);

  async function loadTemplateAgents() {
    agentsLoading = true;
    const res = await execFlow('a3t-agents-list', 'a3t', { org_id: 'org-default' });
    if (res.ok) {
      const allAgents = Array.isArray(res.data) ? res.data : (res.data?.rows ?? res.data?.output ?? []);
      templateAgents = allAgents.filter((a: any) => a.template_id === template?.id);
    }
    agentsLoading = false;
  }

  function cancelDelete() {
    deleteTarget = null;
    deleteConfirmName = '';
  }

  async function confirmDelete() {
    if (!deleteTarget || deleteConfirmName !== deleteTarget.name) return;
    deleting = true;
    const res = await execFlow('a3t-templates-delete', 'a3t', { id: deleteTarget.id, org_id: 'org-default' });
    deleting = false;
    if (res.ok) {
      selected = '';
      deleteTarget = null;
      deleteConfirmName = '';
      await loadTemplates();
    } else {
      error = res.error ?? 'Delete failed';
    }
  }

  onMount(() => {
    loadTemplates();
    // Restore selection from URL
    const panel = getActivePanel();
    if (panel?.props?.selectedId) {
      selected = panel.props.selectedId;
    }
  });

  // Collapsible sections
  let sections = $state({ platform: true, my: true, training: true, community: false });

  let platformTemplates = $derived(allTemplates.filter(t => t.source === 'platform'));
  let myTemplates = $derived(allTemplates.filter(t => t.source === 'my'));
  let trainingTemplates = $derived(allTemplates.filter(t => t.source === 'training'));
  let communityTemplates = $derived(allTemplates.filter(t => t.source === 'community'));

  let tplSections = $derived([
    { key: 'platform', label: 'Platform Templates', hint: 'Built-in A3T templates', items: platformTemplates },
    { key: 'my', label: 'My Templates', hint: '', items: myTemplates },
    { key: 'training', label: 'Training Templates', hint: 'Learn A3T step by step', items: trainingTemplates },
    { key: 'community', label: 'Community Templates', hint: 'Shared by the community', items: communityTemplates },
  ]);

  // When set, the slide-in panel is showing a not-yet-persisted template.
  // The same panel serves both editing an existing template and creating a
  // new one, so `template` resolves to the draft in create mode.
  let draftTemplate = $state<any>(null);
  // While a new template's spec is still auto-generated, keep regenerating it (with the
  // document name) as the user types the name. Flips true once they edit the spec/designer.
  let specEdited = $state(false);
  let template = $derived(draftTemplate ? draftTemplate : (selected ? allTemplates.find(t => t.name === selected) : null));

  let propNode = $state<'detail' | 'yaml' | 'policy' | 'ownership' | 'agents'>('detail');
  let panelClosing = $state(false);

  function selectTemplate(name: string) {
    draftTemplate = null;
    selected = name;
    propNode = 'yaml';
    navigateToDetail('templates', 'Templates', name);
    loadTemplateAgents();
  }

  // Open the edit slide-in on a blank draft. Tags are kept as a comma string
  // for the input; saveTemplate passes them straight to the create flow.
  function newTemplate() {
    selected = '';
    draftTemplate = {
      id: `tpl-${crypto.randomUUID().slice(0, 8)}`,
      name: '', version: '1.0.0', activeVersion: '1.0.0',
      vertical: '', tags: '', source: 'my',
      spec_yaml: defaultSpecYaml(''),
      published: false, usedBy: 0, available: true,
      icon: 'NEW', color: '#c47028', versions: ['1.0.0'], created_at: null,
      isNew: true,
    };
    specEdited = false;
    propNode = 'detail';
  }

  let saving = $state(false);
  let saveMsg = $state('');

  async function saveTemplate() {
    if (!template) return;
    saving = true; saveMsg = '';

    // Create mode: persist the draft, then reopen the panel on the saved row.
    if (draftTemplate?.isNew) {
      if (!draftTemplate.name?.trim()) { saving = false; saveMsg = 'Name required'; return; }
      const res = await execFlow('a3t-templates-create', 'a3t', {
        org_id: 'org-default',
        user_id: 'usr-superadmin',
        timestamp: new Date().toISOString(),
        data: {
          id: draftTemplate.id,
          name: draftTemplate.name.trim(),
          version: draftTemplate.version || '1.0.0',
          source: draftTemplate.source || 'my',
          spec_yaml: draftTemplate.spec_yaml,
          tags: draftTemplate.tags ?? '',
          vertical: draftTemplate.vertical ?? '',
        },
      });
      saving = false;
      if (res.ok) {
        const nm = draftTemplate.name.trim();
        draftTemplate = null;
        await loadTemplates();
        selectTemplate(nm);
      } else {
        saveMsg = res.error ?? 'Create failed';
      }
      return;
    }

    // Agents built from this template take the change: say which before saving.
    if (templateAgents.length > 0) {
      const names = templateAgents.map((a: any) => a.name ?? a.id).slice(0, 10).join(', ');
      const more = templateAgents.length > 10 ? ` and ${templateAgents.length - 10} more` : '';
      if (!confirm(`This template is used by ${templateAgents.length} agent(s): ${names}${more}.\n\nSaving changes what those agents run. Continue?`)) {
        saving = false;
        return;
      }
    }
    const res = await execFlow('a3t-templates-update', 'a3t', {
      id: template.id,
      org_id: 'org-default',
      user_id: 'usr-superadmin',
      timestamp: new Date().toISOString(),
      data: {
        name: template.name,
        version: template.version,
        source: template.source === 'platform' ? 'system' : template.source,
        spec_yaml: template.spec_yaml,
        tags: Array.isArray(template.tags) ? template.tags.join(',') : (template.tags ?? ''),
        vertical: template.vertical ?? '',
      },
    });
    saving = false;
    if (res.ok) {
      saveMsg = 'Saved';
      setTimeout(() => saveMsg = '', 2000);
    } else {
      saveMsg = res.error ?? 'Save failed';
    }
  }

  async function duplicateTemplate() {
    if (!template) return;
    const newName = `${template.name}-copy`;
    const id = `tpl-${crypto.randomUUID().slice(0, 8)}`;
    const res = await execFlow('a3t-templates-create', 'a3t', {
      org_id: 'org-default',
      user_id: 'usr-superadmin',
      timestamp: new Date().toISOString(),
      data: {
        id, name: newName, version: template.version,
        source: 'my', spec_yaml: template.spec_yaml,
        tags: Array.isArray(template.tags) ? template.tags.join(',') : (template.tags ?? ''),
        vertical: template.vertical ?? '',
      },
    });
    if (res.ok) {
      await loadTemplates();
      selectTemplate(newName);
    }
  }

  function closePanel() {
    panelClosing = true;
    setTimeout(() => {
      selected = '';
      draftTemplate = null;
      panelClosing = false;
      navigateTo('templates', 'Templates');
    }, 200);
  }
</script>

<!-- Header is always visible -->
  <div class="screen-header">
    <div class="breadcrumb">Templates <span class="sep">/</span> Browse library</div>
    <div class="screen-title-row">
      <div class="title-block">
        <SubmenuToggle appType="templates" />
        <h1 class="screen-title">Templates</h1>
        <button class="help-btn" onclick={() => docsOpen = true} title="Learn more">
          <svg width="14" height="14" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5"><circle cx="8" cy="8" r="7"/><path d="M6 6a2 2 0 114 0c0 1-1.5 1.5-2 2M8 11.5v.5"/></svg>
        </button>
      </div>
      <div class="screen-actions">
        {#if $canCreate}<NewButton label="New template" accent="spec" onclick={newTemplate} />{/if}
      </div>
    </div>
  </div>
  <div class="screen-body">
    <!-- Slide panel -->
    {#if template}
      <div class="slide-overlay" class:closing={panelClosing} onclick={closePanel}></div>
      <div class="slide-panel" class:closing={panelClosing}>
        <div class="sp-layout">
          <!-- LEFT: Tree menu -->
          <div class="sp-tree-panel">
            <div class="sp-tree-head">
              <div class="sp-tree-icon" style="background: {template.color}">{template.icon}</div>
              <div class="sp-tree-info">
                <div class="sp-tree-name">{template.name ? template.name.split('-').map(w => (w[0]?.toUpperCase() ?? '') + w.slice(1)).join(' ') : 'New Template'}</div>
                <div class="sp-tree-meta">v{template.activeVersion}</div>
              </div>
            </div>
            <nav class="sp-tree">
              <button class="sp-tree-node" class:active={propNode === 'detail'} onclick={() => propNode = 'detail'}>
                <svg width="14" height="14" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5"><rect x="1" y="2" width="5" height="4" rx="1"/><rect x="10" y="2" width="5" height="4" rx="1"/><rect x="5.5" y="10" width="5" height="4" rx="1"/><path d="M3.5 6v2a2 2 0 002 2h5a2 2 0 002-2V6"/><path d="M8 10V8"/></svg>
                Template Detail
              </button>
              <button class="sp-tree-node" class:active={propNode === 'yaml'} onclick={() => propNode = 'yaml'}>
                <svg width="14" height="14" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5"><path d="M5 3L1 8l4 5M11 3l4 5-4 5"/></svg>
                Template YAML
              </button>
              <button class="sp-tree-node" class:active={propNode === 'policy'} onclick={() => propNode = 'policy'}>
                <svg width="14" height="14" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5"><path d="M8 1L2 4v4c0 4 3 6.5 6 7.5 3-1 6-3.5 6-7.5V4z"/></svg>
                Policy
              </button>
              <button class="sp-tree-node" class:active={propNode === 'ownership'} onclick={() => propNode = 'ownership'}>
                <svg width="14" height="14" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5"><circle cx="8" cy="5" r="3"/><path d="M2 14a6 6 0 0112 0"/></svg>
                Ownership &amp; Access
              </button>
              <button class="sp-tree-node" class:active={propNode === 'agents'} onclick={() => propNode = 'agents'}>
                <svg width="14" height="14" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5"><rect x="3" y="1" width="10" height="7" rx="2"/><path d="M5 8v2a3 3 0 006 0V8"/></svg>
                Agents
              </button>
            </nav>
          </div>

          <!-- CENTER: Content + close -->
          <div class="sp-main">
            <div class="sp-close-bar">
              <div class="sp-btn-wrap">
                <button class="sp-action-btn" onclick={() => navigateTo('flow-editor', 'Flow Editor')}>
                  <svg width="14" height="14" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5"><path d="M9 1h6v6M9 7L15 1M7 3H2a1 1 0 00-1 1v10a1 1 0 001 1h10a1 1 0 001-1V9"/></svg>
                </button>
                <span class="sp-tooltip">Open in Flow Editor</span>
              </div>
              {#if $canEditTpl}
              <div class="sp-btn-wrap">
                <button class="sp-action-btn save" onclick={saveTemplate} disabled={saving}>
                  <svg width="14" height="14" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5"><path d="M13 15H3a1 1 0 01-1-1V2a1 1 0 011-1h8l3 3v10a1 1 0 01-1 1z"/><path d="M11 15V9H5v6M5 1v4h5"/></svg>
                </button>
                <span class="sp-tooltip">{saving ? 'Saving...' : saveMsg || 'Save template'}</span>
              </div>
              {/if}
              <div class="sp-btn-wrap">
                <button class="sp-action-btn" onclick={duplicateTemplate}>
                  <svg width="14" height="14" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5"><rect x="4" y="4" width="10" height="10" rx="1"/><path d="M4 12H3a1 1 0 01-1-1V3a1 1 0 011-1h8a1 1 0 011 1v1"/></svg>
                </button>
                <span class="sp-tooltip">Duplicate template</span>
              </div>
              <div class="sp-btn-wrap">
                <button class="sp-close" onclick={closePanel}>
                  <svg width="14" height="14" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="2"><path d="M12 4L4 12M4 4l8 8"/></svg>
                </button>
                <span class="sp-tooltip">Close</span>
              </div>
            </div>
            <div class="sp-content">
              {#if propNode === 'detail'}
                <div class="sp-form">
                  <h3>{template.isNew ? 'New Template' : 'Template Details'}</h3>
                  {#if template.isNew}
                    <p class="sf-hint">Set the name and details, edit the spec in Template YAML, then save.</p>
                    <label class="cf-field"><span>Name</span>
                      <input type="text" bind:value={draftTemplate.name} placeholder="my-agent-template"
                        oninput={() => { if (!specEdited) draftTemplate.spec_yaml = defaultSpecYaml(draftTemplate.name); }} />
                    </label>
                    <label class="cf-field"><span>Version</span><input type="text" bind:value={draftTemplate.version} placeholder="1.0.0" /></label>
                    <label class="cf-field"><span>Vertical</span><input type="text" bind:value={draftTemplate.vertical} placeholder="optional" /></label>
                    <label class="cf-field"><span>Tags</span><input type="text" bind:value={draftTemplate.tags} placeholder="comma,separated" /></label>
                    <label class="cf-field"><span>Source</span>
                      <select bind:value={draftTemplate.source}>
                        <option value="my">my</option>
                        <option value="training">training</option>
                        <option value="community">community</option>
                      </select>
                    </label>
                  {:else}
                    <div class="sf-kv">
                      <div class="sf-row"><span class="sf-k">ID</span><span class="sf-v mono">{template.id}</span></div>
                      <div class="sf-row"><span class="sf-k">Name</span><span class="sf-v">{template.name}</span></div>
                      <div class="sf-row"><span class="sf-k">Version</span><span class="sf-v">{template.activeVersion}</span></div>
                      <div class="sf-row"><span class="sf-k">Vertical</span><span class="sf-v">{template.vertical || '-'}</span></div>
                      <div class="sf-row"><span class="sf-k">Source</span><span class="sf-v">{template.source}</span></div>
                      <div class="sf-row"><span class="sf-k">Published</span><span class="sf-v">{template.published ?? 'false'}</span></div>
                      <div class="sf-row"><span class="sf-k">Tags</span><span class="sf-v">{Array.isArray(template.tags) ? template.tags.join(', ') : (template.tags || '-')}</span></div>
                    </div>
                  {/if}
                </div>

              {:else if propNode === 'yaml'}
                {#if template.spec_yaml}
                  <FlowEditorPane aiKind="template" aiName={template.name} initialYaml={template.spec_yaml} onchange={(yaml) => { if (template) template.spec_yaml = yaml; if (draftTemplate?.isNew && yaml.trim() !== defaultSpecYaml(draftTemplate.name).trim()) specEdited = true; }} onopensub={handleOpenSub} />
                {:else}
                  <div class="sp-empty">No spec defined for this template.</div>
                {/if}

              {:else if propNode === 'agents'}
                <div class="sp-form">
                  <div class="pf-head">
                    <h3>Agents</h3>
                    {#if template?.id}<button class="btn spec sm" onclick={() => navigateTo('agents', 'Agents', { createFromTemplate: template.id })}>+ Create Agent</button>{/if}
                  </div>
                  <p class="sf-hint">Agents instantiated from this template — one template, many agents.</p>
                  {#if agentsLoading}
                    <div class="sf-placeholder">Loading agents...</div>
                  {:else if templateAgents.length === 0}
                    <div class="sf-placeholder">No agents created from this template yet.</div>
                  {:else}
                    <div class="sf-agents-list">
                      {#each templateAgents as a}
                        <div class="sf-agent-row">
                          <div class="sf-agent-info">
                            <strong>{a.name}</strong>
                            <span class="mono dim" style="font-size: 11px;">{a.id}</span>
                          </div>
                          <span class="sf-agent-mode">{a.mode}</span>
                          <span class="sf-agent-status" class:active={a.status === 'running'} class:draft={a.status === 'draft'}>{a.status}</span>
                        </div>
                      {/each}
                    </div>
                  {/if}
                </div>

              {:else if propNode === 'ownership'}
                <div class="sp-form">
                  <h3>Ownership &amp; Access</h3>
                  <div class="sf-kv">
                    <div class="sf-row"><span class="sf-k">Created by</span><span class="sf-v">usr-superadmin</span></div>
                    <div class="sf-row"><span class="sf-k">Org</span><span class="sf-v">org-default</span></div>
                    {#if template.created_at}<div class="sf-row"><span class="sf-k">Created</span><span class="sf-v">{new Date(template.created_at).toLocaleString()}</span></div>{/if}
                    <div class="sf-row"><span class="sf-k">Namespace</span><span class="sf-v">a3t</span></div>
                  </div>
                </div>

              {:else if propNode === 'policy'}
                <div class="sp-form">
                  <h3>Policy</h3>
                  <p class="sf-hint">Security and runtime policy for agents using this template.</p>
                  <div class="sf-kv">
                    <div class="sf-row"><span class="sf-k">Preset</span><select class="sf-select"><option value="standard" selected>standard</option><option value="restricted">restricted</option><option value="elevated">elevated</option></select></div>
                    <div class="sf-row"><span class="sf-k">Runtime</span><select class="sf-select"><option value="sandboxed" selected>sandboxed</option><option value="co-located">co-located</option></select></div>
                    <div class="sf-row"><span class="sf-k">Audit</span><select class="sf-select"><option value="all_decisions" selected>all_decisions</option><option value="failures_only">failures_only</option><option value="none">none</option></select></div>
                    <div class="sf-row"><span class="sf-k">Max Flow Depth</span><span class="sf-v">3</span></div>
                    <div class="sf-row"><span class="sf-k">Memory Scopes</span><span class="sf-v">private, shared</span></div>
                  </div>
                  <div class="sf-note">Policy intersected with ClusterPolicy and PolicyPreset.</div>
                </div>
              {/if}
            </div>
          </div>
        </div>
      </div>
    {/if}

    <PageBanner
      headline="Start with a template."
      highlightWord="template"
      subtitle="Give it memory, knowledge, and tools. Wire it into the Canvas."
      description="Every A3T agent begins as a typed template. Templates encode domain expertise — bid pipelines, incident triage, payment review — so you configure rather than engineer."
      accent="spec"
      docsPath="/templates"
    />
    <!-- Pinned and latest, under the banner: the section page leads with what
         it is for, then the items you actually return to. -->
    <AppHome appType="templates" embedded />


    <!-- New templates now open in the edit slide-in panel (see newTemplate). -->

    <!-- Template sections — all always visible, folded when empty -->
    {#each tplSections as sec}
      <div class="tpl-group" class:tpl-empty={sec.items.length === 0}>
        <button class="tpl-group-header" onclick={() => { if (sec.items.length > 0) sections[sec.key] = !sections[sec.key]; }}>
          <svg class="tpl-chevron" class:open={sections[sec.key] && sec.items.length > 0} width="12" height="12" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="2"><path d="M6 4l4 4-4 4"/></svg>
          <h3>{sec.label}</h3>
          <span class="tpl-group-count">{sec.items.length}</span>
          {#if sec.hint}<span class="tpl-group-hint">{sec.hint}</span>{/if}
        </button>
        {#if sections[sec.key] && sec.items.length > 0}
          <div class="card-grid">
            {#each sec.items as t}
              <div class="template-card">
                <button class="tc-delete" title="Delete" onclick={() => requestDelete(t.id, t.name)}>
                  <svg width="12" height="12" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5"><path d="M4 4l8 8M12 4l-8 8"/></svg>
                </button>
                <div class="tc-click" role="button" tabindex="0" onclick={() => selectTemplate(t.name)} onkeydown={(e) => { if (e.key === 'Enter') selectTemplate(t.name); }}>
                  <div class="tc-icon" style="background: {t.color}">{t.icon}</div>
                  <div class="tc-body">
                    <div class="tc-name">{t.name.split('-').map(w => w[0].toUpperCase() + w.slice(1)).join(' ')}</div>
                    <div class="tc-desc">{t.description}</div>
                    <div class="tc-tags">{#each t.tags as tag}<span class="badge mut mono" style="font-size: 10px;">{tag}</span>{/each}</div>
                    <div class="tc-footer">
                      <span class="mono dim" style="font-size: 11px;">v{t.activeVersion}</span>
                      {#if t.vertical}<span class="dim" style="font-size: 11px;">&middot; {t.vertical}</span>{/if}
                    </div>
                  </div>
                </div>
                <button class="tc-create-agent" onclick={(e) => { e.stopPropagation(); navigateTo('agents', 'Agents', { createFromTemplate: t.id }); }}>+ Create Agent</button>
              </div>
            {/each}
          </div>
        {/if}
      </div>
    {/each}
  </div>

<!-- Delete confirmation dialog -->
{#if deleteTarget}
  <div class="del-overlay" onclick={cancelDelete}></div>
  <div class="del-dialog">
    <div class="del-dialog-head">
      <svg width="20" height="20" viewBox="0 0 16 16" fill="none" stroke="var(--err, #d44)" stroke-width="1.5"><path d="M8 1L2 4v4c0 4 3 6.5 6 7.5 3-1 6-3.5 6-7.5V4z"/><path d="M8 5v4M8 11h.01" stroke-width="2" stroke-linecap="round"/></svg>
      <h3>Delete Template</h3>
    </div>
    {#if deleteUsers === null}
      <p class="del-msg">Checking which agents use it…</p>
    {:else if deleteUsers.length > 0}
      <p class="del-msg"><strong>{deleteTarget.name}</strong> can’t be deleted while agents are built from it. Delete these {deleteUsers.length} agent(s) first:</p>
      <ul class="del-users">
        {#each deleteUsers as a}<li>{a.name ?? a.id}</li>{/each}
      </ul>
    {:else}
      <p class="del-msg">This will permanently delete <strong>{deleteTarget.name}</strong> and cannot be undone.</p>
      <p class="del-msg">Type the template name to confirm:</p>
      <input class="del-input" type="text" bind:value={deleteConfirmName} placeholder={deleteTarget.name} />
    {/if}
    {#if error && deleteTarget}<p class="del-msg" style="color: var(--err)">{error}</p>{/if}
    <div class="del-actions">
      <button class="btn ghost" onclick={cancelDelete}>Cancel</button>
      <button class="btn danger" disabled={deleting || !deleteUsers || deleteUsers.length > 0 || deleteConfirmName !== deleteTarget.name} onclick={confirmDelete}>
        {deleting ? 'Deleting...' : 'Delete Template'}
      </button>
    </div>
  </div>
{/if}

<DocsPanel open={docsOpen} pageKey="templates" onClose={() => docsOpen = false} />

<!-- Stacked sub-editors — each slides in 10px further right -->
{#each subStack as sub, idx}
  {#if idx === subStack.length - 1}
    <div class="sub-overlay" class:sub-closing={sub.closing} onclick={animateCloseTopSub}></div>
  {/if}
  <div class="sub-slide" class:sub-closing={sub.closing} style="left: {140 + idx * 10}px; z-index: {201 + idx * 2};">
    <div class="sub-slide-bar">
      {#if sub.type === 'resource-new'}
        <button class="sub-slide-save" onclick={saveResource} disabled={!resName || resSaving}>
          {resSaving ? 'Creating...' : 'Create'}
        </button>
      {/if}
      <button class="sub-slide-close" onclick={animateCloseTopSub}>
        <svg width="14" height="14" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="2"><path d="M12 4L4 12M4 4l8 8"/></svg>
      </button>
    </div>
    <div class="sub-slide-title">
      {#if sub.type === 'resource-new'}New {sub.data?.label ?? 'Resource'}
      {:else if sub.type === 'trigger-new'}New Trigger
      {:else if sub.type === 'trigger'}Edit Trigger
      {:else}{sub.type}{/if}
    </div>
    <div class="sub-slide-body">
      {#if sub.type === 'trigger-new' || sub.type === 'trigger'}
        <TriggersList />
      {:else if sub.type === 'resource-new'}
        <div class="sub-res-form">
          <div class="sub-res-row2">
            <label>Name <input type="text" bind:value={resName} placeholder="my-resource" /></label>
            <label>Namespace <input type="text" bind:value={resNs} /></label>
          </div>
          {#if !sub.data?.type}
            <label>Type
              <select bind:value={resType} onchange={() => { resConfig = RES_TYPE_TEMPLATES[resType] ?? '{}'; mcpConfig = {}; }}>
                <option value="openai">OpenAI</option>
                <option value="anthropic">Anthropic</option>
                <option value="groq">Groq</option>
                <option value="ollama">Ollama</option>
                <option value="mcp">MCP Server</option>
                <option value="adb">ADB (Agent Memory)</option>
                <option value="lancedb">LanceDB</option>
                <option value="http">HTTP API</option>
                <option value="redis">Redis</option>
              </select>
            </label>
          {:else}
            <div class="sub-res-type-badge">{sub.data.label ?? sub.data.type}</div>
          {/if}

          {#if resType === 'mcp'}
            <McpResourceForm value={mcpConfig} resourceName={resName} onchange={(cfg) => { mcpConfig = cfg; }} />
          {:else}
            <label>Configuration (JSON)
              <textarea class="sub-res-config" rows="12" bind:value={resConfig} spellcheck="false"></textarea>
            </label>
          {/if}
        </div>
      {:else}
        <div style="padding: 20px; color: var(--text-4);">Unknown: {sub.type}</div>
      {/if}
    </div>
  </div>
{/each}

<style>
  /* Stacked sub-editor — slides in like SlidePanel but offset 10px right */
  .sub-overlay {
    position: fixed; inset: 0; background: rgba(0,0,0,0.08);
    z-index: 200; animation: subOverIn 0.2s ease-out;
  }
  .sub-overlay.sub-closing { animation: subOverOut 0.2s ease-in forwards; }
  @keyframes subOverIn { from { opacity: 0; } to { opacity: 1; } }
  @keyframes subOverOut { from { opacity: 1; } to { opacity: 0; } }

  .sub-slide {
    position: fixed; top: 42px; right: 6px; bottom: 6px;
    background: var(--bg-page); border: 1px solid var(--border); border-radius: 14px;
    z-index: 201; overflow: hidden;
    box-shadow: 0 8px 40px rgba(0,0,0,0.14), -4px 0 20px rgba(0,0,0,0.06);
    animation: subSlideIn 0.25s cubic-bezier(0.16, 1, 0.3, 1);
    display: flex; flex-direction: column;
  }
  .sub-slide.sub-closing { animation: subSlideOut 0.2s ease-in forwards; }
  @keyframes subSlideIn { from { transform: translateX(100%); } to { transform: translateX(0); } }
  @keyframes subSlideOut { from { transform: translateX(0); } to { transform: translateX(100%); } }

  .sub-slide-bar {
    display: flex; align-items: center; justify-content: flex-end; gap: 6px;
    padding: 6px 12px; height: 44px; flex-shrink: 0;
    background: var(--bg-page); border-bottom: 1px solid var(--border);
  }
  .sub-slide-close {
    width: 32px; height: 32px; border: 1px solid var(--border); border-radius: 50%;
    background: var(--bg-surface); color: var(--text-3); cursor: pointer;
    display: grid; place-items: center; transition: all 0.12s;
  }
  .sub-slide-close:hover { background: var(--text-1); color: var(--bg-page); border-color: var(--text-1); }
  .sub-slide-save {
    padding: 5px 14px; border: 1px solid var(--runtime); border-radius: 5px;
    background: var(--runtime); color: white; font-size: 12px; font-weight: 600; cursor: pointer;
  }
  .sub-slide-save:hover:not(:disabled) { opacity: 0.9; }
  .sub-slide-save:disabled { opacity: 0.4; cursor: default; }

  .sub-slide-title {
    font-family: var(--font-serif); font-size: 20px; font-weight: 500;
    color: var(--text-1); padding: 12px 20px 8px;
  }
  .sub-slide-body { flex: 1; overflow: auto; }

  .sub-res-form { padding: 20px 24px; display: flex; flex-direction: column; gap: 10px; }
  .sub-res-form label { display: flex; flex-direction: column; gap: 4px; font-size: 12px; font-weight: 500; color: var(--text-2); }
  .sub-res-form input, .sub-res-form select { padding: 8px 10px; border: 1px solid var(--border); border-radius: 4px; background: var(--bg-surface); color: var(--text-1); font-size: 13px; font-family: var(--font-mono); outline: none; }
  .sub-res-form input:focus, .sub-res-form select:focus { border-color: var(--spec); }
  .sub-res-row2 { display: grid; grid-template-columns: 1fr 1fr; gap: 10px; }
  .sub-res-config { width: 100%; padding: 10px; border: 1px solid var(--border); border-radius: 4px; background: var(--bg-page); color: var(--text-1); font-family: var(--font-mono); font-size: 13px; outline: none; resize: vertical; }
  .sub-res-config:focus { border-color: var(--spec); }
  .sub-res-type-badge { font-family: var(--font-mono); font-size: 12px; font-weight: 600; color: var(--spec-ink); background: var(--spec-bg); padding: 6px 12px; border-radius: 4px; display: inline-block; }
  /* Cards */
  .screen-header { padding: 18px 32px 12px; border-bottom: 1px solid var(--border); background: var(--bg-page); }
  .breadcrumb { font-size: 12px; color: var(--text-3); margin-bottom: 8px; font-family: var(--font-mono); }
  .breadcrumb .sep { color: var(--text-5); }
  .screen-title-row { display: flex; align-items: center; gap: 18px; }
  .title-block { flex: 1; display: flex; align-items: center; gap: 10px; }
  .screen-title { font-family: var(--font-serif); font-size: 24px; font-weight: 500; letter-spacing: -0.02em; }
  .help-btn { width: 24px; height: 24px; border: 1px solid var(--border); border-radius: 50%; background: var(--bg-surface); color: var(--text-4); display: grid; place-items: center; cursor: pointer; }
  .help-btn:hover { background: var(--spec-bg); border-color: var(--spec); color: var(--spec); }
  .screen-actions { display: flex; gap: 8px; }
  .screen-body { flex: 1; overflow-y: auto; padding: 20px 32px 40px; position: relative; }
  .status-msg { padding: 20px; text-align: center; color: var(--text-4); font-size: 14px; }
  .status-msg.err { color: var(--err, #d44); font-family: var(--font-mono); font-size: 12px; white-space: pre-wrap; }

  .create-form { background: var(--bg-surface); border: 1px solid var(--border); border-radius: var(--r); padding: 20px 24px; margin-bottom: 20px; }
  .create-form h3 { font-family: var(--font-serif); font-size: 16px; font-weight: 500; margin-bottom: 14px; }
  .cf-actions { display: flex; gap: 8px; justify-content: flex-end; margin-top: 16px; }

  .tpl-group { margin-bottom: 24px; }
  .tpl-group-header {
    display: flex; align-items: center; gap: 10px; width: 100%;
    padding: 0 0 10px; border: none; background: none; cursor: pointer; text-align: left;
  }
  .tpl-group-header h3 { font-family: var(--font-serif); font-size: 16px; font-weight: 500; color: var(--text-1); }
  .tpl-group-count { font-family: var(--font-mono); font-size: 11px; color: var(--text-4); background: var(--bg-subtle); padding: 1px 6px; border-radius: 3px; }
  .tpl-group-hint { font-size: 11px; color: var(--text-4); font-style: italic; margin-left: auto; }
  .tpl-chevron { color: var(--text-4); transition: transform 0.2s; flex-shrink: 0; }
  .tpl-chevron.open { transform: rotate(90deg); }
  .tpl-empty { opacity: 0.5; }
  .tpl-empty .tpl-group-header { cursor: default; }
  .tpl-empty .tpl-chevron { opacity: 0; }

  .card-grid { display: grid; grid-template-columns: repeat(auto-fill, minmax(300px, 1fr)); gap: 12px; }
  .template-card { display: flex; flex-direction: column; background: var(--bg-surface); border: 1px solid var(--border); border-radius: var(--r); box-shadow: var(--shadow-sm); transition: all 0.15s; overflow: hidden; position: relative; }
  .template-card:hover { box-shadow: var(--shadow-lg); border-color: var(--border-strong); transform: translateY(-1px); }
  .tc-click { flex: 1; display: grid; grid-template-columns: 40px 1fr; gap: 14px; padding: 16px 18px 12px; cursor: pointer; text-align: left; align-content: start; }
  .tc-icon { width: 40px; height: 40px; border-radius: var(--r-sm); color: white; display: grid; place-items: center; font-size: 12px; font-weight: 700; font-family: var(--font-mono); }
  .tc-body { display: flex; flex-direction: column; gap: 5px; min-width: 0; }
  .tc-name { font-size: 14px; font-weight: 550; }
  .tc-desc { font-size: 12px; color: var(--text-2); line-height: 1.4; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .tc-tags { display: flex; flex-wrap: wrap; gap: 3px; }
  .tc-footer { display: flex; align-items: center; gap: 6px; margin-top: 2px; }
  .tc-delete {
    position: absolute; top: 8px; right: 8px;
    width: 24px; height: 24px; border: 1px solid transparent; border-radius: 4px;
    background: none; color: var(--text-5); cursor: pointer; display: grid; place-items: center;
    opacity: 0; transition: opacity 0.1s;
  }
  .template-card:hover .tc-delete { opacity: 1; }
  .tc-delete:hover { background: var(--err-bg, #fde8e8); color: var(--err, #d44); border-color: var(--err); }
  .tc-create-agent {
    display: block; width: 100%; padding: 8px; border: none;
    border-top: 1px solid var(--divider); background: none;
    font-size: 12px; font-weight: 500; color: var(--text-3);
    cursor: pointer; transition: all 0.12s;
  }
  .tc-create-agent:hover { background: var(--spec-bg, #fbf0e0); color: var(--spec, #c47028); }

  /* Slide panel — 3D popup effect */
  .slide-overlay {
    position: fixed; inset: 0;
    background: rgba(0,0,0,0.12);
    z-index: 100;
    animation: overlayIn 0.2s ease-out;
  }
  .slide-overlay.closing { animation: overlayOut 0.2s ease-in forwards; }
  @keyframes overlayIn { from { opacity: 0; } to { opacity: 1; } }
  @keyframes overlayOut { from { opacity: 1; } to { opacity: 0; } }

  .slide-panel {
    position: fixed;
    top: 42px; left: 110px; right: 6px; bottom: 6px;
    background: var(--bg-page);
    border: 1px solid var(--border);
    border-radius: 14px;
    z-index: 101;
    overflow: hidden;
    box-shadow:
      0 8px 40px rgba(0,0,0,0.14),
      0 2px 10px rgba(0,0,0,0.06),
      -4px 0 20px rgba(0,0,0,0.04);
    animation: panelIn 0.25s cubic-bezier(0.16, 1, 0.3, 1);
  }
  .slide-panel.closing {
    animation: panelOut 0.2s ease-in forwards;
  }
  @keyframes panelIn {
    from { transform: translateX(100%); }
    to { transform: translateX(0); }
  }
  @keyframes panelOut {
    from { transform: translateX(0); }
    to { transform: translateX(100%); }
  }

  /* Layout: left tree + main content */
  .sp-layout { display: grid; grid-template-columns: 200px 1fr; height: 100%; }

  /* Left: Tree panel */
  .sp-tree-panel {
    display: flex; flex-direction: column; background: var(--bg-surface);
    border-right: 1px solid var(--border); overflow-y: auto;
  }
  .sp-tree-head {
    display: flex; align-items: center; gap: 10px;
    padding: 12px 14px; border-bottom: 1px solid var(--border);
  }
  .sp-tree-icon { width: 28px; height: 28px; border-radius: 6px; display: grid; place-items: center; font-family: var(--font-mono); font-size: 10px; font-weight: 700; color: white; flex-shrink: 0; }
  .sp-tree-info { min-width: 0; }
  .sp-tree-name { font-family: var(--font-serif); font-size: 13px; font-weight: 500; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .sp-tree-meta { font-size: 10px; color: var(--text-4); margin-top: 1px; }

  .sp-tree { display: flex; flex-direction: column; padding: 6px 0; flex: 1; }
  .sp-tree-node {
    display: flex; align-items: center; gap: 8px;
    padding: 7px 14px; border: none; background: none;
    cursor: pointer; font-size: 12px; color: var(--text-3);
    text-align: left; transition: all 0.1s;
  }
  .sp-tree-node:hover { color: var(--text-1); background: var(--bg-subtle); }
  .sp-tree-node.active {
    color: var(--text-1); font-weight: 550;
    background: var(--bg-subtle);
    box-shadow: inset 3px 0 0 var(--spec, #c47028);
  }
  .sp-tree-node svg { color: var(--text-4); flex-shrink: 0; }
  .sp-tree-node.active svg { color: var(--spec, #c47028); }

  .sp-tree-tags { padding: 10px 14px; border-top: 1px solid var(--border); display: flex; flex-wrap: wrap; gap: 3px; margin-top: auto; }
  .tpl-tag { font-family: var(--font-mono); font-size: 9px; padding: 2px 6px; border-radius: 3px; background: var(--bg-subtle); color: var(--text-3); }

  /* Main area */
  .sp-main { overflow: hidden; position: relative; padding-top: 44px; }
  .sp-close-bar {
    position: absolute; top: 0; left: 0; right: 0; height: 44px;
    display: flex; align-items: center; justify-content: flex-end; gap: 6px;
    padding: 0 12px; z-index: 10;
    background: var(--bg-page);
    border-bottom: 1px solid var(--border);
  }
  .sp-btn-wrap { position: relative; }
  .sp-tooltip {
    position: absolute; bottom: -30px; left: 50%; transform: translateX(-50%);
    padding: 4px 10px; background: var(--text-1); color: var(--bg-page);
    font-size: 11px; font-weight: 500; white-space: nowrap;
    border-radius: 5px; pointer-events: none;
    opacity: 0; transition: opacity 0.15s 0.3s;
    z-index: 20;
  }
  .sp-tooltip::before {
    content: ''; position: absolute; bottom: 100%; left: 50%; transform: translateX(-50%);
    border: 4px solid transparent; border-bottom-color: var(--text-1);
  }
  .sp-btn-wrap:hover .sp-tooltip { opacity: 1; }

  .sp-action-btn {
    width: 32px; height: 32px;
    border: 1px solid var(--border); border-radius: 50%;
    background: var(--bg-surface); color: var(--text-3);
    cursor: pointer; display: grid; place-items: center;
    transition: all 0.12s;
  }
  .sp-action-btn:hover {
    background: var(--bg-subtle); color: var(--text-1);
    border-color: var(--border-strong);
  }
  .sp-action-btn.save:hover {
    background: var(--ok-bg, #e3f4ef); color: var(--ok, #23987f);
    border-color: var(--ok);
  }
  .sp-close {
    width: 32px; height: 32px;
    border: 1px solid var(--border); border-radius: 50%;
    background: var(--bg-surface); color: var(--text-3);
    cursor: pointer; display: grid; place-items: center;
    transition: all 0.12s;
  }
  .sp-close:hover {
    background: var(--text-1); color: var(--bg-page);
    border-color: var(--text-1);
  }

  .sp-content { height: 100%; overflow: hidden; }
  .sp-empty { padding: 40px; text-align: center; color: var(--text-4); font-size: 13px; }

  /* Form views */
  .sp-form { padding: 24px 32px; max-width: 600px; }
  .sp-form h3 { font-family: var(--font-serif); font-size: 18px; font-weight: 500; margin-bottom: 16px; }
  .sf-kv { display: flex; flex-direction: column; }
  .sf-row { display: flex; justify-content: space-between; align-items: center; padding: 9px 0; border-bottom: 1px solid var(--divider); font-size: 13px; gap: 12px; }
  .sf-row:last-child { border-bottom: 0; }
  .sf-k { color: var(--text-3); flex-shrink: 0; }
  .sf-v { color: var(--text-1); text-align: right; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .sf-select { padding: 4px 8px; border: 1px solid var(--border); border-radius: 4px; background: var(--bg-surface); color: var(--text-1); font-size: 12px; font-family: var(--font-mono); }
  .sf-hint { font-size: 12px; color: var(--text-4); margin-bottom: 12px; }
  .pf-head { display: flex; align-items: center; justify-content: space-between; gap: 12px; margin-bottom: 4px; }
  .pf-head h3 { margin-bottom: 0; }
  .btn.sm { padding: 5px 11px; font-size: 12px; }
  .sf-placeholder { font-size: 12px; color: var(--text-4); padding: 20px; text-align: center; background: var(--bg-subtle); border-radius: var(--r-sm); }
  .sf-agents-list { display: flex; flex-direction: column; gap: 4px; }
  .sf-agent-row { display: flex; align-items: center; gap: 12px; padding: 10px 14px; background: var(--bg-surface); border: 1px solid var(--border); border-radius: var(--r-sm); }
  .sf-agent-info { flex: 1; display: flex; flex-direction: column; gap: 1px; min-width: 0; }
  .sf-agent-info strong { font-size: 13px; }
  .sf-agent-mode { font-family: var(--font-mono); font-size: 10px; color: var(--text-3); padding: 2px 6px; background: var(--bg-subtle); border-radius: 3px; }
  .sf-agent-status { font-size: 10px; font-weight: 550; padding: 2px 8px; border-radius: 3px; }
  .sf-agent-status.draft { color: var(--text-3); background: var(--bg-subtle); }
  .sf-agent-status.active { color: var(--ok); background: var(--ok-bg); }
  .sf-note { font-size: 10px; color: var(--text-4); margin-top: 14px; padding: 8px 10px; background: var(--bg-subtle); border-radius: var(--r-sm); line-height: 1.5; }

  /* Delete confirmation dialog */
  .del-overlay {
    position: fixed; inset: 0; background: rgba(0,0,0,0.3); z-index: 200;
    animation: overlayIn 0.15s ease-out;
  }
  .del-dialog {
    position: fixed; top: 50%; left: 50%; transform: translate(-50%, -50%);
    background: var(--bg-surface); border: 1px solid var(--border);
    border-radius: 12px; padding: 24px 28px; z-index: 201;
    width: 400px; max-width: 90vw;
    box-shadow: 0 12px 40px rgba(0,0,0,0.15), 0 2px 8px rgba(0,0,0,0.08);
    animation: dialogIn 0.2s cubic-bezier(0.16, 1, 0.3, 1);
  }
  @keyframes dialogIn { from { transform: translate(-50%, -48%) scale(0.96); opacity: 0; } to { transform: translate(-50%, -50%) scale(1); opacity: 1; } }
  .del-dialog-head { display: flex; align-items: center; gap: 10px; margin-bottom: 14px; }
  .del-dialog-head h3 { font-family: var(--font-serif); font-size: 16px; font-weight: 500; }
  .del-msg { font-size: 13px; color: var(--text-2); line-height: 1.5; margin-bottom: 10px; }
  .del-msg strong { color: var(--text-1); }
  .del-input {
    width: 100%; padding: 9px 12px; border: 1px solid var(--border); border-radius: var(--r-sm);
    font-size: 13px; font-family: var(--font-mono); background: var(--bg-page); color: var(--text-1);
    outline: none; margin-bottom: 16px;
  }
  .del-input:focus { border-color: var(--err, #d44); box-shadow: 0 0 0 2px rgba(212,68,68,0.1); }
  .del-actions { display: flex; gap: 8px; justify-content: flex-end; }
  .btn.danger {
    background: var(--err, #d44); color: white; border: none; padding: 8px 16px;
    border-radius: var(--r-sm); font-size: 13px; font-weight: 500; cursor: pointer;
  }
  .btn.danger:hover { opacity: 0.9; }
  .btn.danger:disabled { opacity: 0.4; cursor: default; }
  .del-users { margin: 0 0 12px; padding-left: 18px; font-size: 13px; color: var(--text-2); max-height: 160px; overflow: auto; }
</style>
