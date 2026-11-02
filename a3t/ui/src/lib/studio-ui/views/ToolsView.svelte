<script lang="ts">
  /// An MCP server's Tools view: the tools it offers (listed on open), and
  /// calling one with arguments — a form from the tool's input schema, or JSON.
  /// A call runs the tool for real on that server.
  import ConfigForm from '../parts/ConfigForm.svelte';
  import { fromText, toText, type Doc, type Tool, type Workspace } from '$lib/studio-core';

  let { doc, workspace }: { doc: Doc; workspace: Workspace } = $props();

  const kind = $derived(workspace.kindOf(doc));
  const tools = $derived((doc.ui.tools ?? null) as Tool[] | null);
  const tool = $derived(tools?.find((t) => t.name === doc.ui.tool) ?? null);
  const args = $derived(fromText(doc.ui.args));
  let filter = $state('');
  let asJson = $state(false);
  const shown = $derived((tools ?? []).filter((t) => !filter.trim() || `${t.name} ${t.description}`.toLowerCase().includes(filter.trim().toLowerCase())));

  async function list() {
    if (!kind.tools || doc.ui.listing) return;
    doc.ui.listing = true;
    doc.ui.error = '';
    const r = await kind.tools(doc);
    doc.ui.listing = false;
    if (r.ok) doc.ui.tools = r.data;
    else doc.ui.error = r.error;
  }

  $effect(() => {
    if (!doc.loaded) void workspace.load(doc);
    if (doc.ui.tools === null && !doc.ui.listing && !doc.ui.error) void list();
  });

  /// Blank arguments from the tool's input schema (its properties, by type).
  function pick(t: Tool) {
    doc.ui.tool = t.name;
    doc.ui.result = null;
    const props = t.inputSchema?.properties ?? {};
    const blank = (p: any): unknown => (p?.type === 'number' || p?.type === 'integer' ? 0 : p?.type === 'boolean' ? false : p?.type === 'object' ? {} : p?.type === 'array' ? [] : '');
    doc.ui.args = toText(Object.fromEntries(Object.entries<any>(props).map(([k, p]) => [k, p?.default ?? blank(p)])));
  }

  async function call() {
    if (!tool || doc.ui.calling || args.error || !kind.callTool) return;
    doc.ui.calling = true;
    doc.ui.result = null;
    const r = await kind.callTool(doc, tool.name, args.value);
    doc.ui.calling = false;
    doc.ui.result = r.ok ? { ok: true, text: typeof r.data === 'string' ? r.data : JSON.stringify(r.data, null, 2) } : { ok: false, text: r.error };
  }
</script>

