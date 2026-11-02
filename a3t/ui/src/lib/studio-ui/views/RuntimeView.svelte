<script lang="ts">
  /// A runtime: its state and labels, the host it runs on, and what is deployed
  /// on it — each deployment's serving state and runs, with pause / resume /
  /// restart / undeploy. Runtime-wide pause / resume / hibernate for admins. Its
  /// pool config (labels, default, url) is in the Form panel.
  import EditButton from '../parts/EditButton.svelte';
  import KeyValues from '../parts/KeyValues.svelte';
  import { ops, type DeploymentAction, type DeploymentRow, type Doc, type RuntimeAction, type RuntimeEntry, type Workspace } from '$lib/studio-core';

  let { doc, workspace }: { doc: Doc; workspace: Workspace } = $props();

  const kind = $derived(workspace.kindOf(doc));
  let entry = $state<RuntimeEntry | null>(null);
  let error = $state('');
  let busy = $state('');
  let message = $state<{ ok: boolean; text: string } | null>(null);

  $effect(() => {
    if (!doc.loaded) void workspace.load(doc);
  });
  $effect(() => {
    doc.ref.name;
    void refresh();
  });

  async function refresh() {
    const o = await ops.overview();
    if (!o.ok) { error = o.error; return; }
    error = '';
    entry = o.data.runtimes.find((r) => r.runtime.name === doc.ref.name) ?? null;
  }

  async function runtimeAct(action: RuntimeAction) {
    if (!(await workspace.host.confirm(`${action[0].toUpperCase()}${action.slice(1)} runtime ${doc.ref.name}? Everything deployed on it is affected.`))) return;
    busy = action; message = null;
    const r = await ops.runtime(doc.ref.name, action);
    busy = '';
    message = r.ok ? { ok: true, text: `Runtime: ${action} done.` } : { ok: false, text: r.error };
    await refresh();
  }
  async function deploymentAct(row: DeploymentRow, action: DeploymentAction) {
    if (action === 'undeploy' && !(await workspace.host.confirm(`Undeploy ${row.deployment.target}?`))) return;
    busy = `${row.deployment.id}:${action}`; message = null;
    const r = await ops.deployment(row.deployment.id, action);
    busy = '';
    message = r.ok ? { ok: true, text: `${row.deployment.target}: ${action} done.` } : { ok: false, text: r.error };
    await refresh();
  }
  /// What a deployment runs: a flow (open it), or an agent.
  function openTarget(row: DeploymentRow) {
    const ref = row.deployment.flowRef || (row.deployment.kind === 'flow' ? row.deployment.target : '');
    if (!ref.includes('/')) return;
    const [project, ...name] = ref.split('/');
    workspace.open({ project, kind: 'flow', name: name.join('/') });
  }
  const when = (s?: string) => (s ? new Date(s).toLocaleString([], { month: 'short', day: 'numeric', hour: '2-digit', minute: '2-digit' }) : '');
</script>

