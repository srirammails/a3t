<script lang="ts">
  /// A document's Save and Push, wherever it is edited (its view, its code, its
  /// Form panel). The state lives on the document, so two bars for one document
  /// show the same thing and never push twice.
  ///
  ///   Save (Ctrl/⌘+S)  keep the edits as a draft here — nothing leaves the browser
  ///   Push             send it to the registry; the draft is done with
  /// (Deploy — onto a runtime — and Run / Debug are the views' own buttons.)
  import type { Doc, Workspace } from '$lib/studio-core';

  /// `quiet`: only failures are written out (narrow bars); the chip says the rest.
  let { doc, workspace, valid = true, quiet = false }: { doc: Doc; workspace: Workspace; valid?: boolean; quiet?: boolean } = $props();

  const kind = $derived(workspace.kindOf(doc));
  const unsaved = $derived(workspace.isDirty(doc));
  const unpushed = $derived(workspace.isUnpushed(doc));
  const canDraft = $derived(typeof doc.text === 'string');

  function save() {
    if (!canDraft || !unsaved) return;
    if (workspace.saveDraft(doc)) doc.ui.saved = { ok: true, text: 'Saved as a draft — Push sends it to the registry.' };
  }
  async function push() {
    if (!kind.push || !unpushed || !valid || doc.ui.saving) return;
    doc.ui.saving = true;
    doc.ui.saved = null;
    const r = await workspace.push(doc);
    doc.ui.saving = false;
    doc.ui.saved = r.ok ? { ok: true, text: 'Pushed to the registry.' } : { ok: false, text: r.error };
  }
  async function discard() {
    if (!(await workspace.host.confirm(`Discard the draft of ${doc.ref.name}? ${doc.isNew ? 'The new item stays open, unsaved.' : 'It goes back to the registry’s version.'}`))) return;
    workspace.discardDraft(doc);
    doc.ui.saved = null;
  }

  /// Ctrl/⌘+S saves the active document only.
  function onkey(e: KeyboardEvent) {
    if (!(e.ctrlKey || e.metaKey) || e.key.toLowerCase() !== 's' || workspace.active !== doc) return;
    e.preventDefault();
    save();
  }
  const ago = (t: number) => {
    const s = Math.round((Date.now() - t) / 1000);
    return s < 60 ? 'just now' : s < 3600 ? `${Math.round(s / 60)} min ago` : new Date(t).toLocaleString();
  };
</script>

<svelte:window onkeydown={onkey} />

{#if kind.push}
  <span class="sb">
    {#if unsaved}<span class="chip unsaved" title="Edits not saved yet — Save (Ctrl/⌘+S) keeps them as a draft">unsaved</span>
    {:else if doc.draft}<span class="chip draft" title="Saved here {ago(doc.draft.at)}, not pushed to the registry">draft<button class="x" onclick={discard} title="Discard the draft">×</button></span>{/if}
    {#if doc.ui.draftStale}<span class="chip stale" title="The registry’s copy changed after this draft was made; pushing replaces it">registry changed</span>{/if}
    {#if doc.ui.saved && !(quiet && doc.ui.saved.ok)}<span class="msg" class:bad={!doc.ui.saved.ok} title={doc.ui.saved.text}>{doc.ui.saved.text}</span>{/if}
    {#if canDraft}
      <button class="btn ghost sm" disabled={!unsaved} onclick={save} title="Save a draft here (Ctrl/⌘+S) — nothing is sent">Save</button>
    {/if}
    <button class="btn spec sm" disabled={!unpushed || !valid || doc.ui.saving} onclick={push} title="Push to the registry">{doc.ui.saving ? 'Pushing…' : 'Push'}</button>
  </span>
{/if}

<style>
  .sb { display: inline-flex; align-items: center; gap: 6px; min-width: 0; }
  .chip { display: inline-flex; align-items: center; gap: 3px; font-family: var(--font-mono); font-size: 10px; padding: 2px 6px; border-radius: 3px; white-space: nowrap; }
  .unsaved { color: var(--warn); background: var(--warn-bg); }
  .draft { color: var(--spec-ink, var(--spec)); background: var(--spec-bg); }
  .stale { color: var(--err, #b42318); background: var(--err-bg, #fde8e8); }
  .x { border: 0; background: none; padding: 0 0 0 2px; color: inherit; cursor: pointer; font-size: 11px; line-height: 1; opacity: 0.7; }
  .x:hover { opacity: 1; }
  .msg { font-size: 11.5px; color: var(--ok, #15803d); max-width: 220px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .msg.bad { color: var(--err, #b42318); }
</style>
