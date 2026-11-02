<script lang="ts">
  /// An agent's settings (Form panel), built from its template: fields the
  /// template requires (must be set), optional ones (the template's default
  /// until overridden), locked ones (read-only), hidden ones (not shown); then
  /// its name, mode, status and where it is served.
  import SaveBar from '../parts/SaveBar.svelte';
  import { agentModel, fromText, toText, type Doc, type Visibility, type Workspace } from '$lib/studio-core';

  let { doc, workspace }: { doc: Doc | null; workspace: Workspace } = $props();

  const value = $derived((fromText(doc?.text).value ?? {}) as any);
  const settings = $derived((value.settings ?? {}) as Record<string, unknown>);
  const spec = $derived((doc?.data?.template?.spec_yaml ?? '') as string);
  const fields = $derived.by(() => {
    const list = agentModel.templateFields(spec).filter((f) => f.visibility !== 'hidden');
    for (const k of Object.keys(settings)) if (!list.some((f) => f.key === k) && agentModel.visibilities(spec)[k] !== 'hidden') list.push({ key: k, visibility: 'optional', value: undefined });
    return list;
  });
  const need = $derived(agentModel.missing(spec, settings));

  function edit(path: string[], v: unknown) {
    if (!doc) return;
    const next = structuredClone($state.snapshot(value));
    let o: any = next;
    for (const p of path.slice(0, -1)) o = o[p] ??= {};
    if (v === undefined) delete o[path[path.length - 1]];
    else o[path[path.length - 1]] = v;
    doc.text = toText(next);
  }
  const show = (v: unknown) => (v === undefined ? '' : typeof v === 'string' ? v : JSON.stringify(v));
  /// Strings and numbers edit as themselves; lists and objects as JSON.
  function parseLike(like: unknown, text: string): unknown {
    if (typeof like === 'number') return text === '' ? undefined : Number(text);
    if (like && typeof like === 'object') { try { return JSON.parse(text); } catch { return text; } }
    return text;
  }
  const ICON: Record<Visibility, string> = { required: '●', optional: '○', locked: '🔒', hidden: '' };
</script>