<div class="tv">
  <div class="head">
    <span class="kind">{kind.label}</span>
    <span class="mono">{doc.ref.project}/{doc.ref.name}</span>
    <span class="grow"></span>
    <button class="btn ghost sm" disabled={doc.ui.listing} onclick={list}>{doc.ui.listing ? 'Listing…' : '↻ List tools'}</button>
  </div>

  <div class="body">
    <aside>
      <input class="filter" placeholder="Filter {tools?.length ?? 0} tools" bind:value={filter} />
      {#if doc.ui.listing && !tools}<div class="note">Asking {doc.ref.name} for its tools…</div>{/if}
      {#if doc.ui.error}<div class="err">{doc.ui.error}</div>{/if}
      {#if tools && !tools.length}<div class="note">This server offers no tools.</div>{/if}
      <ul>
        {#each shown as t (t.name)}
          <li><button class:on={doc.ui.tool === t.name} onclick={() => pick(t)} title={t.description}>
            <span class="tn">{t.name}</span><span class="td">{t.description}</span>
          </button></li>
        {/each}
      </ul>
    </aside>

    <section>
      {#if !tool}
        <div class="note">Pick a tool to see what it takes and call it.</div>
      {:else}
        <h3>{tool.name}</h3>
        {#if tool.description}<p class="desc">{tool.description}</p>{/if}
        <div class="argbar">
          <span class="lbl">Arguments</span>
          <span class="grow"></span>
          <button class="lnk" onclick={() => (asJson = !asJson)}>{asJson ? 'Form' : 'JSON'}</button>
        </div>
        {#if asJson || args.error || !args.value || typeof args.value !== 'object' || Array.isArray(args.value)}
          {#if args.error}<div class="err">Not valid JSON: {args.error}</div>{/if}
          <textarea class="json" rows="8" spellcheck="false" bind:value={doc.ui.args}></textarea>
        {:else}
          <ConfigForm value={args.value} onchange={(v) => (doc.ui.args = toText(v))} />
        {/if}
        <div class="callbar">
          <button class="btn spec sm" disabled={doc.ui.calling || !!args.error} onclick={call}>{doc.ui.calling ? 'Calling…' : `Call ${tool.name}`}</button>
          <span class="warn">Runs the tool on {doc.ref.name} for real.</span>
        </div>
        {#if doc.ui.result}
          <pre class="result" class:bad={!doc.ui.result.ok}>{doc.ui.result.text}</pre>
        {/if}
      {/if}
    </section>
  </div>
</div>

<style>
  .tv { display: flex; flex-direction: column; height: 100%; min-height: 0; }
  .head { display: flex; align-items: center; gap: 10px; padding: 6px 12px; border-bottom: 1px solid var(--border); background: var(--bg-surface); }
  .kind { font-size: 11px; text-transform: uppercase; letter-spacing: 0.04em; color: var(--text-3); }
  .mono { font-family: var(--font-mono); font-size: 12.5px; }
  .grow { flex: 1; }
  .body { flex: 1; min-height: 0; display: grid; grid-template-columns: minmax(220px, 30%) 1fr; }
  aside { border-right: 1px solid var(--border); overflow-y: auto; display: flex; flex-direction: column; }
  .filter { margin: 8px; padding: 5px 8px; font-size: 12px; }
  ul { list-style: none; margin: 0; padding: 0; }
  li button { display: flex; flex-direction: column; gap: 2px; width: 100%; padding: 6px 12px; border: 0; background: transparent; cursor: pointer; text-align: left; }
  li button:hover { background: var(--bg-subtle); }
  li button.on { background: var(--spec-bg); }
  .tn { font-family: var(--font-mono); font-size: 12.5px; color: var(--text-1); }
  .td { font-size: 11.5px; color: var(--text-3); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  section { overflow-y: auto; padding: 14px 18px 30px; display: flex; flex-direction: column; gap: 10px; }
  h3 { font-family: var(--font-mono); font-size: 15px; font-weight: 600; margin: 0; }
  .desc { margin: 0; color: var(--text-2); font-size: 13px; line-height: 1.5; max-width: 760px; }
  .argbar { display: flex; align-items: center; }
  .lbl { font-size: 11px; text-transform: uppercase; letter-spacing: 0.04em; color: var(--text-3); }
  .lnk { border: 0; background: transparent; color: var(--spec); cursor: pointer; font-size: 12px; }
  .json { font-family: var(--font-mono); font-size: 12.5px; padding: 8px 10px; max-width: 760px; }
  .callbar { display: flex; align-items: center; gap: 10px; }
  .warn { font-size: 11.5px; color: var(--warn, #b45309); }
  .result { margin: 0; padding: 10px 12px; border-radius: var(--r-sm); border: 1px solid var(--border); background: var(--bg-page); font-family: var(--font-mono); font-size: 12px; white-space: pre-wrap; word-break: break-word; max-width: 900px; }
  .result.bad { background: var(--err-bg, #fde8e8); color: var(--err, #b42318); }
  .note { padding: 10px 12px; color: var(--text-4); font-size: 12.5px; }
  .err { margin: 8px; padding: 8px 10px; border-radius: var(--r-sm); background: var(--err-bg, #fde8e8); color: var(--err, #b42318); font-size: 12px; white-space: pre-wrap; }
</style>
