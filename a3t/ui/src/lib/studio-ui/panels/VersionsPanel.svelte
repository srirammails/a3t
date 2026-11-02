<script lang="ts">
  /// The versions the registry keeps of the open item (bottom region, beside
  /// Runs): which is active — what runs without a pin — which one the editor
  /// holds, and for each: load it into the editor, or make it the active one.
  /// A new version comes from Sync → Push as a new version.
  import { sortVersions, type Doc, type Workspace } from '$lib/studio-core';

  let { doc, workspace }: { doc: Doc | null; workspace: Workspace } = $props();

  const kind = $derived(doc ? workspace.kindOf(doc) : null);
  const can = $derived(!!doc && !!kind?.versions && !doc.isNew);
  let list = $state<string[]>([]);
  let active = $state('');
  let loading = $state(false);
  let error = $state('');
  let note = $state('');
  let busy = $state('');
  const editing = $derived(doc && kind?.versionOf ? kind.versionOf(doc.text ?? '') : '');
  const pushedVersion = $derived(doc && kind?.versionOf ? kind.versionOf(doc.saved ?? '') : '');

  async function refresh() {
    if (!doc || !kind?.versions) return;
    loading = true; error = '';
    const r = await kind.versions(doc);
    loading = false;
    if (!r.ok) { error = r.error; list = []; return; }
    list = sortVersions(r.data.versions);
    active = r.data.active;
  }
  // Again whenever another item is shown, or this one is pushed.
  $effect(() => { void doc?.address; void doc?.saved; if (can) void refresh(); });

  async function load(v: string) {
    if (!doc || !kind?.loadVersion) return;
    if (workspace.isDirty(doc) && !(await workspace.host.confirm(`Replace the edits in ${doc.ref.name} with version ${v}? Unsaved edits are lost (Save keeps them as a draft first).`))) return;
    busy = `load:${v}`; note = '';
    const r = await kind.loadVersion(doc, v);
    busy = '';
    if (!r.ok) { error = r.error; return; }
    doc.text = r.data;
    note = `Version ${v} is in the editor — Save keeps it as a draft, Sync pushes it.`;
  }
  async function activate(v: string) {
    if (!doc || !kind?.activate) return;
    if (!(await workspace.host.confirm(`Make ${v} the active version of ${doc.ref.name}? Runs without a pinned version use it from now on.`))) return;
    busy = `act:${v}`; note = '';
    const r = await kind.activate(doc, v);
    busy = '';
    if (!r.ok) { error = r.error; return; }
    note = `${v} is now active.`;
    await refresh();
  }
</script>

{#if !doc}
  <div class="note">Open a flow to see its versions.</div>
{:else if !kind?.versions}
  <div class="note">{kind?.label} items have no versions in the registry yet.</div>
{:else if doc.isNew}
  <div class="note">Not in the registry yet — Sync → Push creates its first version{editing ? ` (${editing})` : ''}.</div>
{:else}
  <div class="vp">
    <div class="bar">
      <span>{list.length} version{list.length === 1 ? '' : 's'} in the registry</span>
      {#if editing}<span class="dim">· editing <b class="mono">{editing}</b>{editing !== pushedVersion ? ' (not pushed)' : ''}</span>{/if}
      <span class="grow"></span>
      {#if note}<span class="ok">{note}</span>{/if}
      {#if error}<span class="err">{error}</span>{/if}
      <button class="lnk" onclick={refresh} disabled={loading}>{loading ? 'Loading…' : '↻ Refresh'}</button>
    </div>
    <table>
      <thead><tr><th>Version</th><th></th><th></th></tr></thead>
      <tbody>
        {#each list as v (v)}
          <tr class:act={v === active}>
            <td class="mono">{v}</td>
            <td>
              {#if v === active}<span class="pill on" title="What runs without a pinned version">active</span>{/if}
              {#if v === editing}<span class="pill ed" title="The version in the editor">in editor</span>{/if}
            </td>
            <td class="acts">
              <button class="lnk" disabled={!!busy} onclick={() => load(v)} title="Put this version's text in the editor">{busy === `load:${v}` ? 'Loading…' : 'Load'}</button>
              {#if v !== active}<button class="lnk" disabled={!!busy} onclick={() => activate(v)} title="Roll back or forward to this version">{busy === `act:${v}` ? 'Activating…' : 'Make active'}</button>{/if}
            </td>
          </tr>
        {:else}
          {#if !loading && !error}<tr><td colspan="3" class="dim">No versions listed.</td></tr>{/if}
        {/each}
      </tbody>
    </table>
  </div>
{/if}

<style>
  .note { padding: 14px; color: var(--text-4); font-size: 12.5px; }
  .vp { height: 100%; overflow: auto; font-size: 12.5px; }
  .bar { display: flex; align-items: center; gap: 8px; padding: 6px 12px; border-bottom: 1px solid var(--border); color: var(--text-2); position: sticky; top: 0; background: var(--bg-surface); }
  .grow { flex: 1; }
  .dim { color: var(--text-4); }
  .mono { font-family: var(--font-mono); }
  .ok { color: var(--ok, #15803d); font-size: 11.5px; }
  .err { color: var(--err, #b42318); font-size: 11.5px; }
  table { width: 100%; border-collapse: collapse; }
  th { text-align: left; font-size: 10.5px; text-transform: uppercase; letter-spacing: 0.05em; color: var(--text-4); font-weight: 600; padding: 5px 12px; }
  td { padding: 5px 12px; border-top: 1px solid var(--divider, var(--border)); }
  tr.act td { background: var(--runtime-bg, #e3f4ef); }
  .pill { font-size: 10.5px; padding: 1px 7px; border-radius: 10px; margin-right: 4px; }
  .pill.on { background: var(--runtime, #1a7158); color: #fff; }
  .pill.ed { background: var(--spec-bg); color: var(--spec-ink, var(--spec)); }
  .acts { text-align: right; white-space: nowrap; }
  .lnk { border: 0; background: none; color: var(--spec); cursor: pointer; font-size: 12px; padding: 0 0 0 10px; }
  .lnk:disabled { opacity: 0.5; cursor: default; }
</style>
