<script lang="ts">
  import NewButton from '$lib/components/shared/NewButton.svelte';
  import * as YAML from 'yaml';
  import { flows as flowsMcp } from '$lib/services/api';
  import FlowEditorPane from '$lib/components/editor/FlowEditorPane.svelte';
  import { onMount } from 'svelte';
  import { getActivePanel } from '$lib/stores/panels.svelte.ts';

  // The flow browser lived here as a slide-out panel; the shell's context pane
  // lists every flow now, so this page takes its selection from the URL
  // (/flow-editor/<namespace>~<name>) instead of owning a second list.
  let availableFlows: { name: string; namespace: string; version: string; step_count: number }[] = $state([]);
  /// The context pane addresses a flow as "<namespace>~<name>": a slash would be
  /// read as another path segment.
  function keyFromSelection(sel: string | undefined): string {
    return sel && sel.includes('~') ? sel.replace('~', '/') : '';
  }
  let selectedFlowKey = $state(keyFromSelection(getActivePanel()?.props?.selectedId));
  let loading = $state(false);
  let dirty = $state(false);

  // The source of truth — YAML string
  let yamlSource = $state(`namespace: a3t
name: new-flow
version: "1.0.0"
description: ""
enabled: true
steps:
  - name: step-1
    type: set
    config:
      hello: world
`);

  // Parse for graph + form
  let parsed = $derived.by(() => {
    try { return YAML.parse(yamlSource) ?? {}; } catch { return null; }
  });


  // ── Load flows list ──
  onMount(async () => {
    // Load the flow the context pane asked for before listing, so the editor is
    // populated as soon as possible.
    if (selectedFlowKey) loadFlow(selectedFlowKey);
    for (const ns of ['a3t', 'default']) {
      const res = await flowsMcp.list(ns);
      if (res?.data?.workflows) {
        for (const f of res.data.workflows) {
          availableFlows.push({
            name: f.name,
            namespace: f.namespace ?? ns,
            version: f.version ?? '1.0.0',
            step_count: f.step_count ?? 0,
          });
        }
      }
    }
    availableFlows = [...availableFlows];
  });

  // ── Load selected flow ──
  async function loadFlow(key: string) {
    if (!key) return;
    const [ns, name] = key.split('/');
    loading = true;
    try {
      const res = await flowsMcp.get(name, ns);
      if (res?.data) {
        const text = typeof res.data === 'string' ? res.data : res.data.text ?? '';
        const match = text.match(/```yaml\n([\s\S]*?)```/);
        if (match) {
          yamlSource = match[1].trim();
        } else {
          yamlSource = text;
        }
        dirty = false;
      }
    } catch (e) {
      console.error('Failed to load flow:', e);
    } finally {
      loading = false;
    }
  }

  function newFlow() {
    selectedFlowKey = '';
    yamlSource = `namespace: a3t\nname: new-flow\nversion: "1.0.0"\ndescription: ""\nenabled: true\nsteps:\n  - name: step-1\n    type: set\n    config:\n      hello: world\n`;
    dirty = false;
  }

  function onYamlChange(val: string) {
    yamlSource = val;
    dirty = true;
  }

  // ── Deploy flow ──
  async function deployFlow() {
    if (!parsed) return;
    loading = true;
    try {
      if (selectedFlowKey) {
        const [ns, name] = selectedFlowKey.split('/');
        await flowsMcp.get(name, ns);
      }

      const res = await import('$lib/services/api').then(m => m.mcpCall('create_flow', { yaml: yamlSource }));
      if (res.ok) {
        dirty = false;
        const name = parsed.name;
        const ns = parsed.namespace ?? 'default';
        selectedFlowKey = `${ns}/${name}`;
        if (!availableFlows.find(f => f.namespace === ns && f.name === name)) {
          availableFlows = [...availableFlows, { name, namespace: ns, version: parsed.version ?? '1.0.0', step_count: parsed.steps?.length ?? 0 }];
        }
      }
    } catch (e) {
      console.error('Deploy failed:', e);
    } finally {
      loading = false;
    }
  }
</script>

<div class="flow-editor">
  <!-- Toolbar -->
  <div class="editor-toolbar">
    <!-- Which flow is open. Choosing one is the context pane's job now, so this
         is a label rather than a menu button. -->
    <span class="file-menu-btn as-label">
      <svg width="16" height="16" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.3"><path d="M3 2h6l4 4v8a1 1 0 01-1 1H3a1 1 0 01-1-1V3a1 1 0 011-1z"/><path d="M9 2v4h4"/></svg>
      <span class="file-menu-label">{selectedFlowKey || 'Untitled flow'}</span>
    </span>
    <NewButton label="New flow" accent="spec" size="sm" onclick={newFlow} title="Start a new flow" />

    <span style="flex: 1;"></span>

    {#if dirty}<span class="dirty-badge">unsaved</span>{/if}

    <button class="btn ghost sm" disabled={loading} onclick={() => { yamlSource = yamlSource; dirty = false; }}>Reset</button>
    <button class="btn spec sm" disabled={loading || !dirty} onclick={deployFlow}>
      {loading ? 'Deploying...' : 'Deploy'}
    </button>
  </div>

  <div class="editor-content">
    <!-- Editor body -->
    <div class="editor-body">
      <FlowEditorPane aiKind="flow" aiName={selectedFlowKey} initialYaml={yamlSource} onchange={onYamlChange} />
    </div>
  </div>
</div>

<style>
  .flow-editor { display: flex; flex-direction: column; height: 100%; overflow: hidden; }

  /* Toolbar */
  .editor-toolbar {
    display: flex; align-items: center; gap: 8px;
    padding: 8px 16px; border-bottom: 1px solid var(--border);
    background: var(--bg-surface); flex-shrink: 0;
  }

  .file-menu-btn.as-label { cursor: default; background: transparent; }
  .file-menu-btn {
    display: flex; align-items: center; gap: 6px;
    padding: 5px 10px; border: 1px solid var(--border); border-radius: var(--r-sm);
    background: var(--bg-surface); color: var(--text-2); font-size: 12px;
    font-family: var(--font-mono); cursor: pointer; transition: all 0.12s;
  }
  .file-menu-btn:hover { background: var(--bg-subtle); color: var(--text-1); border-color: var(--border-strong); }
  .file-menu-label { max-width: 200px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }

  .dirty-badge { font-family: var(--font-mono); font-size: 10px; color: var(--warn); background: var(--warn-bg); padding: 2px 6px; border-radius: 3px; }

  /* Editor content — hosts panel + editor side by side */
  .editor-content { flex: 1; overflow: hidden; position: relative; display: flex; }

  /* Overlay */

  /* Flow browser panel */
  @keyframes panelSlideIn { from { transform: translateX(-100%); opacity: 0; } to { transform: translateX(0); opacity: 1; } }









  /* Editor body */
  .editor-body { flex: 1; overflow: hidden; position: relative; }
</style>
