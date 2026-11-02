<script lang="ts">
  /// A canvas: the flow it deploys (pick one, or drop a flow from the Explorer),
  /// where that flow would be placed, its live deployment, and Deploy / Pause /
  /// Resume / Restart / Undeploy. Placement, lifecycle and health are in the Form
  /// panel. Deploy uses the canvas as pushed.
  import EditButton from '../parts/EditButton.svelte';
  import SaveBar from '../parts/SaveBar.svelte';
  import KeyValues from '../parts/KeyValues.svelte';
  import { flows, fromText, ITEM_MIME, ops, setAt, toText, type DeploymentAction, type DeploymentRow, type Doc, type Workspace } from '$lib/studio-core';

  let { doc, workspace }: { doc: Doc; workspace: Workspace } = $props();

  const kind = $derived(workspace.kindOf(doc));
  const parsed = $derived(fromText(doc.text));
  const spec = $derived((parsed.value ?? {}) as any);
  const flowRef = $derived(String(spec.flowRef ?? ''));
  const [flowNs, flowName] = $derived(flowRef.includes('/') ? [flowRef.split('/')[0], flowRef.split('/').slice(1).join('/')] : ['', '']);
  const dirty = $derived(workspace.isUnpushed(doc));
  const nameProblem = $derived(doc.isNew ? kind.checkName?.(doc.ref.name) ?? '' : '');

  let projectFlows = $state<string[]>([]);
  let live = $state<DeploymentRow | null>(null);
  let placement = $state<any>(null);
  let busy = $state('');
  let message = $state<{ ok: boolean; text: string } | null>(null);
  let dropping = $state(false);

  $effect(() => {
    if (!doc.loaded && !doc.isNew) void workspace.load(doc);
  });
  $effect(() => {
    const p = doc.ref.project;
    void flows.list(p).then((r) => { projectFlows = r.ok ? r.data.map((f: any) => `${f.namespace ?? p}/${f.name}`).sort() : []; });
  });
  // Live state and placement follow the saved flow.
  $effect(() => {
    if (!flowNs || dirty) { placement = null; return; }
    void refresh();
  });

  async function refresh() {
    const o = await ops.overview();
    const mine = `${doc.ref.project}/${doc.ref.name}`;
    live = o.ok ? ops.rowsOf(o.data).find((r) => r.deployment.flowRef === flowRef || r.deployment.canvas === mine) ?? null : null;
    if (flowNs) {
      const p = await ops.placement(flowNs, flowName);
      placement = p.ok ? p.data : { error: p.error };
    }
  }

  function attach(ref: string) {
    if (!parsed.value) return;
    doc.text = toText(setAt(spec, ['flowRef'], ref));
  }
  function ondragover(e: DragEvent) {
    if (e.dataTransfer?.types.includes(ITEM_MIME)) { e.preventDefault(); dropping = true; }
  }
  function ondrop(e: DragEvent) {
    dropping = false;
    const raw = e.dataTransfer?.getData(ITEM_MIME);
    if (!raw) return;
    e.preventDefault();
    const it = JSON.parse(raw);
    if (it.kind === 'flow') attach(`${it.project}/${it.name}`);
    else message = { ok: false, text: `A canvas deploys a flow; ${it.name} is not one.` };
  }

  async function deploy() {
    busy = 'deploy'; message = null;
    const r = await ops.deploy(flowNs, flowName);
    busy = '';
    message = r.ok
      ? { ok: true, text: r.data?.deployed ? `Deployed on ${r.data.placedRuntime ?? 'its runtime'}${r.data.edition === 'ee' ? ' (sandbox created)' : ''}.` : JSON.stringify(r.data) }
      : { ok: false, text: r.error };
    await refresh();
  }
  async function act(action: DeploymentAction) {
    if (!live) return;
    if (action === 'undeploy' && !(await workspace.host.confirm(`Undeploy ${live.deployment.target} from ${live.deployment.runtime}?`))) return;
    busy = action; message = null;
    const r = await ops.deployment(live.deployment.id, action);
    busy = '';
    message = r.ok ? { ok: true, text: `${action[0].toUpperCase()}${action.slice(1)}: done.` } : { ok: false, text: r.error };
    await refresh();
  }
