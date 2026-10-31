<script lang="ts">
  import { openPublish } from '$lib/stores/publish.svelte.ts';
  import ExposedSections from '$lib/components/shared/ExposedSections.svelte';
  import { inContext, canvasKind, itemProject } from '$lib/stores/context.svelte.ts';
  import { contextRev_ } from '$lib/stores/apps.svelte.ts';
  import SubmenuToggle from '$lib/components/shared/SubmenuToggle.svelte';
  import NewButton from '$lib/components/shared/NewButton.svelte';
  // Canvas = per-flow deployment config (Option 1). Agents-style list of
  // canvases (My / Shared / Community); New/Edit opens the drag-drop designer.
  // Runtime pools (placement targets) are managed via the Runtimes popin.
  import { canvasApi, opsApi } from '$lib/services/api';
  import PageBanner from '$lib/components/shared/PageBanner.svelte';
  import AppHome from '$lib/shell/AppHome.svelte';
  import EntityCard from '$lib/components/shared/EntityCard.svelte';
  import FoldSection from '$lib/components/shared/FoldSection.svelte';
  import CanvasDesigner from '$lib/components/editor/CanvasDesigner.svelte';
  import { getActivePanel } from '$lib/stores/panels.svelte.ts';
  import { onMount } from 'svelte';

  let loading = $state(true);
  let error = $state('');
  let canvases = $state<any[]>([]);
  /// Presets (`preset-*`) are shared placement shapes; every other canvas is mine.
  const myCanvases = $derived(canvases.filter((c) => canvasKind(c) === 'owned'));
  const sharedCanvases = $derived(canvases.filter((c) => canvasKind(c) === 'shared'));
  let runtimes = $state<any[]>([]);
  let deployMsg = $state('');
  // Live k8s deployment status from the controller, keyed by flowRef.
  let live = $state<Record<string, any>>({});
  let controllerOn = $state(false);

  // Designer popin.
  let designerOpen = $state(false);
  let editing = $state<any>(null);


  async function load() {
    loading = true; error = '';
    // Live state comes from the deployment ledger — the record every deploy
    // path writes, CE and EE alike — not from the controller, which may not be
    // running at all.
    const [cv, rt, ops] = await Promise.all([canvasApi.list(), canvasApi.runtimes(), opsApi.overview()]);
    if (rt.ok) runtimes = rt.data?.runtimes ?? [];
    live = {}; controllerOn = false;
    if (ops.ok) {
      controllerOn = true;
      const rows = [...(ops.data.runtimes ?? []).flatMap((r: any) => r.deployments), ...(ops.data.unplaced ?? [])];
      const LABEL: Record<string, string> = { serving: 'serving', paused: 'paused', 'runtime-paused': 'runtime paused', hibernated: 'hibernated', unplaced: 'unplaced' };
      for (const row of rows) {
        live[row.deployment.flowRef] = { phase: LABEL[row.serving] ?? row.serving, message: row.deployment.pauseReason ?? '', runtime: row.deployment.runtime };
      }
    }
    if (cv.ok) {
      canvases = (Array.isArray(cv.data) ? cv.data : (cv.data?.resources ?? []))
        .map((r: any) => ({ ...r, project: itemProject('resource', r.namespace, { ...r, type: 'canvas' }) }))
        .filter((r: any) => inContext(r.project, canvasKind(r))).map((r: any) => {
        const cfg = r.config ?? {};
        const placement = cfg.placement ?? {};
        const placed = resolveRuntime(placement);
        const flowRef = cfg.flowRef ?? '—';
        return {
          name: r.name, namespace: r.namespace, project: r.project, config: cfg,
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


  // A different project (or "show platform") is a different list.
  let loadedRev = contextRev_();
  $effect(() => {
    const rev = contextRev_();
    if (rev === loadedRev) return;
    loadedRev = rev;
    void load();
  });
  onMount(async () => {
    await load();
    // Deep-link from an agent's "Edit canvas" → open the designer on that canvas.
    const p = getActivePanel();
    // Also from the submenu, which addresses a canvas by `selectedId`.
    const want = p?.props?.editCanvas ?? p?.props?.selectedId;
    if (want) {
      const c = canvases.find((x) => x.name === want);
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
    // Refused while deployed or bound to an agent — the reason names what to remove first.
    else deployMsg = `${c.name}: ${res.error ?? 'delete failed'}`;
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

</script>

<div class="screen-header">
  <div class="breadcrumb">Canvas <span class="sep">/</span> All</div>
  <div class="screen-title-row">
    <div class="title-block"><SubmenuToggle appType="canvas" /><h1 class="screen-title">Canvas</h1></div>
    <div class="screen-actions">
      <NewButton label="New canvas" accent="control" onclick={newCanvas} />
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
    {#snippet canvasCards(list: any[])}
      <div class="card-grid">
        {#each list as c (c.namespace + '/' + c.name)}
          <EntityCard
            icon={(c.name ?? '?').slice(0, 2).toUpperCase()}
            color="#3a6ea5"
            name={c.name}
            description={c.flowRef === '—' ? 'preset — copy it to deploy a flow' : `flow ${c.flowRef}`}
            tags={c.live ? [c.mode, c.session, c.live.phase] : [c.mode, c.session, c.state]}
            meta={c.live ? (c.live.runtime || c.placedRuntime || c.mode) : (controllerOn ? 'not deployed' : (c.placedRuntime ? '→ ' + c.placedRuntime : 'no runtime'))}
            project={c.project ?? c.namespace}
            actionLabel="Deploy"
            onclick={() => editCanvas(c)}
            onaction={() => deployCanvas(c)}
            ondelete={() => removeCanvas(c)}
            onpublish={canvasKind(c) === 'owned' ? () => openPublish({ kind: 'resource', from: c.namespace, ref: c.name, label: c.name }) : undefined}
          />
        {/each}
      </div>
    {/snippet}

    <FoldSection title="My Canvases" count={myCanvases.length} hint="Deploy a flow in this project">
      {@render canvasCards(myCanvases)}
    </FoldSection>
    <FoldSection title="Shared Canvases" count={sharedCanvases.length} hint="Presets — placement shapes every project can copy">
      {@render canvasCards(sharedCanvases)}
    </FoldSection>
    <ExposedSections kind="canvas" noun="Canvases" />
  {/if}
</div>

<!-- Drag-drop designer popin -->
<CanvasDesigner open={designerOpen} canvas={editing} onclose={() => (designerOpen = false)} onsaved={load} />


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
  .screen-body { flex: 1; overflow-y: auto; padding: 20px 32px 40px; }
  .deploy-banner { margin-bottom: 14px; font-size: 13px; color: var(--text-2); background: var(--bg-subtle); border: 1px solid var(--border); border-radius: 6px; padding: 8px 12px; }
  .status-msg { padding: 20px; text-align: center; color: var(--text-4); font-size: 14px; }
  .status-msg.err { color: var(--err, #d44); font-family: var(--font-mono); font-size: 12px; }

  .card-grid { display: grid; grid-template-columns: repeat(auto-fill, minmax(300px, 1fr)); gap: 12px; }

</style>
