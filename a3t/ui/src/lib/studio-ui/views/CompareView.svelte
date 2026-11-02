<script lang="ts">
  /// The split view, in the document's own tab: yours (left) beside another
  /// copy (right), differences marked, each side's version underneath.
  ///   sync     the registry moved on: Merge it into yours, or Push yours over it
  ///   merge    the merge left conflicts: finish by hand — the arrows take the
  ///            registry's side of a change, or edit the left — then Sync
  ///   history  two versions (or one and the editor), read-only
  import DiffEditor from '$lib/components/editor/DiffEditor.svelte';
  import type { Doc, Workspace } from '$lib/studio-core';
  import { closeCompare, conflicted, merge, push, sync, type Compare } from '../sync';

  let { doc, workspace }: { doc: Doc; workspace: Workspace } = $props();

  const c = $derived(doc.ui.compare as Compare);
  const live = $derived(c.mode !== 'history');
  /// Live modes edit the document itself on the left.
  const leftText = $derived(live ? doc.text ?? '' : c.left.text);
  const leftVersion = $derived(live ? workspace.kindOf(doc).versionOf?.(doc.text ?? '') ?? c.left.version : c.left.version);
  const title = $derived(c.mode === 'sync' ? 'The registry has a newer copy' : c.mode === 'merge' ? `Merged — ${c.conflicts?.length ?? 0} conflict${c.conflicts?.length === 1 ? '' : 's'} to finish` : 'Compare versions');
  const busy = $derived(!!doc.ui.saving);

  /// The version to put in the editor: the one that is not the editor already.
  const other = $derived(c.mode === 'history' ? (c.left.label === 'In the editor' ? c.right : c.left) : null);
  function useOther() {
    if (!other) return;
    doc.text = other.text;
    closeCompare(doc);
  }
</script>

<div class="cv">
  <div class="head">
    <span class="t">{title}</span>
    {#if c.mode === 'merge' && c.conflicts?.length}
      <span class="cf" title="Both sides changed these; your value was kept">{c.conflicts.map((x) => x.path.join('.') || '(document)').join(' · ')}</span>
    {/if}
    {#if doc.ui.saved}<span class="msg" class:bad={!doc.ui.saved.ok} title={doc.ui.saved.text}>{doc.ui.saved.text}</span>{/if}
    <span class="grow"></span>
    {#if c.mode === 'sync'}
      <button class="btn spec sm" disabled={busy || !conflicted(doc)} onclick={() => merge(doc, workspace)} title="Merge the registry’s changes into yours (automatic where it can be)">⇅ Merge</button>
      <button class="btn ghost sm danger" disabled={busy || !conflicted(doc)} onclick={() => push(doc, workspace)} title="Overwrite the registry’s copy with yours">⇡ Push yours</button>
    {:else if c.mode === 'merge'}
      <button class="btn spec sm" disabled={busy} onclick={() => { closeCompare(doc); void sync(doc, workspace); }} title="Push the merged text (checks the registry again first)">⇅ Sync</button>
    {:else}
      {#if other}<button class="btn ghost sm" onclick={useOther} title="Put this version’s text in the editor (unsaved; Save keeps it as a draft)">Use {other.label} in the editor</button>{/if}
    {/if}
    <button class="btn ghost sm" onclick={() => closeCompare(doc)} title="Back to the editor">Close</button>
  </div>
  <div class="body">
    {#key `${c.mode}:${c.right.version}:${c.left.version}`}
      <DiffEditor left={leftText} right={c.right.text} editable={live} onchange={(t) => { if (live) doc.text = t; }} />
    {/key}
  </div>
  <div class="foot">
    <div class="side"><span class="lb">{c.left.label}</span>{#if leftVersion}<span class="ver">v{leftVersion}</span>{/if}{#if live}<span class="hint">editable</span>{/if}</div>
    <div class="side"><span class="lb">{c.right.label}</span>{#if c.right.version}<span class="ver">v{c.right.version}</span>{/if}</div>
  </div>
</div>

<style>
  .cv { display: flex; flex-direction: column; height: 100%; min-height: 0; }
  .head { display: flex; align-items: center; gap: 8px; padding: 6px 12px; border-bottom: 1px solid var(--border); background: var(--bg-surface); }
  .t { font-weight: 600; font-size: 12.5px; }
  .cf { font: 11px var(--font-mono); color: var(--err, #b42318); background: var(--err-bg, #fde8e8); padding: 2px 6px; border-radius: 3px; max-width: 40%; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .msg { font-size: 11.5px; color: var(--ok, #15803d); max-width: 30%; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .msg.bad { color: var(--warn, #b45309); }
  .grow { flex: 1; }
  .danger { color: var(--err, #b42318); }
  .body { flex: 1; min-height: 0; }
  .foot { display: grid; grid-template-columns: 1fr 1fr; border-top: 1px solid var(--border); background: var(--bg-subtle); }
  .side { display: flex; align-items: center; gap: 8px; padding: 5px 12px; font-size: 12px; color: var(--text-2); }
  .side + .side { border-left: 1px solid var(--border); }
  .lb { font-weight: 500; }
  .ver { font: 11px var(--font-mono); color: var(--spec-ink, var(--spec)); background: var(--spec-bg); padding: 1px 6px; border-radius: 3px; }
  .hint { font-size: 10.5px; color: var(--text-4); }
</style>
