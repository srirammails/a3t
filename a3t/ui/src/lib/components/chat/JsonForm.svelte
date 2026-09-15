<script lang="ts">
  /// A form view for the JSON-shaped artifacts — resources, triggers, agents.
  ///
  /// Flows get the full designer (graph + code + form) because there is a workflow
  /// model behind them. These have no such model, so rather than invent one this
  /// renders the document's own fields: scalars as inputs, string arrays as a
  /// list, nested objects as a labelled group. That covers a resource's
  /// `config.{url,transport,tools[]}` without needing a schema per resource type.

  let { value = '', onchange }: { value: string; onchange?: (val: string) => void } = $props();

  let parsed = $state<any>(null);
  let parseError = $state('');

  // Depends on `value` only. Deciding `parseError` from the freshly written
  // `parsed` read it back inside the same effect, and since JSON.parse returns a
  // NEW object every run that made the effect retrigger itself forever — Svelte
  // then aborts the component's reactivity, so opening a different item left the
  // form showing the previous one. Decide from a local instead.
  $effect(() => {
    try {
      const next = JSON.parse(value || '{}');
      const obj = next && typeof next === 'object' && !Array.isArray(next) ? next : null;
      parsed = obj;
      parseError = obj ? '' : 'Not a JSON object — use the Code tab.';
    } catch (e: any) {
      parsed = null;
      parseError = e?.message ?? 'Invalid JSON';
    }
  });

  /// Deep clone that is transparent to Svelte's `$state` proxies.
  /// `structuredClone` throws on them ("could not be cloned"), which is exactly
  /// how the flow form silently stopped writing back.
  function plainClone<T>(v: T): T {
    if (Array.isArray(v)) return v.map(plainClone) as unknown as T;
    if (v && typeof v === 'object') {
      const out: any = {};
      for (const k of Object.keys(v as any)) out[k] = plainClone((v as any)[k]);
      return out;
    }
    return v;
  }

  /// Fields the server owns; editing them here would be discarded on save at best
  /// and rewrite history at worst.
  const READONLY = new Set(['created_at', 'updated_at', 'created_by', 'id']);

  function emit() {
    try {
      onchange?.(JSON.stringify(plainClone(parsed), null, 2));
    } catch (e: any) {
      parseError = `Could not apply this change: ${e?.message ?? e}`;
    }
  }

  function setAt(path: (string | number)[], val: unknown) {
    let obj: any = parsed;
    for (let i = 0; i < path.length - 1; i++) obj = obj[path[i]];
    const key = path[path.length - 1];
    if (val === '' || val === undefined) delete obj[key as any];
    else obj[key as any] = val;
    parsed = { ...parsed };
    emit();
  }

  function setArrayItem(path: (string | number)[], idx: number, val: string) {
    let obj: any = parsed;
    for (const p of path) obj = obj[p];
    obj[idx] = val;
    parsed = { ...parsed };
    emit();
  }

  function addArrayItem(path: (string | number)[]) {
    let obj: any = parsed;
    for (const p of path) obj = obj[p];
    obj.push('');
    parsed = { ...parsed };
    emit();
  }

  function removeArrayItem(path: (string | number)[], idx: number) {
    let obj: any = parsed;
    for (const p of path) obj = obj[p];
    obj.splice(idx, 1);
    parsed = { ...parsed };
    emit();
  }

  type Row = { key: string; path: (string | number)[]; val: any; depth: number };

  /// Flatten to rows so the markup stays one loop — Svelte snippets would need to
  /// recurse, and the documents here are only a couple of levels deep.
  function rowsOf(obj: any, base: (string | number)[] = [], depth = 0): Row[] {
    const out: Row[] = [];
    for (const key of Object.keys(obj ?? {})) {
      const val = obj[key];
      const path = [...base, key];
      out.push({ key, path, val, depth });
      if (val && typeof val === 'object' && !Array.isArray(val)) {
        out.push(...rowsOf(val, path, depth + 1));
      }
    }
    return out;
  }

  const rows = $derived(parsed ? rowsOf(parsed) : []);
  const isScalar = (v: any) => v === null || ['string', 'number', 'boolean'].includes(typeof v);
  const isStringArray = (v: any) => Array.isArray(v) && v.every((x) => typeof x === 'string');
</script>

