<script lang="ts">
  /// Flow Studio: the flow editor as a small IDE on the reusable studio shell.
  /// Explorer (left: the project's flows, resources by kind, triggers,
  /// templates, canvases), open documents in tabs (flows edit in graph / code /
  /// form; other items preview), and Runs / Problems below. Every region resizes,
  /// minimizes to its edge and maximizes; the layout and open tabs are kept.
  import SubmenuToggle from '$lib/components/shared/SubmenuToggle.svelte';
  import StudioShell from '$lib/components/studio/StudioShell.svelte';
  import Explorer from '$lib/components/studio/Explorer.svelte';
  import DocTabs from '$lib/components/studio/DocTabs.svelte';
  import FlowDocument from '$lib/components/studio/FlowDocument.svelte';
  import ItemPreview from '$lib/components/studio/ItemPreview.svelte';
  import FlowRunPanel from '$lib/components/run/FlowRunPanel.svelte';
  import FlowEditorPane from '$lib/components/editor/FlowEditorPane.svelte';
  import { Studio } from '$lib/components/studio/layout.svelte.ts';
  import { parse, problems, stepKeys, newFlowYaml, type Doc, type OpenItem } from '$lib/components/studio/flow';
  import { getActivePanel } from '$lib/stores/panels.svelte.ts';
  import { getProject } from '$lib/stores/context.svelte.ts';

  const studio = new Studio('flows', {
    left: { size: 280, minimized: false, active: 'explorer' },
    right: { size: 380, minimized: false, active: 'form' },
    bottom: { size: 320, minimized: true, active: 'runs' },
  });

  // ── Open documents (kept per browser; texts reload from the server) ──
  const TABS_KEY = 'a3t.studio.flows.tabs';
  const ICONS: Record<string, string> = { flow: 'ƒ', resource: '◆', trigger: '⚡', template: '▤', canvas: '▦' };

  function restore(): { docs: Doc[]; active: string | null } {
    try {
      const v = JSON.parse(localStorage.getItem(TABS_KEY) ?? 'null');
      if (v && Array.isArray(v.docs)) {
        const docs = v.docs.filter((d: Doc) => !d.isNew).map((d: Doc) => ({ ...d, yaml: undefined, saved: undefined, loaded: false }));
        return { docs, active: docs.some((d: Doc) => d.id === v.active) ? v.active : docs[0]?.id ?? null };
      }
    } catch { /* private window */ }
    return { docs: [], active: null };
  }
  const restored = restore();
  let docs = $state<Doc[]>(restored.docs);
  let activeId = $state<string | null>(restored.active);
  let newCount = 0;

  function persist() {
    try {
      localStorage.setItem(TABS_KEY, JSON.stringify({ active: activeId, docs: docs.filter((d) => !d.isNew).map(({ id, kind, ns, name, data }) => ({ id, kind, ns, name, data: kind === 'flow' ? undefined : data })) }));
    } catch { /* private window */ }
  }

  const idOf = (it: OpenItem) => `${it.kind}:${it.ns}/${it.id ?? it.name}`;

  function open(it: OpenItem) {
    const id = idOf(it);
    if (!docs.some((d) => d.id === id)) docs.push({ id, kind: it.kind, ns: it.ns, name: it.name, data: it.data, loaded: it.kind !== 'flow' });
    activeId = id;
    persist();
  }

  function close(id: string) {
    const d = docs.find((x) => x.id === id);
    if (d?.kind === 'flow' && (d.yaml ?? '') !== (d.saved ?? '') && !confirm(`${d.name} has changes that are not deployed. Close it anyway?`)) return;
    const i = docs.findIndex((x) => x.id === id);
    docs.splice(i, 1);
    if (activeId === id) activeId = docs[Math.min(i, docs.length - 1)]?.id ?? null;
    persist();
  }

  function newFlow(ns?: string) {
    newCount += 1;
    const n = ns || getProject() || 'a3t';
    const yaml = newFlowYaml(n, newCount);
    const doc: Doc = { id: `flow:new:${newCount}`, kind: 'flow', ns: n, name: newCount > 1 ? `new-flow-${newCount}` : 'new-flow', yaml, saved: '', isNew: true };
    docs.push(doc);
    activeId = doc.id;
  }

  function deployed(doc: Doc, ns: string, name: string) {
    const id = `flow:${ns}/${name}`;
    const other = docs.findIndex((d) => d.id === id && d !== doc);
    if (other >= 0) docs.splice(other, 1);
    doc.id = id; doc.ns = ns; doc.name = name; doc.isNew = false; doc.loaded = true;
    activeId = id;
    persist();
  }

  // A flow named in the address (/flow-editor/<ns>~<name>) opens on arrival.
  const sel = getActivePanel()?.props?.selectedId as string | undefined;
  if (sel && sel.includes('~')) {
    const [ns, name] = sel.split('~');
    open({ kind: 'flow', ns, name });
  }

  const active = $derived(docs.find((d) => d.id === activeId) ?? null);
  const activeFlow = $derived(active?.kind === 'flow' ? active : null);
  const parsed = $derived(activeFlow ? parse(activeFlow.yaml).doc : null);
  const issues = $derived(activeFlow?.loaded || activeFlow?.isNew ? problems(activeFlow.yaml) : []);
  const tabs = $derived(docs.map((d) => ({
    id: d.id, title: d.name, icon: ICONS[d.kind], hint: `${d.kind} ${d.ns}/${d.name}`,
    dirty: d.kind === 'flow' && (d.isNew || (d.loaded && (d.yaml ?? '') !== (d.saved ?? ''))),
  })));

  // ── Run / Debug: the bottom Runs panel ──
  let runDialog = $state(false);
  let runDebug = $state(false);
  function run(debug: boolean) {
    runDebug = debug;
    studio.reveal('bottom', 'runs');
    runDialog = true;
  }
