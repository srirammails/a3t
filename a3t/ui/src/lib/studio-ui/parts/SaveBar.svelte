<script lang="ts">
  /// Unsaved marker, the outcome of the last save, and Save (Ctrl/⌘+S) for a
  /// document whose kind can save. Shown wherever the document is edited (its
  /// code view, its Form panel); the state lives on the document, so two bars
  /// for one document show the same thing and never save twice.
  import type { Doc, Workspace } from '$lib/studio-core';

  let { doc, workspace, valid = true }: { doc: Doc; workspace: Workspace; valid?: boolean } = $props();

  const kind = $derived(workspace.kindOf(doc));
  const dirty = $derived(workspace.isDirty(doc));

  async function save() {
    if (!kind.save || !dirty || !valid || doc.ui.saving) return;
    doc.ui.saving = true;
    doc.ui.saved = null;
    const r = await workspace.save(doc);
    doc.ui.saving = false;
    doc.ui.saved = r.ok ? { ok: true, text: 'Saved.' } : { ok: false, text: r.error };
  }

  /// Ctrl/⌘+S saves the active document only.
  function onkey(e: KeyboardEvent) {
    if (!(e.ctrlKey || e.metaKey) || e.key.toLowerCase() !== 's' || workspace.active !== doc || !kind.save) return;
    e.preventDefault();
    void save();
  }
</script>

<svelte:window onkeydown={onkey} />

{#if kind.save}
  <span class="sb">
    {#if dirty}<span class="dirty">unsaved</span>{/if}
    {#if doc.ui.saved}<span class="msg" class:bad={!doc.ui.saved.ok} title={doc.ui.saved.text}>{doc.ui.saved.text}</span>{/if}
    <button class="btn spec sm" disabled={!dirty || !valid || doc.ui.saving} onclick={save} title="Save (Ctrl/⌘+S)">{doc.ui.saving ? 'Saving…' : 'Save'}</button>
  </span>
{/if}

<style>
  .sb { display: inline-flex; align-items: center; gap: 8px; min-width: 0; }
  .dirty { font-family: var(--font-mono); font-size: 10px; color: var(--warn); background: var(--warn-bg); padding: 2px 6px; border-radius: 3px; }
  .msg { font-size: 11.5px; color: var(--ok, #15803d); max-width: 220px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .msg.bad { color: var(--err, #b42318); }
</style>
