<script lang="ts">
  // Canvas = per-flow deployment config (Option 1). Agents-style list of
  // canvases (My / Shared / Community); New/Edit opens the drag-drop designer.
  // Runtime pools (placement targets) are managed via the Runtimes popin.
  import { canvasApi } from '$lib/services/api';
  import DesignerPanel from '$lib/components/shared/DesignerPanel.svelte';
  import PageBanner from '$lib/components/shared/PageBanner.svelte';
  import AppHome from '$lib/shell/AppHome.svelte';
  import EntityCard from '$lib/components/shared/EntityCard.svelte';
  import CanvasDesigner from '$lib/components/editor/CanvasDesigner.svelte';
  import { getActivePanel } from '$lib/stores/panels.svelte.ts';
  import { onMount } from 'svelte';

  let loading = $state(true);
  let error = $state('');
  let canvases = $state<any[]>([]);
  let runtimes = $state<any[]>([]);
  let deployMsg = $state('');
  // Live k8s deployment status from the controller, keyed by flowRef.
  let live = $state<Record<string, any>>({});
  let controllerOn = $state(false);

  // Designer popin.
  let designerOpen = $state(false);
  let editing = $state<any>(null);

  // Runtimes popin (DesignerPanel).
  const RT_NODES = [
    { id: 'details', label: 'Details', icon: '<rect x="2" y="2" width="12" height="12" rx="2"/><path d="M5 6h6M5 9h4"/>' },
    { id: 'labels', label: 'Labels & Placement', icon: '<path d="M8 1v4M8 11v4M1 8h4M11 8h4"/><circle cx="8" cy="8" r="3"/>' },
    { id: 'review', label: 'Review & Create', icon: '<path d="M4 8l3 3 5-6"/><circle cx="8" cy="8" r="7"/>' },
  ];
  let showRuntimes = $state(false);
  let rtTab = $state('details');
  let rtForm = $state({ name: '', namespace: 'default', labelsText: '', url: '', default: false });
  let rtBusy = $state('');

  async function load() {
    loading = true; error = '';
    const [cv, rt, dep] = await Promise.all([canvasApi.list(), canvasApi.runtimes(), canvasApi.deployments()]);
    if (rt.ok) runtimes = rt.data?.runtimes ?? [];
    live = {}; controllerOn = false;
    if (dep.ok && dep.data?.controller) {
      controllerOn = true;
      for (const c of (dep.data?.canvases ?? [])) {
        const fr = c.spec?.flowRef ?? `${c.namespace}/${c.name}`;
        live[fr] = { phase: c.status?.phase ?? 'Pending', message: c.status?.message ?? '', runtime: c.status?.placedRuntime ?? '' };
      }
    }
    if (cv.ok) {
      canvases = (Array.isArray(cv.data) ? cv.data : (cv.data?.resources ?? [])).map((r: any) => {
        const cfg = r.config ?? {};
        const placement = cfg.placement ?? {};
        const placed = resolveRuntime(placement);
        const flowRef = cfg.flowRef ?? '—';
        return {
          name: r.name, namespace: r.namespace, config: cfg,
          flowRef,
          mode: placement.mode ?? 'shared',
          state: cfg.lifecycle?.state ?? 'running',
          session: cfg.lifecycle?.session ?? 'ephemeral',
          deps: (cfg.health?.dependsOn ?? []).length,
          placedRuntime: placed?.name ?? null,
          live: live[flowRef] ?? null,
        };
      });
    } else {
      error = cv.status === 403 ? 'Canvas management requires an Admin role.' : (cv.error ?? 'Failed to load canvases');
    }
    loading = false;
  }
  onMount(async () => {
    await load();
    // Deep-link from an agent's "Edit canvas" → open the designer on that canvas.
    const p = getActivePanel();
    if (p?.props?.editCanvas) {
      const c = canvases.find((x) => x.name === p.props.editCanvas);
      if (c) editCanvas(c);
    }
  });

  // Client mirror of flowr-types::canvas::select_runtime — shows which runtime pool
  // a canvas resolves to, live, without a round-trip.
  function resolveRuntime(placement: any): any | null {
    const sel: Record<string, string> = placement?.runtimeSelector ?? {};
    const ns = (placement?.namespace ?? '').trim();
    const pool = runtimes.filter((r) => !ns || r.namespace === ns);
    if (Object.keys(sel).length === 0) return pool.find((r) => r.default) ?? pool[0] ?? null;
    return pool.find((r) => Object.entries(sel).every(([k, v]) => (r.labels ?? {})[k] === v)) ?? null;
  }

  function newCanvas() { editing = null; designerOpen = true; }
  function editCanvas(c: any) { editing = c; designerOpen = true; }
  async function removeCanvas(c: any) {
    if (!confirm(`Delete canvas "${c.name}"?`)) return;
    const res = await canvasApi.remove(c.namespace, c.name);
    if (res.ok) await load();
  }
  async function deployCanvas(c: any) {
    const [ns, nm] = (c.flowRef || '').split('/');
    if (!ns || !nm) { deployMsg = `${c.name}: no flowRef to deploy`; return; }
    deployMsg = `Deploying ${c.flowRef}…`;
    const res = await canvasApi.deploy(ns, nm);
    if (res.ok && res.data?.deployed) {
      const ee = res.data?.edition === 'ee';
      deployMsg = ee
        ? `${c.name}: deployed → sandbox created on ${res.data.placedRuntime ?? c.mode}`
        : `${c.name}: placed on ${res.data.placedRuntime} (CE logical)`;
      // Refresh live status so the card badge updates.
      setTimeout(load, 1500);
    } else {
      deployMsg = `${c.name}: ${res.data?.error ?? res.error ?? 'deploy failed'}`;
    }
  }

  function openRuntimes() { rtForm = { name: '', namespace: 'default', labelsText: '', url: '', default: false }; rtBusy = ''; rtTab = 'details'; showRuntimes = true; }
  async function createRuntime() {
    if (!rtForm.name.trim()) { rtBusy = 'Name required'; return; }
    const labels: Record<string, string> = {};
    for (const p of rtForm.labelsText.split(',').map((s) => s.trim()).filter(Boolean)) {
      const [k, v] = p.split('=').map((s) => s.trim()); if (k && v) labels[k] = v;
    }
    rtBusy = 'saving';
    const res = await canvasApi.createRuntime(rtForm.namespace, rtForm.name.trim(), { labels, namespace: rtForm.namespace, url: rtForm.url || undefined, default: rtForm.default });
    if (res.ok) { showRuntimes = false; await load(); } else rtBusy = res.error || `Create failed (HTTP ${res.status})`;
  }