</script>

<div class="fs">
  <StudioShell
    {studio}
    left={{ panels: [{ id: 'explorer', title: 'Explorer' }], panel: leftPanel }}
    right={{ panels: [{ id: 'form', title: 'Form' }, { id: 'ai', title: 'AI' }], panel: rightPanel }}
    bottom={{ panels: [{ id: 'runs', title: 'Runs' }, { id: 'problems', title: 'Problems' }], panel: bottomPanel, badge: (id) => (id === 'problems' && issues.length ? issues.length : null) }}
    center={centerArea}
  />
</div>

{#snippet leftPanel(_id: string)}
  <Explorer onopen={open} onnewflow={(ns) => newFlow(ns)} activeId={activeId ?? ''} />
{/snippet}

{#snippet centerArea()}
  <div class="center-head">
    <SubmenuToggle appType="flow-editor" />
    <DocTabs {studio} {tabs} active={activeId} onselect={(id) => { activeId = id; persist(); }} onclose={close}>
      {#snippet actions()}
        <button class="new" onclick={() => newFlow()} title="New flow">+ New flow</button>
      {/snippet}
    </DocTabs>
  </div>
  <div class="center-body">
    {#if !active}
      <div class="empty">
        <p>Open a flow from the Explorer, or start a new one.</p>
        <button class="btn spec sm" onclick={() => newFlow()}>+ New flow</button>
      </div>
    {:else}
      {#key active.id}
        {#if active.kind === 'flow'}
          <FlowDocument doc={active} onrun={run} ondeployed={(ns, name) => deployed(active, ns, name)} />
        {:else}
          <ItemPreview doc={active} />
        {/if}
      {/key}
    {/if}
  </div>
{/snippet}

{#snippet rightPanel(id: string)}
  {#if activeFlow && (activeFlow.loaded || activeFlow.isNew)}
    {#key `${activeFlow.id}:${id}`}
      <FlowEditorPane part={id === 'ai' ? 'ai' : 'form'} aiKind="flow" aiName={`${activeFlow.ns}/${activeFlow.name}`}
        initialYaml={activeFlow.yaml ?? ''} onchange={(v: string) => (activeFlow!.yaml = v)} />
    {/key}
  {:else}
    <div class="note">{active ? `The ${id === 'ai' ? 'AI assistant' : 'form'} works on flows.` : 'Open a flow to edit it here.'}</div>
  {/if}
{/snippet}

{#snippet bottomPanel(id: string)}
  {#if id === 'runs'}
    {#if activeFlow && !activeFlow.isNew}
      <FlowRunPanel ns={activeFlow.ns} name={activeFlow.name} schema={parsed?.input?.schema ?? null} steps={stepKeys(parsed?.do)} bind:dialogOpen={runDialog} bind:debugMode={runDebug} />
    {:else}
      <div class="note">{activeFlow ? 'Deploy this flow to run it.' : 'Open a flow to see its runs.'}</div>
    {/if}
  {:else}
    {#if !activeFlow}
      <div class="note">Open a flow to check it.</div>
    {:else if !issues.length}
      <div class="note ok">No problems in {activeFlow.name}.</div>
    {:else}
      <ul class="problems">
        {#each issues as p}<li class={p.level}><span>{p.level === 'error' ? '✗' : '!'}</span>{p.message}</li>{/each}
      </ul>
    {/if}
  {/if}
{/snippet}

<style>
  .fs { height: 100%; min-height: 0; }
  .center-head { display: flex; align-items: center; gap: 4px; padding-left: 6px; background: var(--bg-page); border-bottom: 1px solid var(--border); }
  .center-head :global(.tabs) { flex: 1; border-bottom: 0; }
  .center-body { flex: 1; min-height: 0; display: flex; flex-direction: column; }
  .center-body > :global(*) { flex: 1; min-height: 0; }
  .new { border: 0; background: transparent; color: var(--spec); font-size: 12px; padding: 0 10px; cursor: pointer; white-space: nowrap; }
  .new:hover { background: var(--bg-subtle); }
  .empty { display: flex; flex-direction: column; align-items: center; justify-content: center; gap: 10px; color: var(--text-3); }
  .note { padding: 14px; color: var(--text-4); font-size: 12.5px; }
  .note.ok { color: var(--ok, #15803d); }
  .problems { list-style: none; margin: 0; padding: 8px 12px; font-size: 12.5px; }
  .problems li { display: flex; gap: 8px; padding: 3px 0; }
  .problems li.error span { color: var(--err, #b42318); }
  .problems li.warning span { color: var(--warn, #b45309); }
</style>
