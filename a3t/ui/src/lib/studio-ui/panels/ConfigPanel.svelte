<script lang="ts">
  /// The Form panel (right region) for any item with a config: a form built
  /// from its own keys and values, saved with the document. Masked values stay
  /// as stored unless replaced; kinds that cannot save show it read-only.
  import ConfigForm from '../parts/ConfigForm.svelte';
  import SaveBar from '../parts/SaveBar.svelte';
  import { fromText, toText, type Doc, type Workspace } from '$lib/studio-core';

  let { doc, workspace }: { doc: Doc | null; workspace: Workspace } = $props();

  const kind = $derived(doc ? workspace.kindOf(doc) : null);
  const parsed = $derived(fromText(doc?.text));
  const masked = $derived(doc && kind?.masked ? kind.masked(doc) : new Set<string>());

  $effect(() => {
    if (doc && !doc.loaded && !doc.isNew) void workspace.load(doc);
  });
</script>

{#if !doc || !kind}
  <div class="note">Open an item to see its settings here.</div>
{:else}
  <div class="cp">
    <div class="head">
      <span class="title">{kind.label}{doc.type ? ` · ${doc.type}` : ''}</span>
      <span class="grow"></span>
      <SaveBar {doc} {workspace} valid={!parsed.error} />
    </div>
    <div class="body">
      {#if !doc.loaded}
        <div class="note">Loading…</div>
      {:else if doc.error}
        <div class="err">{doc.error}</div>
      {:else if parsed.error || !parsed.value || typeof parsed.value !== 'object' || Array.isArray(parsed.value)}
        <div class="err">The settings are not a JSON object{parsed.error ? `: ${parsed.error}` : ''} — fix them in the code view.</div>
      {:else}
        {#if !kind.push}<div class="ro">Read-only here{kind.console ? ` — edit in ${kind.console.title}` : ''}.</div>{/if}
        <!-- A fresh form per item: nothing half-typed carries over. -->
        {#key doc.address}
          <ConfigForm value={parsed.value} readonly={!kind.push} {masked} stack onchange={(v) => (doc.text = toText(v))} />
        {/key}
      {/if}
    </div>
  </div>
{/if}

<style>
  .cp { display: flex; flex-direction: column; height: 100%; min-height: 0; }
  .head { display: flex; align-items: center; gap: 8px; padding: 6px 12px; border-bottom: 1px solid var(--border); }
  .title { font-size: 11px; text-transform: uppercase; letter-spacing: 0.04em; color: var(--text-3); white-space: nowrap; }
  .grow { flex: 1; }
  .body { flex: 1; min-height: 0; overflow-y: auto; padding: 12px 14px 30px; }
  .ro { font-size: 12px; color: var(--text-4); margin-bottom: 8px; }
  .note { padding: 14px; color: var(--text-4); font-size: 12.5px; }
  .err { padding: 8px 10px; border-radius: var(--r-sm); background: var(--err-bg, #fde8e8); color: var(--err, #b42318); font-size: 12px; }
</style>
