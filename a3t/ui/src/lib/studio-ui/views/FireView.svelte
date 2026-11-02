<script lang="ts">
  /// A trigger's Fire view: what it is (type, where it listens, what it starts)
  /// and firing it with a payload, as its type would; the runs it starts show
  /// in the Runs panel below. Fires the trigger as saved.
  import YamlEditor from '$lib/components/editor/YamlEditor.svelte';
  import { fromText, type Doc, type Workspace } from '$lib/studio-core';
  import type { Studio } from '../layout.svelte.ts';

  let { doc, workspace, studio }: { doc: Doc; workspace: Workspace; studio: Studio } = $props();

  const kind = $derived(workspace.kindOf(doc));
  const t = $derived(doc.data ?? null);
  const type = $derived(t?.config?.type ?? '');
  const where = $derived(type === 'webhook' ? `POST /webhook${t?.config?.path ?? ''}` : type === 'event' ? `event type ${t?.config?.source ?? '—'}` : type === 'cron' ? `cron ${t?.config?.expression ?? ''}` : type);
  const targets = $derived((t?.targets ?? []) as any[]);
  const payload = $derived(fromText(doc.ui.payload));
  const edited = $derived(workspace.isDirty(doc));

  $effect(() => {
    if (!doc.loaded) void workspace.load(doc);
  });

  async function fire() {
    if (!kind.fire || doc.ui.firing || payload.error) return;
    doc.ui.firing = true;
    doc.ui.error = '';
    doc.ui.fired = null;
    const r = await kind.fire(doc, payload.value);
    doc.ui.firing = false;
    if (r.ok) {
      doc.ui.fired = { how: r.data.how, at: new Date().toLocaleTimeString(), detail: JSON.stringify(r.data.detail, null, 2) };
      studio.reveal('bottom', 'target-runs');
    } else doc.ui.error = r.error;
  }
</script>

<div class="fv">
  <div class="head">
    <span class="kind">{kind.label}{type ? ` · ${type}` : ''}</span>
    <span class="mono">{doc.ref.project}/{doc.ref.name}</span>
    {#if t && !t.enabled}<span class="paused">paused</span>{/if}
    <span class="grow"></span>
    <button class="btn spec sm" disabled={!t || doc.ui.firing || !!payload.error} onclick={fire}>{doc.ui.firing ? 'Firing…' : '⚡ Fire'}</button>
  </div>

  {#if !doc.loaded}
    <div class="note">Loading {doc.ref.name}…</div>
  {:else if doc.error}
    <div class="err">Could not load {doc.ref.name}: {doc.error}</div>
  {:else}
    <div class="body">
      <dl>
        <dt>Listens</dt><dd class="mono">{where || '—'}</dd>
        <dt>Starts</dt>
        <dd>
          {#each targets as x, i (i)}<span class="chip">{x.type === 'workflow' ? 'ƒ' : x.type} {x.namespace ? `${x.namespace}/` : ''}{x.name ?? x.agent_id ?? x.url ?? ''}</span>{/each}
          {#if !targets.length}<span class="dim">nothing yet</span>{/if}
        </dd>
        {#if t?.description}<dt>About</dt><dd>{t.description}</dd>{/if}
      </dl>
      {#if edited}<div class="hint">Config has unsaved changes; Fire uses the trigger as saved.</div>{/if}
      <div class="lbl">Payload <span class="dim">— JSON; {type === 'event' ? 'the event’s data' : type === 'webhook' ? 'the request body' : 'the flow’s input'}</span></div>
      {#if payload.error}<div class="err">Not valid JSON: {payload.error}</div>{/if}
      <div class="editor"><YamlEditor value={doc.ui.payload ?? ''} onchange={(v: string) => (doc.ui.payload = v)} /></div>
      {#if doc.ui.error}<div class="err">{doc.ui.error}</div>{/if}
      {#if doc.ui.fired}
        <div class="ok">Fired at {doc.ui.fired.at}: {doc.ui.fired.how}. Its runs show below.</div>
        <details><summary>What the platform answered</summary><pre>{doc.ui.fired.detail}</pre></details>
      {/if}
    </div>
  {/if}
</div>

<style>
  .fv { display: flex; flex-direction: column; height: 100%; min-height: 0; }
  .head { display: flex; align-items: center; gap: 10px; padding: 6px 12px; border-bottom: 1px solid var(--border); background: var(--bg-surface); }
  .kind { font-size: 11px; text-transform: uppercase; letter-spacing: 0.04em; color: var(--text-3); }
  .mono { font-family: var(--font-mono); font-size: 12.5px; }
  .paused { font-size: 10px; padding: 1px 6px; border-radius: 3px; background: var(--warn-bg); color: var(--warn); }
  .grow { flex: 1; }
  .body { flex: 1; min-height: 0; overflow-y: auto; padding: 14px 18px 30px; display: flex; flex-direction: column; gap: 10px; }
  dl { display: grid; grid-template-columns: 90px 1fr; gap: 6px 12px; margin: 0; font-size: 13px; max-width: 900px; }
  dt { color: var(--text-3); font-size: 11px; text-transform: uppercase; letter-spacing: 0.04em; padding-top: 2px; }
  dd { margin: 0; display: flex; flex-wrap: wrap; gap: 6px; }
  .chip { font-family: var(--font-mono); font-size: 12px; padding: 1px 8px; border-radius: 4px; background: var(--spec-bg); color: var(--spec-ink, var(--spec)); }
  .dim { color: var(--text-4); }
  .lbl { font-size: 11px; text-transform: uppercase; letter-spacing: 0.04em; color: var(--text-3); margin-top: 6px; }
  .lbl .dim { text-transform: none; letter-spacing: 0; }
  .editor { height: 180px; max-width: 900px; border: 1px solid var(--border); border-radius: var(--r-sm); overflow: hidden; display: flex; }
  .editor > :global(*) { flex: 1; }
  .hint { font-size: 12px; color: var(--warn, #b45309); }
  .ok { font-size: 12.5px; color: var(--ok, #15803d); }
  details { font-size: 12px; color: var(--text-3); max-width: 900px; }
  pre { margin: 6px 0 0; padding: 8px 10px; background: var(--bg-page); border: 1px solid var(--border); border-radius: var(--r-sm); font-family: var(--font-mono); font-size: 11.5px; white-space: pre-wrap; word-break: break-word; }
  .note { padding: 14px; color: var(--text-4); font-size: 12.5px; }
  .err { padding: 8px 10px; border-radius: var(--r-sm); background: var(--err-bg, #fde8e8); color: var(--err, #b42318); font-size: 12px; white-space: pre-wrap; max-width: 900px; }
</style>
