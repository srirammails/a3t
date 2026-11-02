<script lang="ts">
  /// A document's Save and Sync, wherever it is edited (its view, its code, its
  /// Form panel). The state lives on the document, so two bars for one document
  /// show the same thing and never push twice.
  ///
  ///   Save (Ctrl/⌘+S)   keep the edits as a draft here — nothing leaves the browser
  ///   Sync ▾ Push       send them to the registry, over what it has
  ///          Push as a new version   bump the version, keep the old one beside it
  ///          Merge      bring the registry's changes in (three-way, from the copy
  ///                     the edits started from); review, then Push
  /// The main Sync button checks the registry first: changed since the edits
  /// started → Merge, else → Push. The dropdown also lists every version the
  /// registry keeps (load one into the editor, or make it active), as the
  /// Versions panel does. (Deploy, Run and Debug are the views' own.)
  import { bumpVersion, sortVersions, type Doc, type Workspace } from '$lib/studio-core';

  /// `quiet`: only failures are written out (narrow bars); the chips say the rest.
  let { doc, workspace, valid = true, quiet = false }: { doc: Doc; workspace: Workspace; valid?: boolean; quiet?: boolean } = $props();

  const kind = $derived(workspace.kindOf(doc));
  const unsaved = $derived(workspace.isDirty(doc));
  const unpushed = $derived(workspace.isUnpushed(doc));
  const canDraft = $derived(typeof doc.text === 'string');
  const stale = $derived(!!doc.ui.draftStale);
  const conflicts = $derived((doc.ui.conflicts ?? []) as { path: (string | number)[] }[]);
  const version = $derived(kind.versionOf ? kind.versionOf(doc.text ?? '') : '');

  function say(ok: boolean, text: string) { doc.ui.saved = { ok, text }; }

  function save() {
    if (!canDraft || !unsaved) return;
    if (workspace.saveDraft(doc)) say(true, 'Saved as a draft — Sync pushes it to the registry.');
  }

  async function push(opts: { force?: boolean } = {}) {
    menu = false;
    if (!kind.push || !valid || doc.ui.saving) return;
    doc.ui.saving = true; doc.ui.saved = null;
    // Has the registry moved on since these edits started? Then Push would
    // overwrite someone's work: ask, and offer Merge instead.
    if (!opts.force && !doc.isNew) {
      const c = await workspace.remoteChanged(doc);
      if (c.changed && !(await workspace.host.confirm(`The registry’s copy of ${doc.ref.name} changed since your edits started. Push over it anyway? (Cancel, then Sync → Merge to bring its changes in.)`))) {
        doc.ui.saving = false; say(false, 'Not pushed — the registry changed; Merge first.'); return;
      }
    }
    const r = await workspace.push(doc);
    doc.ui.saving = false;
    say(r.ok, r.ok ? `Pushed${version ? ` ${version}` : ''} to the registry.` : r.error);
  }

  async function pushNew(part: 'patch' | 'minor' | 'major') {
    if (!kind.withVersion) return;
    const next = bumpVersion(version, part);
    if (!(await workspace.host.confirm(`Push ${doc.ref.name} as version ${next}? ${version ? `${version} stays in the registry; ` : ''}${next} becomes the active one.`))) { menu = false; return; }
    doc.text = kind.withVersion(doc.text ?? '', next);
    await push({ force: true });
  }

  async function merge() {
    menu = false;
    doc.ui.saving = true; doc.ui.saved = null;
    const r = await workspace.merge(doc);
    doc.ui.saving = false;
    if (!r.ok) { say(false, r.error); return; }
    say(!r.conflicts.length, r.conflicts.length
      ? `Merged with ${r.conflicts.length} conflict${r.conflicts.length > 1 ? 's' : ''} — your values kept there; check them, then Push.`
      : r.applied ? `Merged ${r.applied} change${r.applied > 1 ? 's' : ''} from the registry — review, then Push.` : 'Nothing to merge: the registry has no other changes.');
  }

  async function discard() {
    if (!(await workspace.host.confirm(`Discard the draft of ${doc.ref.name}? ${doc.isNew ? 'The new item stays open, unsaved.' : 'It goes back to the registry’s version.'}`))) return;
    workspace.discardDraft(doc);
    doc.ui.saved = null; doc.ui.conflicts = null;
  }

  /// Sync: check the registry, then merge (it moved on) or push (it did not).
  async function sync() {
    if (doc.ui.saving) return;
    if (doc.isNew) { await push({ force: true }); return; }
    doc.ui.saving = true; doc.ui.saved = null;
    const c = await workspace.remoteChanged(doc);
    doc.ui.saving = false;
    if (c.error) { say(false, c.error); return; }
    if (c.changed) await merge();
    else if (unpushed) await push({ force: true });
    else say(true, 'In sync with the registry.');
  }

  // ── Sync menu: checks the registry and lists its versions when opened ──
  let menu = $state(false);
  let checking = $state(false);
  let versions = $state<string[]>([]);
  let active = $state('');
  async function openMenu() {
    menu = !menu;
    if (!menu || doc.isNew) return;
    checking = true;
    const [, vs] = await Promise.all([workspace.remoteChanged(doc), kind.versions ? kind.versions(doc) : Promise.resolve(null)]);
    if (vs?.ok) { versions = sortVersions(vs.data.versions); active = vs.data.active; }
    checking = false;
  }
  async function loadVersion(v: string) {
    menu = false;
    if (!kind.loadVersion) return;
    if (workspace.isDirty(doc) && !(await workspace.host.confirm(`Replace the edits in ${doc.ref.name} with version ${v}? Unsaved edits are lost (Save keeps them as a draft first).`))) return;
    const r = await kind.loadVersion(doc, v);
    if (!r.ok) { say(false, r.error); return; }
    doc.text = r.data;
    say(true, `Version ${v} is in the editor.`);
  }
  async function activateVersion(v: string) {
    menu = false;
    if (!kind.activate || !(await workspace.host.confirm(`Make ${v} the active version of ${doc.ref.name}? Runs without a pinned version use it from now on.`))) return;
    const r = await kind.activate(doc, v);
    say(r.ok, r.ok ? `${v} is now active.` : r.error);
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
  const pathText = (p: (string | number)[]) => p.join('.') || '(the whole document)';
</script>

<svelte:window onkeydown={onkey} />

{#if kind.push}
  <span class="sb">
    {#if unsaved}<span class="chip unsaved" title="Edits not saved yet — Save (Ctrl/⌘+S) keeps them as a draft">unsaved</span>
    {:else if doc.draft}<span class="chip draft" title="Saved here {ago(doc.draft.at)}, not pushed to the registry">draft<button class="x" onclick={discard} title="Discard the draft">×</button></span>{/if}
    {#if stale}<span class="chip stale" title="The registry’s copy changed after these edits started — Sync → Merge brings its changes in">registry changed</span>{/if}
    {#if conflicts.length}<span class="chip stale" title={'Both changed — your value was kept:\n' + conflicts.map((c) => pathText(c.path)).join('\n')}>{conflicts.length} conflict{conflicts.length > 1 ? 's' : ''}</span>{/if}
    {#if doc.ui.saved && !(quiet && doc.ui.saved.ok)}<span class="msg" class:bad={!doc.ui.saved.ok} title={doc.ui.saved.text}>{doc.ui.saved.text}</span>{/if}
    {#if canDraft}
      <button class="btn ghost sm" disabled={!unsaved} onclick={save} title="Save a draft here (Ctrl/⌘+S) — nothing is sent">Save</button>
    {/if}
    <span class="split">
      <button class="btn spec sm main" disabled={doc.ui.saving || !valid} onclick={sync}
        title="Sync: check the registry — merge its changes if it moved on, else push yours">
        {doc.ui.saving ? 'Syncing…' : '⇅ Sync'}
      </button>
      <button class="btn spec sm caret" onclick={openMenu} aria-label="Sync options" aria-haspopup="menu" aria-expanded={menu}>▾</button>
      {#if menu}
        <span class="ov" role="presentation" onclick={() => (menu = false)}></span>
        <span class="menu" role="menu">
          <span class="mh">Sync with the registry{checking ? ' — checking…' : stale ? ' — it has changed' : ''}</span>
          <button role="menuitem" disabled={!unpushed || !valid} onclick={() => push()}>
            <span>⇡ Push{version ? ` ${version}` : ''}</span><small>{doc.isNew ? 'Create it in the registry' : 'Overwrite the registry’s copy with yours'}</small>
          </button>
          {#if kind.withVersion && !doc.isNew}
            <span class="sub">
              <span class="sl">⇡ Push as a new version</span>
              {#each ['patch', 'minor', 'major'] as const as part}
                <button role="menuitem" disabled={!valid} onclick={() => pushNew(part)} title="{version || '?'} → {bumpVersion(version, part)}; {version} stays">
                  {bumpVersion(version, part)} <small>{part}</small>
                </button>
              {/each}
            </span>
          {/if}
          <button role="menuitem" disabled={doc.isNew || checking || !stale} onclick={merge}>
            <span>⇅ Merge</span><small>{stale ? 'Bring the registry’s changes into yours; review, then Push' : checking ? 'Checking the registry…' : 'The registry has not changed since your edits started'}</small>
          </button>
          {#if kind.versions && !doc.isNew}
            <span class="mh vh">Versions in the registry</span>
            {#each versions as v (v)}
              <span class="vrow">
                <button class="vload" role="menuitem" onclick={() => loadVersion(v)} title="Load {v} into the editor">
                  <span class="mono">{v}</span>
                  {#if v === active}<span class="pill on">active</span>{/if}
                  {#if v === version}<span class="pill ed">in editor</span>{/if}
                </button>
                {#if v !== active && kind.activate}<button class="vact" onclick={() => activateVersion(v)} title="Make {v} the active version">make active</button>{/if}
              </span>
            {:else}
              <span class="vnone">{checking ? 'Loading…' : 'None listed.'}</span>
            {/each}
          {/if}
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
  .menu { position: absolute; right: 0; top: calc(100% + 4px); z-index: 51; min-width: 280px; display: flex; flex-direction: column; padding: 4px;
          background: var(--bg-surface); border: 1px solid var(--border); border-radius: var(--r-sm); box-shadow: var(--shadow-lg); }
  .mh { padding: 4px 10px 6px; font-size: 10.5px; text-transform: uppercase; letter-spacing: 0.05em; color: var(--text-4); }
  .menu button { display: flex; flex-direction: column; align-items: flex-start; gap: 1px; border: 0; background: transparent; padding: 7px 10px; border-radius: var(--r-sm);
                 cursor: pointer; text-align: left; color: var(--text-1); font-size: 13px; }
  .menu button:hover:not(:disabled) { background: var(--bg-subtle); }
  .menu button:disabled { opacity: 0.45; cursor: default; }
  .menu small { color: var(--text-3); font-size: 11.5px; }
  .vh { border-top: 1px solid var(--border); margin-top: 4px; padding-top: 8px; }
  .vrow { display: flex; align-items: center; }
  .menu .vload { flex: 1; flex-direction: row; align-items: center; gap: 6px; padding: 5px 10px; font-size: 12.5px; }
  .menu .vact { flex-direction: row; padding: 4px 8px; font-size: 11.5px; color: var(--spec); }
  .vnone { padding: 4px 10px 6px; font-size: 12px; color: var(--text-4); }
  .mono { font-family: var(--font-mono); }
  .pill { font-size: 10px; padding: 0 6px; border-radius: 8px; }
  .pill.on { background: var(--runtime, #1a7158); color: #fff; }
  .pill.ed { background: var(--spec-bg); color: var(--spec-ink, var(--spec)); }
  .menu { max-height: 70vh; overflow: auto; }
  .sub { display: flex; align-items: center; gap: 2px; padding: 3px 4px 3px 10px; flex-wrap: wrap; }
  .sl { font-size: 13px; color: var(--text-1); margin-right: 6px; }
  .sub button { flex-direction: row; align-items: baseline; gap: 4px; padding: 4px 8px; font-family: var(--font-mono); font-size: 12px; border: 1px solid var(--border); }
</style>