{#if !doc}
  <div class="note">Open an agent.</div>
{:else if !doc.loaded}
  <div class="note">Loading…</div>
{:else if doc.error}
  <div class="err">{doc.error}</div>
{:else}
  <div class="af">
    <div class="head"><span class="title">Agent</span><span class="grow"></span><SaveBar {doc} {workspace} valid={!need.length} /></div>
    <div class="body">
      {#key doc.address}
        <section>
          <h4>Identity</h4>
          <label><span>Name</span><input value={value.name ?? ''} oninput={(e) => edit(['name'], e.currentTarget.value)} /></label>
          <div class="row2">
            <label><span>Mode</span><input value={value.mode ?? ''} list="agent-modes" oninput={(e) => edit(['mode'], e.currentTarget.value)} /></label>
            <label><span>Status</span><input value={value.status ?? ''} list="agent-statuses" oninput={(e) => edit(['status'], e.currentTarget.value)} /></label>
          </div>
          <datalist id="agent-modes"><option value="session"></option><option value="stateless"></option></datalist>
          <datalist id="agent-statuses"><option value="draft"></option><option value="active"></option><option value="paused"></option></datalist>
        </section>

        <section>
          <h4>Settings <span class="from">from template {doc.data?.template?.name ?? '?'}</span></h4>
          {#if need.length}<div class="err">Required: {need.join(', ')}</div>{/if}
          {#each fields as f (f.key)}
            {@const own = f.key in settings}
            {@const shown = own ? settings[f.key] : f.value}
            <div class="field {f.visibility}" class:own>
              <div class="top">
                <span class="vis" title={f.visibility}>{ICON[f.visibility]}</span>
                <span class="k">{f.key}</span>
                {#if f.visibility === 'optional'}
                  {#if own}<button class="lnk" onclick={() => edit(['settings', f.key], undefined)} title="Use the template's value">Use template</button>
                  {:else}<button class="lnk" onclick={() => edit(['settings', f.key], f.value ?? '')}>Override</button>{/if}
                {/if}
              </div>
              {#if f.visibility === 'locked' || (f.visibility === 'optional' && !own)}
                <div class="ro" title={show(shown)}>{show(shown) || '—'}</div>
              {:else if typeof shown === 'boolean'}
                <input type="checkbox" checked={shown} onchange={(e) => edit(['settings', f.key], e.currentTarget.checked)} />
              {:else if shown && typeof shown === 'object'}
                <textarea rows="3" spellcheck="false" oninput={(e) => edit(['settings', f.key], parseLike(shown, e.currentTarget.value))}>{JSON.stringify(shown, null, 1)}</textarea>
              {:else}
                <input type={typeof (f.value ?? shown) === 'number' ? 'number' : 'text'} value={own ? show(shown) : ''}
                  placeholder={f.visibility === 'required' ? `placeholder: ${show(f.value)}` : show(f.value)}
                  oninput={(e) => edit(['settings', f.key], parseLike(f.value ?? shown, e.currentTarget.value))} />
              {/if}
            </div>
          {/each}
        </section>

        <section>
          <h4>Serving</h4>
          <label><span>Chat flow (target)</span><input value={value.serving?.target ?? ''} oninput={(e) => edit(['serving', 'target'], e.currentTarget.value)} placeholder="a3t/a3t-chat-engine" /></label>
          <label><span>Endpoint path</span><input value={value.serving?.ep_path ?? ''} oninput={(e) => edit(['serving', 'ep_path'], e.currentTarget.value)} placeholder="/agent/<id>/chat" /></label>
        </section>
      {/key}
    </div>
  </div>
{/if}

<style>
  .af { display: flex; flex-direction: column; height: 100%; min-height: 0; }
  .head { display: flex; align-items: center; gap: 8px; padding: 6px 12px; border-bottom: 1px solid var(--border); }
  .title { font-size: 11px; text-transform: uppercase; letter-spacing: 0.04em; color: var(--text-3); }
  .grow { flex: 1; }
  .body { flex: 1; min-height: 0; overflow-y: auto; padding: 10px 12px 30px; display: flex; flex-direction: column; gap: 14px; }
  section { display: flex; flex-direction: column; gap: 6px; }
  h4 { margin: 0; font-size: 11px; text-transform: uppercase; letter-spacing: 0.05em; color: var(--text-2); }
  .from { text-transform: none; letter-spacing: 0; font-weight: 400; color: var(--text-4); }
  label { display: flex; flex-direction: column; gap: 3px; font-size: 11px; color: var(--text-2); text-transform: none; letter-spacing: 0; font-weight: 500; }
  .row2 { display: grid; grid-template-columns: 1fr 1fr; gap: 8px; }
  input:not([type='checkbox']), textarea { padding: 6px 9px; font-size: 12.5px; font-family: var(--font-mono); }
  textarea { resize: vertical; }
  .field { border: 1px solid var(--border); border-radius: var(--r-sm); padding: 6px 8px; display: flex; flex-direction: column; gap: 4px; }
  .field.required { border-color: var(--warn, #b45309); }
  .field.locked { background: var(--bg-page); }
  .top { display: flex; align-items: center; gap: 6px; }
  .vis { width: 14px; text-align: center; font-size: 10px; color: var(--text-4); }
  .required .vis { color: var(--warn, #b45309); }
  .k { flex: 1; font-family: var(--font-mono); font-size: 12px; font-weight: 600; }
  .ro { font-family: var(--font-mono); font-size: 11.5px; color: var(--text-3); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .lnk { border: 0; background: transparent; color: var(--spec); cursor: pointer; font-size: 11.5px; padding: 0; }
  .note { padding: 14px; color: var(--text-4); font-size: 12.5px; }
  .err { padding: 6px 10px; border-radius: var(--r-sm); background: var(--err-bg, #fde8e8); color: var(--err, #b42318); font-size: 12px; }
</style>