<div class="rv">
  <div class="head">
    <span class="kind">{kind.label}</span>
    <span class="mono">{doc.ref.project}/{doc.ref.name}</span>
    {#if entry}<span class="pill {entry.state}">{entry.state}</span>{/if}
    {#if entry?.runtime.default}<span class="pill">default</span>{/if}
    <span class="grow"></span>
    <EditButton {doc} {workspace} />
    {#if entry?.canControl}
      {#if entry.state === 'active'}
        <button class="btn ghost sm" disabled={!!busy} onclick={() => runtimeAct('pause')}>Pause runtime</button>
        <button class="btn ghost sm" disabled={!!busy} onclick={() => runtimeAct('hibernate')}>Hibernate</button>
      {:else}
        <button class="btn ghost sm" disabled={!!busy} onclick={() => runtimeAct('resume')}>Resume runtime</button>
      {/if}
    {/if}
    <button class="btn ghost sm" onclick={refresh}>↻</button>
  </div>

  <div class="body">
    {#if error}<div class="err">{error}</div>{/if}
    {#if message}<div class="msg" class:bad={!message.ok}>{message.text}</div>{/if}
    {#if !entry && !error}
      <p class="dim">This runtime is not in the live ledger (not registered, or nothing has reported it yet).</p>
    {:else if entry}
      <div class="cards">
        <section class="card">
          <h3>Runtime</h3>
          <KeyValues value={{ labels: entry.runtime.labels, namespace: entry.runtime.namespace, url: entry.runtime.url, callersGetWhenPaused: entry.pausedResponse }} />
        </section>
        <section class="card">
          <h3>Host</h3>
          <KeyValues value={entry.host} />
        </section>
      </div>

      <h3 class="sec">Deployed here <span class="n">{entry.deployments.length}</span></h3>
      {#if !entry.deployments.length}
        <p class="dim">Nothing is deployed on this runtime. Deploy a flow from its canvas.</p>
      {:else}
        <table>
          <thead><tr><th>What</th><th>Kind</th><th>Serving</th><th>Mode</th><th>Runs</th><th>Deployed</th><th></th></tr></thead>
          <tbody>
            {#each entry.deployments as row (row.deployment.id)}
              <tr>
                <td><button class="lnk mono" onclick={() => openTarget(row)} title="Open">{row.deployment.target}</button></td>
                <td>{row.deployment.kind}</td>
                <td><span class="pill {row.serving}">{row.serving.replace('-', ' ')}</span>{#if row.deployment.pauseReason}<span class="dim small" title={row.deployment.pauseReason}> — {row.deployment.pauseReason}</span>{/if}</td>
                <td>{row.deployment.mode}</td>
                <td class="stats"><KeyValues value={row.stats} /></td>
                <td class="dim small">{row.deployment.deployedBy} · {when(row.deployment.deployedAt)}</td>
                <td class="acts">
                  {#if row.deployment.state === 'paused'}
                    <button class="lnk" disabled={!!busy} onclick={() => deploymentAct(row, 'resume')}>Resume</button>
                  {:else}
                    <button class="lnk" disabled={!!busy} onclick={() => deploymentAct(row, 'pause')}>Pause</button>
                  {/if}
                  <button class="lnk" disabled={!!busy} onclick={() => deploymentAct(row, 'restart')}>Restart</button>
                  <button class="lnk danger" disabled={!!busy} onclick={() => deploymentAct(row, 'undeploy')}>Undeploy</button>
                </td>
              </tr>
            {/each}
          </tbody>
        </table>
      {/if}
      <p class="dim">Logs need a log stream from the platform (not there yet).</p>
    {/if}
  </div>
</div>

<style>
  .rv { display: flex; flex-direction: column; height: 100%; min-height: 0; }
  .head { display: flex; align-items: center; gap: 8px; padding: 6px 12px; border-bottom: 1px solid var(--border); background: var(--bg-surface); }
  .kind { font-size: 11px; text-transform: uppercase; letter-spacing: 0.04em; color: var(--text-3); }
  .mono { font-family: var(--font-mono); font-size: 12.5px; }
  .grow { flex: 1; }
  .pill { font-size: 10.5px; padding: 1px 7px; border-radius: 10px; background: var(--bg-subtle); color: var(--text-2); white-space: nowrap; }
  .pill.active, .pill.serving { background: var(--runtime-bg, #e3f4ef); color: var(--runtime, #1a7158); }
  .pill.paused, .pill.runtime-paused, .pill.hibernated { background: var(--warn-bg); color: var(--warn); }
  .pill.unplaced { background: var(--err-bg, #fde8e8); color: var(--err, #b42318); }
  .body { flex: 1; min-height: 0; overflow-y: auto; padding: 14px 18px 30px; display: flex; flex-direction: column; gap: 12px; }
  .cards { display: grid; grid-template-columns: repeat(auto-fill, minmax(320px, 1fr)); gap: 12px; }
  .card { border: 1px solid var(--border); border-radius: var(--r-sm); background: var(--bg-surface); padding: 12px 14px; display: flex; flex-direction: column; gap: 8px; }
  h3 { margin: 0; font-family: var(--font-serif); font-size: 15px; font-weight: 500; }
  .sec { margin-top: 6px; }
  .n { font-family: var(--font-mono); font-size: 11px; color: var(--text-4); }
  table { border-collapse: collapse; font-size: 12.5px; width: 100%; }
  th, td { text-align: left; padding: 6px 8px; border-bottom: 1px solid var(--border); vertical-align: top; }
  th { font-size: 11px; text-transform: uppercase; letter-spacing: 0.04em; color: var(--text-3); font-weight: 500; }
  .acts { white-space: nowrap; }
  .lnk { border: 0; background: transparent; color: var(--spec); cursor: pointer; font-size: 12px; padding: 0 6px 0 0; }
  .lnk.danger { color: var(--err, #b42318); }
  .lnk:disabled { opacity: 0.5; cursor: default; }
  .stats :global(dl) { font-size: 11px; }
  .dim { margin: 0; color: var(--text-4); font-size: 12.5px; }
  .small { font-size: 11.5px; }
  .msg { font-size: 12.5px; color: var(--ok, #15803d); }
  .msg.bad { color: var(--err, #b42318); }
  .err { padding: 8px 10px; border-radius: var(--r-sm); background: var(--err-bg, #fde8e8); color: var(--err, #b42318); font-size: 12px; }
</style>