</script>

<div class="screen-header">
  <div class="breadcrumb">Canvas <span class="sep">/</span> All</div>
  <div class="screen-title-row">
    <div class="title-block"><h1 class="screen-title">Canvas</h1></div>
    <div class="screen-actions">
      <button class="btn" onclick={openRuntimes}>Runtimes <span class="rt-count">{runtimes.length}</span></button>
      <button class="btn runtime" onclick={newCanvas}>+ New canvas</button>
    </div>
  </div>
</div>

<div class="screen-body">
  <PageBanner
    headline="Deploy a flow as a Canvas."
    highlightWord="Canvas"
    subtitle="A canvas is one flow's deployment metadata: where it loads, how it scales, what it depends on."
    description="Pick the flow, set placement (runtime selector labels + isolation), lifecycle (scale/restart), and health (dependent resources). The canvas resolves to a runtime pool; Deploy performs the placement."
    accent="control"
    docsPath="/canvas"
  />
  <!-- Pinned and latest, under the banner: the section page leads with what
       it is for, then the items you actually return to. -->
  <AppHome appType="canvas" embedded />


  {#if deployMsg}<div class="deploy-banner">{deployMsg}</div>{/if}

  {#if loading}
    <div class="status-msg">Loading…</div>
  {:else if error}
    <div class="status-msg err">{error}</div>
  {:else}
    <div class="agt-group" class:agt-empty={canvases.length === 0}>
      <button class="agt-group-header">
        <svg class="agt-chevron" class:open={canvases.length > 0} width="12" height="12" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="2"><path d="M6 4l4 4-4 4"/></svg>
        <h3>My Canvases</h3>
        <span class="agt-group-count">{canvases.length}</span>
      </button>
      {#if canvases.length > 0}
        <div class="card-grid">
          {#each canvases as c}
            <EntityCard
              icon={(c.name ?? '?').slice(0, 2).toUpperCase()}
              color="#3a6ea5"
              name={c.name}
              description="flow {c.flowRef}"
              tags={c.live ? [c.mode, c.session, c.live.phase] : [c.mode, c.session, c.state]}
              meta={c.live ? (c.live.runtime || c.placedRuntime || c.mode) : (controllerOn ? 'not deployed' : (c.placedRuntime ? '→ ' + c.placedRuntime : 'no runtime'))}
              actionLabel="Deploy"
              onclick={() => editCanvas(c)}
              onaction={() => deployCanvas(c)}
              ondelete={() => removeCanvas(c)}
            />
          {/each}
        </div>
      {/if}
    </div>

    <div class="agt-group agt-empty">
      <button class="agt-group-header">
        <svg class="agt-chevron" width="12" height="12" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="2" style="opacity:0;"><path d="M6 4l4 4-4 4"/></svg>
        <h3>Shared Canvases</h3>
        <span class="agt-group-count">0</span>
        <span class="agt-group-hint">Shared across teams</span>
      </button>
    </div>
    <div class="agt-group agt-empty">
      <button class="agt-group-header">
        <svg class="agt-chevron" width="12" height="12" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="2" style="opacity:0;"><path d="M6 4l4 4-4 4"/></svg>
        <h3>Community Canvases</h3>
        <span class="agt-group-count">0</span>
        <span class="agt-group-hint">Open-source deployment specs</span>
      </button>
    </div>
  {/if}
</div>

<!-- Drag-drop designer popin -->
<CanvasDesigner open={designerOpen} canvas={editing} onclose={() => (designerOpen = false)} onsaved={load} />

<!-- Runtime pool popin (canonical designer form) -->
<DesignerPanel open={showRuntimes} onclose={() => (showRuntimes = false)} icon="R" name={rtForm.name || 'New Runtime'} meta="runtime pool" nodes={RT_NODES} activeNode={rtTab} onselect={(id) => (rtTab = id)}>
  {#if rtTab === 'details'}
    <div class="pf"><h3>Details</h3>
      <div class="cf-grid">
        <label class="cf-field"><span>Name</span><input bind:value={rtForm.name} placeholder="rt-gpu-prod" /></label>
        <label class="cf-field"><span>Namespace</span><input bind:value={rtForm.namespace} /></label>
      </div>
      <label class="cf-field"><span>URL (optional)</span><input bind:value={rtForm.url} placeholder="http://runtime-gpu.a3t.svc:3002" /></label>
      <label class="cf-field cf-check"><input type="checkbox" bind:checked={rtForm.default} /> <span>Default pool</span></label>
    </div>
  {:else if rtTab === 'labels'}
    <div class="pf"><h3>Labels &amp; placement</h3>
      <p class="pf-hint">A canvas's placement selector matches these labels.</p>
      <label class="cf-field"><span>Labels (key=value, comma-separated)</span><input bind:value={rtForm.labelsText} placeholder="class=gpu, tier=prod" /></label>
    </div>
  {:else}
    <div class="pf"><h3>Review &amp; create</h3>
      <div class="pf-row"><span class="pf-k">Name</span><span class="pf-v">{rtForm.name || '—'}</span></div>
      <div class="pf-row"><span class="pf-k">Namespace</span><span class="pf-v">{rtForm.namespace}</span></div>
      <div class="pf-row"><span class="pf-k">Labels</span><span class="pf-v">{rtForm.labelsText || '(none)'}</span></div>
      <div class="pf-row"><span class="pf-k">Default</span><span class="pf-v">{rtForm.default ? 'yes' : 'no'}</span></div>
      <div class="create-actions">
        <button class="btn" onclick={() => (showRuntimes = false)}>Cancel</button>
        <button class="btn spec" onclick={createRuntime} disabled={rtBusy === 'saving' || !rtForm.name.trim()}>{rtBusy === 'saving' ? 'Saving…' : 'Create Runtime'}</button>
        {#if rtBusy && rtBusy !== 'saving'}<span class="cf-msg err">{rtBusy}</span>{/if}
      </div>
    </div>
  {/if}
</DesignerPanel>

<style>
  .screen-header { padding: 18px 32px 12px; border-bottom: 1px solid var(--border); background: var(--bg-page); }
  .breadcrumb { font-size: 12px; color: var(--text-3); margin-bottom: 8px; font-family: var(--font-mono); }
  .breadcrumb .sep { color: var(--text-5); }
  .screen-title-row { display: flex; align-items: center; gap: 18px; }
  .title-block { flex: 1; display: flex; align-items: center; gap: 10px; }
  .screen-title { font-family: var(--font-serif); font-size: 24px; font-weight: 500; letter-spacing: -0.02em; }
  .screen-actions { display: flex; gap: 8px; }
  .btn { padding: 7px 14px; border: 1px solid var(--border); border-radius: 6px; background: var(--bg-surface); color: var(--text-2); font-size: 13px; cursor: pointer; }
  .btn:hover { background: var(--bg-subtle); }
  .btn.runtime { background: var(--runtime-bg, #eaf3ee); color: var(--runtime, #1a7158); border-color: transparent; }
  .rt-count { font-family: var(--font-mono); font-size: 11px; color: var(--text-4); }
  .screen-body { flex: 1; overflow-y: auto; padding: 20px 32px 40px; }
  .deploy-banner { margin-bottom: 14px; font-size: 13px; color: var(--text-2); background: var(--bg-subtle); border: 1px solid var(--border); border-radius: 6px; padding: 8px 12px; }
  .status-msg { padding: 20px; text-align: center; color: var(--text-4); font-size: 14px; }
  .status-msg.err { color: var(--err, #d44); font-family: var(--font-mono); font-size: 12px; }

  .card-grid { display: grid; grid-template-columns: repeat(auto-fill, minmax(300px, 1fr)); gap: 12px; }

  .agt-group { margin-bottom: 24px; }
  .agt-group-header {
    display: flex; align-items: center; gap: 10px; width: 100%;
    padding: 0 0 10px; border: none; background: none; cursor: pointer; text-align: left;
  }
  .agt-group-header h3 { font-family: var(--font-serif); font-size: 16px; font-weight: 500; color: var(--text-1); }
  .agt-group-count { font-family: var(--font-mono); font-size: 11px; color: var(--text-4); background: var(--bg-subtle); padding: 1px 6px; border-radius: 3px; }
  .agt-group-hint { font-size: 11px; color: var(--text-4); font-style: italic; margin-left: auto; }
  .agt-chevron { color: var(--text-4); transition: transform 0.2s; flex-shrink: 0; }
  .agt-chevron.open { transform: rotate(90deg); }
  .agt-empty { opacity: 0.5; }
  .agt-empty .agt-group-header { cursor: default; }
</style>
