<script lang="ts">
  /// Pick a runtime and deploy, from inside the canvas pane — or, for a shared
  /// preset, clone it first.
  ///
  /// Nothing here acts directly. Each button sends a request to the agent, so a
  /// deploy goes through `deploy_flow` with both of its gates (tool access and
  /// `flow:deploy` on the chosen pool), and anything that needs Admin raises the
  /// approval card in the conversation. A button is a shortcut for asking.
  import { mcpCall } from '$lib/services/api';

  let { namespace, name, text, onrequest }: {
    namespace: string;
    name: string;
    /// The pane's current JSON, read for the canvas's flowRef / preset state.
    text: string;
    onrequest?: (msg: string) => void;
  } = $props();

  let runtimes = $state<{ name: string; namespace: string; labels: Record<string, string> }[]>([]);
  let chosen = $state('');
  let showNew = $state(false);
  let newName = $state('');
  let newTier = $state('test');
  let newClass = $state('cpu');
  /// Plain `let`: read and written by the effect below, so reactive state here
  /// would self-retrigger.
  let loadedFor = '';

  const parsed = $derived.by(() => {
    try { return JSON.parse(text); } catch { return null; }
  });
  const spec = $derived(parsed?.spec ?? parsed ?? {});
  const flowRef = $derived<string>(spec?.flowRef ?? '');
  const isPreset = $derived(!flowRef);
  const pinned = $derived<string>(spec?.placement?.runtime ?? '');

  $effect(() => {
    const key = `${namespace}/${name}`;
    if (loadedFor === key) return;
    loadedFor = key;
    mcpCall('list_runtimes', {}).then((r) => {
      const list = Array.isArray(r.data?.runtimes) ? r.data.runtimes : [];
      runtimes = list;
      if (!chosen) chosen = pinned || list[0]?.name || '';
    });
  });

  function labelText(l: Record<string, string>): string {
    const e = Object.entries(l ?? {});
    return e.length ? e.map(([k, v]) => `${k}=${v}`).join(' ') : 'no labels';
  }

  function deploy() {
    if (!flowRef || !chosen) return;
    const [fns, fname] = flowRef.split('/');
    onrequest?.(
      `Deploy flow ${fns}/${fname} onto runtime ${chosen} using canvas ${namespace}/${name}. ` +
      `Preview the placement first.`
    );
  }

  function createRuntime() {
    const n = newName.trim();
    if (!n) return;
    onrequest?.(
      `Create a new runtime named ${n} with labels tier=${newTier} and class=${newClass}.`
    );
    showNew = false;
    newName = '';
  }
</script>

<div class="cdb">
  {#if isPreset}
    <div class="cdb-row">
      <span class="cdb-note">This is a shared preset — clone it to use it for a flow.</span>
      <button class="cdb-btn" onclick={() => onrequest?.(`Clone the ${namespace}/${name} preset so I can use it for a flow.`)}>
        Clone this preset
      </button>
    </div>
  {:else}
    <div class="cdb-row">
      <span class="cdb-label">Deploy <code>{flowRef}</code> onto</span>
      <select bind:value={chosen} class="cdb-select" disabled={runtimes.length === 0}>
        {#each runtimes as r}
          <option value={r.name}>{r.name} — {labelText(r.labels)}</option>
        {/each}
      </select>
      <button class="cdb-btn primary" onclick={deploy} disabled={!chosen}>Deploy</button>
      <button class="cdb-link" onclick={() => (showNew = !showNew)}>{showNew ? 'Cancel' : '+ New runtime'}</button>
    </div>
  {/if}

  {#if showNew}
    <div class="cdb-row cdb-new">
      <input class="cdb-input" placeholder="runtime name" bind:value={newName} />
      <label>tier
        <select bind:value={newTier} class="cdb-select sm"><option>test</option><option>prod</option></select>
      </label>
      <label>class
        <select bind:value={newClass} class="cdb-select sm"><option>cpu</option><option>gpu</option></select>
      </label>
      <button class="cdb-btn" onclick={createRuntime} disabled={!newName.trim()}>Create</button>
    </div>
    <p class="cdb-hint">Registers a placement target (needs Admin — you'll be asked). It does not start a process or sandbox.</p>
  {/if}
</div>

<style>
  .cdb { border-bottom: 1px solid var(--border); padding: 8px 12px; background: var(--bg-subtle); }
  .cdb-row { display: flex; align-items: center; gap: 8px; flex-wrap: wrap; }
  .cdb-new { margin-top: 8px; }
  .cdb-label, .cdb-note { font-size: 12px; color: var(--text-2); }
  .cdb-label code { font-family: var(--font-mono, ui-monospace, monospace); font-size: 11.5px; }
  .cdb-select {
    font-size: 12px; padding: 4px 6px; border: 1px solid var(--border);
    border-radius: var(--r-sm, 6px); background: var(--bg-surface); color: var(--text-1);
    max-width: 280px;
  }
  .cdb-select.sm { max-width: 90px; }
  .cdb-input {
    font-size: 12px; padding: 4px 8px; border: 1px solid var(--border);
    border-radius: var(--r-sm, 6px); background: var(--bg-surface); color: var(--text-1); width: 150px;
  }
  .cdb-new label { font-size: 11.5px; color: var(--text-3); display: inline-flex; align-items: center; gap: 4px; }
  .cdb-btn {
    padding: 5px 11px; font-size: 12px; border-radius: var(--r-sm, 6px);
    border: 1px solid var(--border-strong); background: var(--bg-surface); color: var(--text-1); cursor: pointer;
  }
  .cdb-btn.primary { background: var(--text-1); color: var(--bg-page); border-color: var(--text-1); }
  .cdb-btn:disabled { opacity: 0.5; cursor: default; }
  .cdb-link { border: none; background: transparent; color: var(--text-3); font-size: 12px; cursor: pointer; text-decoration: underline; }
  .cdb-hint { margin: 6px 0 0; font-size: 11px; color: var(--text-4); }
</style>