<div class="jf">
  {#if parseError}
    <div class="jf-err">{parseError}</div>
  {/if}

  {#each rows as row}
    {#if row.val && typeof row.val === 'object' && !Array.isArray(row.val)}
      <div class="jf-group" style="margin-left: {row.depth * 12}px">{row.key}</div>
    {:else}
      <div class="jf-row" style="margin-left: {row.depth * 12}px">
        <label class="jf-k" for={`jf-${row.path.join('-')}`}>{row.key}</label>

        {#if isStringArray(row.val)}
          <div class="jf-list">
            {#each row.val as item, i}
              <div class="jf-item">
                <input
                  type="text"
                  value={item}
                  oninput={(e) => setArrayItem(row.path, i, e.currentTarget.value)}
                />
                <button class="jf-x" onclick={() => removeArrayItem(row.path, i)} aria-label="Remove">×</button>
              </div>
            {/each}
            <button class="jf-add" onclick={() => addArrayItem(row.path)}>+ add</button>
          </div>
        {:else if typeof row.val === 'boolean'}
          <select
            id={`jf-${row.path.join('-')}`}
            value={String(row.val)}
            onchange={(e) => setAt(row.path, e.currentTarget.value === 'true')}
          >
            <option value="true">true</option>
            <option value="false">false</option>
          </select>
        {:else if typeof row.val === 'number'}
          <input
            id={`jf-${row.path.join('-')}`}
            type="number"
            value={row.val}
            oninput={(e) => setAt(row.path, e.currentTarget.value === '' ? undefined : Number(e.currentTarget.value))}
          />
        {:else if isScalar(row.val)}
          <input
            id={`jf-${row.path.join('-')}`}
            type="text"
            value={row.val ?? ''}
            readonly={READONLY.has(row.key)}
            placeholder={row.val === null ? 'null' : ''}
            oninput={(e) => setAt(row.path, e.currentTarget.value)}
          />
        {:else}
          <!-- An array of objects: no sensible generic widget, so say so rather
               than render something that silently drops structure. -->
          <span class="jf-opaque">{Array.isArray(row.val) ? `${row.val.length} items` : 'nested'} — edit in Code</span>
        {/if}
      </div>
    {/if}
  {/each}

  {#if !parseError && rows.length === 0}
    <div class="jf-err">Nothing to show.</div>
  {/if}
</div>

<style>
  .jf { padding: 12px 14px; overflow-y: auto; height: 100%; box-sizing: border-box; }
  .jf-err { padding: 8px 10px; font-size: 11.5px; color: #dc2626; }

  .jf-group {
    font-size: 10px; font-weight: 600; text-transform: uppercase;
    letter-spacing: 0.06em; color: var(--text-4);
    margin: 12px 0 4px;
  }
  .jf-row {
    display: grid; grid-template-columns: 120px 1fr; gap: 10px;
    align-items: start; padding: 4px 0;
  }
  .jf-k {
    font-size: 11px; color: var(--text-3); padding-top: 5px;
    overflow: hidden; text-overflow: ellipsis;
  }
  .jf-row input, .jf-row select {
    width: 100%; box-sizing: border-box;
    padding: 4px 7px; font-size: 11.5px;
    font-family: var(--font-mono);
    border: 1px solid var(--border); border-radius: var(--r-sm);
    background: var(--bg-surface); color: var(--text-1);
  }
  .jf-row input:read-only { background: var(--bg-subtle); color: var(--text-4); }

  .jf-list { display: flex; flex-direction: column; gap: 4px; }
  .jf-item { display: flex; align-items: center; gap: 4px; }
  .jf-x {
    width: 20px; height: 22px; flex: 0 0 auto;
    border: 1px solid var(--border); border-radius: var(--r-sm);
    background: var(--bg-surface); color: var(--text-4); cursor: pointer;
    line-height: 1;
  }
  .jf-x:hover { color: #dc2626; }
  .jf-add {
    align-self: flex-start;
    padding: 3px 8px; font-size: 10.5px;
    border: 1px dashed var(--border); border-radius: var(--r-sm);
    background: transparent; color: var(--text-4); cursor: pointer;
  }
  .jf-add:hover { color: var(--text-1); border-color: var(--border-strong); }

  .jf-opaque { font-size: 10.5px; color: var(--text-4); padding-top: 5px; }
</style>
