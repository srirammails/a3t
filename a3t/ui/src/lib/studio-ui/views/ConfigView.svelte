<script lang="ts">
  /// A registry item's config as a document: a form built from its own keys and
  /// values, or the same as JSON text. Save (or Ctrl/⌘+S) writes it back through
  /// the item's kind; kinds without `save` show it read-only, with a way to the
  /// Console page that manages them.
  import YamlEditor from '$lib/components/editor/YamlEditor.svelte';
  import ConfigForm from '../parts/ConfigForm.svelte';
  import ModeSwitch from '../parts/ModeSwitch.svelte';
  import { fromText, toText, type Doc, type Workspace } from '$lib/studio-core';

  let { doc, workspace }: { doc: Doc; workspace: Workspace } = $props();

  const kind = $derived(workspace.kindOf(doc));
  const editable = $derived(!!kind.save);
  const dirty = $derived(workspace.isDirty(doc));
  const parsed = $derived(fromText(doc.text));
  const masked = $derived(kind.masked?.(doc) ?? new Set<string>());

  let mode = $state<'form' | 'text'>('form');
  let busy = $state(false);
  let message = $state<{ ok: boolean; text: string } | null>(null);

  $effect(() => {
    if (!doc.loaded) void workspace.load(doc);
  });

  async function save() {
    if (!editable || !dirty || busy) return;
    busy = true; message = null;
    const r = await workspace.save(doc);
    busy = false;
    message = r.ok ? { ok: true, text: 'Saved.' } : { ok: false, text: r.error };
  }
  const onkey = (e: KeyboardEvent) => {
    if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === 's') { e.preventDefault(); void save(); }
  };
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div class="cv" onkeydown={onkey}>
  <div class="head">
    <ModeSwitch {doc} {workspace} />
    <span class="kind">{kind.label}{doc.type ? ` · ${doc.type}` : ''}</span>
    <span class="mono">{doc.ref.project}/{doc.ref.name}</span>
    {#if dirty}<span class="dirty">unsaved</span>{/if}
    {#if message}<span class="msg" class:bad={!message.ok} title={message.text}>{message.text}</span>{/if}
    <span class="grow"></span>
    <div class="mode" role="group" aria-label="Show as">
      <button class:on={mode === 'form'} onclick={() => (mode = 'form')}>Form</button>
      <button class:on={mode === 'text'} onclick={() => (mode = 'text')}>JSON</button>
    </div>
    {#if kind.console}
      {@const c = kind.console}
      <button class="btn ghost sm" onclick={() => workspace.host.openConsole(c.section, c.title)}>Open in {c.title}</button>
    {/if}
    {#if editable}
      <button class="btn spec sm" disabled={!dirty || busy || !!parsed.error} onclick={save} title="Save (Ctrl/⌘+S)">{busy ? 'Saving…' : 'Save'}</button>
    {/if}
  </div>

  {#if !editable}<div class="note">Read-only here — edit it in {kind.console?.title ?? 'its Console page'}.</div>{/if}

  <div class="body">
    {#if !doc.loaded}
      <div class="note">Loading {doc.ref.name}…</div>
    {:else if doc.error}
      <div class="err">Could not load {doc.ref.name}: {doc.error}</div>
    {:else if mode === 'text' || parsed.error || !parsed.value || typeof parsed.value !== 'object'}
      {#if parsed.error}<div class="err">Not valid JSON: {parsed.error} — fix it here to use the form.</div>{/if}
      <div class="text"><YamlEditor value={doc.text ?? ''} readOnly={!editable} onchange={(v: string) => (doc.text = v)} /></div>
    {:else}
      <div class="form">
        <ConfigForm value={parsed.value} readonly={!editable} {masked} onchange={(v) => (doc.text = toText(v))} />
      </div>
    {/if}
  </div>
</div>

<style>
  .cv { display: flex; flex-direction: column; height: 100%; min-height: 0; }
  .head { display: flex; align-items: center; gap: 10px; padding: 6px 12px; border-bottom: 1px solid var(--border); background: var(--bg-surface); }
  .kind { font-size: 11px; text-transform: uppercase; letter-spacing: 0.04em; color: var(--text-3); }
  .mono { font-family: var(--font-mono); font-size: 12.5px; }
  .dirty { font-family: var(--font-mono); font-size: 10px; color: var(--warn); background: var(--warn-bg); padding: 2px 6px; border-radius: 3px; }
  .msg { font-size: 11.5px; color: var(--ok, #15803d); max-width: 30%; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .msg.bad { color: var(--err, #b42318); }
  .grow { flex: 1; }
  .mode { display: flex; gap: 2px; background: var(--bg-subtle); border-radius: var(--r-sm); padding: 2px; }
  .mode button { border: 0; background: transparent; padding: 3px 10px; border-radius: 3px; font-size: 12px; color: var(--text-3); cursor: pointer; }
  .mode button.on { background: var(--bg-surface); color: var(--text-1); box-shadow: var(--shadow-sm); }
  .body { flex: 1; min-height: 0; overflow: auto; display: flex; flex-direction: column; }
  .form { padding: 14px 16px 30px; }
  .text { flex: 1; min-height: 0; }
  .note { padding: 8px 14px; color: var(--text-4); font-size: 12.5px; }
  .err { margin: 10px 14px; padding: 8px 10px; border-radius: var(--r-sm); background: var(--err-bg, #fde8e8); color: var(--err, #b42318); font-size: 12px; }
</style>
