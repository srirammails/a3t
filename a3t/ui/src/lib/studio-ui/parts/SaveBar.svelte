<script lang="ts">
  /// A document's Save and Sync, wherever it is edited (its view, its code, its
  /// Form panel). The state lives on the document, so every bar for it agrees.
  ///
  ///   Save (Ctrl/⌘+S)  keep the edits as a draft here — nothing leaves the browser
  ///   Sync ▾           like Run ▾: the main button does the last one picked
  ///     Sync      check the registry; unchanged → push, newer → split view
  ///     Versions  the versions the registry keeps (bottom panel)
  ///     Merge     the registry's changes into yours   } only when the registry
  ///     Push      yours over the registry's copy      } moved on (a conflict)
  /// See ../sync.ts. (Deploy, Run and Debug are the views' own Run ▾.)
  import type { Doc, Workspace } from '$lib/studio-core';
  import { conflicted, merge, push, sync, type SyncMode } from '../sync';

  /// `quiet`: only failures are written out (narrow bars); the chips say the rest.
  let { doc, workspace, valid = true, quiet = false }: { doc: Doc; workspace: Workspace; valid?: boolean; quiet?: boolean } = $props();

  const kind = $derived(workspace.kindOf(doc));
  const unsaved = $derived(workspace.isDirty(doc));
  const canDraft = $derived(typeof doc.text === 'string');
  const conflict = $derived(conflicted(doc));
  const conflicts = $derived((doc.ui.conflicts ?? []) as { path: (string | number)[] }[]);
  const busy = $derived(!!doc.ui.saving);

  function save() {
    if (!canDraft || !unsaved) return;
    if (workspace.saveDraft(doc)) doc.ui.saved = { ok: true, text: 'Saved as a draft — Sync sends it to the registry.' };
  }
  async function discard() {
    if (!(await workspace.host.confirm(`Discard the draft of ${doc.ref.name}? ${doc.isNew ? 'The new item stays open, unsaved.' : 'It goes back to the registry’s version.'}`))) return;
    workspace.discardDraft(doc);
    doc.ui.saved = null; doc.ui.conflicts = null; doc.ui.compare = null;
  }

  // ── Sync ▾ ──
  const MODES: Record<SyncMode, { label: string; hint: string }> = {
    sync: { label: '⇅ Sync', hint: 'Check the registry: push yours if it has not changed, else show both side by side' },
    versions: { label: '☰ Versions', hint: 'The versions the registry keeps; compare any two' },
    merge: { label: '⤨ Merge', hint: 'Bring the registry’s newer changes into yours — by hand where they clash' },
    push: { label: '⇡ Push', hint: 'Overwrite the registry’s newer copy with yours' },
  };
  let mode = $state<SyncMode>((() => { try { const m = localStorage.getItem('a3t.studio.syncMode'); return m === 'versions' ? m : 'sync'; } catch { return 'sync'; } })());
  let menu = $state(false);
  const enabled = (m: SyncMode) => (m === 'merge' || m === 'push' ? conflict && !doc.isNew : m === 'versions' ? !!kind.versions : valid);
  /// Merge and Push are for a conflict; without one the main button is Sync.
  const shown = $derived<SyncMode>(enabled(mode) ? mode : 'sync');
  function go(m: SyncMode) {
    menu = false;
    if (m === 'sync') void sync(doc, workspace);
    else if (m === 'merge') void merge(doc, workspace);
    else if (m === 'push') void push(doc, workspace);
    else doc.ui.reveal = { region: 'bottom', panel: 'versions' };
  }
  function pick(m: SyncMode) {
    mode = m;
    try { localStorage.setItem('a3t.studio.syncMode', m); } catch { /* private window */ }
    go(m);
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
    {:else if doc.draft}<span class="chip draft" title="Saved here {ago(doc.draft.at)}, not in the registry yet">draft<button class="x" onclick={discard} title="Discard the draft">×</button></span>{/if}
    {#if conflict}<span class="chip stale" title="The registry has a newer copy than the one your edits started from — Merge or Push">registry newer</span>{/if}
    {#if conflicts.length}<span class="chip stale" title={'Both changed — your value was kept:\n' + conflicts.map((c) => c.path.join('.')).join('\n')}>{conflicts.length} conflict{conflicts.length > 1 ? 's' : ''}</span>{/if}
    {#if doc.ui.saved && !(quiet && doc.ui.saved.ok) && !doc.ui.compare}<span class="msg" class:bad={!doc.ui.saved.ok} title={doc.ui.saved.text}>{doc.ui.saved.text}</span>{/if}
    {#if canDraft}
      <button class="btn ghost sm" disabled={!unsaved} onclick={save} title="Save a draft here (Ctrl/⌘+S) — nothing is sent">Save</button>
    {/if}
    <span class="split">
      <button class="btn spec sm main" disabled={busy || !enabled(shown)} onclick={() => go(shown)} title={MODES[shown].hint}>
        {busy ? 'Syncing…' : MODES[shown].label}
      </button>
      <button class="btn spec sm caret" onclick={() => (menu = !menu)} aria-label="Sync, versions, merge or push" aria-haspopup="menu" aria-expanded={menu}>▾</button>
      {#if menu}
        <span class="ov" role="presentation" onclick={() => (menu = false)}></span>
        <span class="menu" role="menu">
          {#each ['sync', 'versions', 'merge', 'push'] as const as m (m)}
            <button role="menuitem" class:on={mode === m} disabled={busy || !enabled(m)} onclick={() => pick(m)}>
              <span>{MODES[m].label}</span><small>{(m === 'merge' || m === 'push') && !conflict ? 'Only when the registry has a newer copy — Sync finds out' : MODES[m].hint}</small>
            </button>
          {/each}
        </span>
      {/if}
    </span>
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
  .split { position: relative; display: inline-flex; }
  .main { border-top-right-radius: 0; border-bottom-right-radius: 0; }
  .caret { border-top-left-radius: 0; border-bottom-left-radius: 0; padding-left: 6px; padding-right: 6px; border-left: 1px solid rgba(255, 255, 255, 0.35); }
  .ov { position: fixed; inset: 0; z-index: 50; }
  .menu { position: absolute; right: 0; top: calc(100% + 4px); z-index: 51; min-width: 290px; display: flex; flex-direction: column; padding: 4px;
          background: var(--bg-surface); border: 1px solid var(--border); border-radius: var(--r-sm); box-shadow: var(--shadow-lg); }
  .menu button { display: flex; flex-direction: column; align-items: flex-start; gap: 1px; border: 0; background: transparent; padding: 7px 10px; border-radius: var(--r-sm);
                 cursor: pointer; text-align: left; color: var(--text-1); font-size: 13px; }
  .menu button:hover:not(:disabled), .menu button.on { background: var(--bg-subtle); }
  .menu button:disabled { opacity: 0.45; cursor: default; }
  .menu small { color: var(--text-3); font-size: 11.5px; }
</style>