</script>

<div class="cv">
  <div class="head">
    <span class="kind">{kind.label}</span>
    {#if doc.isNew}
      <input class="name" bind:value={doc.ref.name} spellcheck="false" title="The canvas's name" />
      {#if nameProblem}<span class="bad">{nameProblem}</span>{/if}
    {:else}
      <span class="mono">{doc.ref.project}/{doc.ref.name}</span>
    {/if}
    {#if live}<span class="pill {live.serving}">{live.serving.replace('-', ' ')}</span>{/if}
    <span class="grow"></span>
    <EditButton {doc} {workspace} />
    {#if live}
      {#if live.deployment.state === 'paused'}
        <button class="btn ghost sm" disabled={!!busy} onclick={() => act('resume')}>Resume</button>
      {:else}
        <button class="btn ghost sm" disabled={!!busy} onclick={() => act('pause')}>Pause</button>
      {/if}
      <button class="btn ghost sm" disabled={!!busy} onclick={() => act('restart')}>Restart</button>
      <button class="btn ghost sm" disabled={!!busy} onclick={() => act('undeploy')}>Undeploy</button>
    {/if}
    <button class="btn spec sm" disabled={!!busy || !flowNs || dirty || !!doc.isNew}
      title={doc.isNew || dirty ? 'Push the canvas first — Deploy uses the registry’s copy' : !flowNs ? 'Attach a flow first' : 'Deploy the flow with this canvas'}
      onclick={deploy}>{busy === 'deploy' ? 'Deploying…' : live ? 'Redeploy' : 'Deploy'}</button>
    <SaveBar {doc} {workspace} valid={!parsed.error && !nameProblem} />
  </div>

  {#if !doc.loaded && !doc.isNew}
    <div class="note">Loading {doc.ref.name}…</div>
  {:else if doc.error}
    <div class="err">Could not load {doc.ref.name}: {doc.error}</div>
  {:else if parsed.error}
    <div class="err">The canvas spec is not valid JSON: {parsed.error}</div>
  {:else}
    <div class="body">
      {#if message}<div class="msg" class:bad={!message.ok}>{message.text}</div>{/if}

      <section class="card flow" class:dropping aria-label="Flow" {ondragover} ondragleave={() => (dropping = false)} {ondrop}>
        <h3>Flow</h3>
        {#if flowRef}
          <div class="row">
            <button class="chip" onclick={() => workspace.open({ project: flowNs, kind: 'flow', name: flowName })} title="Open the flow">ƒ {flowRef}</button>
            <button class="lnk" onclick={() => attach('')}>Detach</button>
          </div>
        {:else}
          <p class="dim">No flow yet. Drop a flow here from the Explorer, or pick one.</p>
        {/if}
        <select value={flowRef} onchange={(e) => attach(e.currentTarget.value)}>
          <option value="">{flowRef ? 'Change flow…' : 'Pick a flow…'}</option>
          {#each projectFlows as f (f)}<option value={f}>{f}</option>{/each}
        </select>
      </section>

      <section class="card">
        <h3>Placement</h3>
        <KeyValues value={{ mode: spec.placement?.mode ?? 'shared', runtimeSelector: spec.placement?.runtimeSelector ?? {}, namespace: spec.placement?.namespace }} />
        {#if placement}
          <h4>Resolves to</h4>
          {#if placement.error}<div class="bad">{placement.error}</div>{:else}<KeyValues value={placement} />{/if}
        {:else if flowNs && dirty}
          <p class="dim">Save to see where it would be placed.</p>
        {/if}
        <p class="dim">Edit placement, lifecycle and health in the Form panel.</p>
      </section>

      <section class="card">
        <h3>Deployment</h3>
        {#if live}
          <KeyValues value={{ runtime: live.deployment.runtime, state: live.deployment.state, serving: live.serving, mode: live.deployment.mode,
            deployedBy: live.deployment.deployedBy, deployedAt: live.deployment.deployedAt, pauseReason: live.deployment.pauseReason }} />
          {#if live.stats}<h4>Runs</h4><KeyValues value={live.stats} />{/if}
          <button class="lnk" onclick={() => workspace.open({ project: live!.deployment.namespace, kind: 'resource', name: live!.deployment.runtime, type: 'runtime' })}>Open runtime {live.deployment.runtime} ↗</button>
        {:else}
          <p class="dim">{flowRef ? 'Not deployed.' : 'Attach a flow and save to deploy it.'}</p>
        {/if}
      </section>
    </div>
  {/if}
</div>

<style>
  .cv { display: flex; flex-direction: column; height: 100%; min-height: 0; }
  .head { display: flex; align-items: center; gap: 8px; padding: 6px 12px; border-bottom: 1px solid var(--border); background: var(--bg-surface); flex-wrap: wrap; }
  .kind { font-size: 11px; text-transform: uppercase; letter-spacing: 0.04em; color: var(--text-3); }
  .mono { font-family: var(--font-mono); font-size: 12.5px; }
  .name { font-family: var(--font-mono); font-size: 12.5px; padding: 4px 8px; width: 200px; flex: 0 1 200px; min-width: 120px; }
  .grow { flex: 1; }
  .pill { font-size: 10.5px; padding: 1px 7px; border-radius: 10px; background: var(--bg-subtle); color: var(--text-2); }
  .pill.serving { background: var(--runtime-bg, #e3f4ef); color: var(--runtime, #1a7158); }
  .pill.paused, .pill.runtime-paused, .pill.hibernated { background: var(--warn-bg); color: var(--warn); }
  .pill.unplaced { background: var(--err-bg, #fde8e8); color: var(--err, #b42318); }
  .body { flex: 1; min-height: 0; overflow-y: auto; padding: 14px 18px 30px; display: grid; grid-template-columns: repeat(auto-fill, minmax(320px, 1fr)); gap: 12px; align-content: start; }
  .msg { grid-column: 1 / -1; font-size: 12.5px; color: var(--ok, #15803d); }
  .msg.bad, .bad { color: var(--err, #b42318); font-size: 12px; }
  .card { border: 1px solid var(--border); border-radius: var(--r-sm); background: var(--bg-surface); padding: 12px 14px; display: flex; flex-direction: column; gap: 8px; }
  .card.flow { border-style: dashed; }
  .card.dropping { border-color: var(--spec); background: var(--spec-bg); }
  h3 { margin: 0; font-family: var(--font-serif); font-size: 15px; font-weight: 500; }
  h4 { margin: 6px 0 0; font-size: 11px; text-transform: uppercase; letter-spacing: 0.04em; color: var(--text-3); }
  .row { display: flex; align-items: center; gap: 8px; }
  .chip { font-family: var(--font-mono); font-size: 12.5px; padding: 3px 10px; border-radius: 4px; border: 0; background: var(--spec-bg); color: var(--spec-ink, var(--spec)); cursor: pointer; }
  select { padding: 5px 8px; font-size: 12.5px; font-family: var(--font-mono); }
  .lnk { border: 0; background: transparent; color: var(--spec); cursor: pointer; font-size: 12px; padding: 0; align-self: flex-start; }
  .dim { margin: 0; color: var(--text-4); font-size: 12.5px; }
  .note { padding: 14px; color: var(--text-4); font-size: 12.5px; }
  .err { margin: 10px 14px; padding: 8px 10px; border-radius: var(--r-sm); background: var(--err-bg, #fde8e8); color: var(--err, #b42318); font-size: 12px; }
</style>
