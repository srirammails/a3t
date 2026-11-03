<script lang="ts">
  /// A form built from a config's own keys and values — no per-type field lists.
  /// Strings, numbers and booleans are fields; objects are sections; arrays are
  /// lists. Fields can be added and removed. Values the server showed masked are
  /// read-only until replaced (saving them unchanged keeps what is stored).
  import { getAt, pathKey, removeAt, setAt, type Path } from '$lib/studio-core';

  let { value, onchange, readonly = false, masked = new Set<string>(), stack = false }: {
    value: Record<string, unknown>;
    onchange?: (next: Record<string, unknown>) => void;
    readonly?: boolean;
    masked?: Set<string>;
    /// Labels above fields, for narrow places (the Form panel).
    stack?: boolean;
  } = $props();

  /// Masked fields the user chose to replace (they become password fields).
  let replacing = $state<Record<string, boolean>>({});
  /// The "+ field" row open on an object, by path.
  let adding = $state<{ at: string; key: string; type: 'text' | 'number' | 'boolean' | 'object' } | null>(null);

  const set = (p: Path, v: unknown) => onchange?.(setAt(value, p, v));
  const remove = (p: Path) => onchange?.(removeAt(value, p));
  const isObj = (v: unknown): v is Record<string, unknown> => !!v && typeof v === 'object' && !Array.isArray(v);
  const blank = (like: unknown): unknown =>
    typeof like === 'number' ? 0 : typeof like === 'boolean' ? false : isObj(like) ? Object.fromEntries(Object.keys(like).map((k) => [k, blank(like[k])])) : Array.isArray(like) ? [] : '';
  const long = (s: string) => s.includes('\n') || s.length > 80;

  function addField(p: Path) {
    if (!adding?.key.trim()) return;
    const k = adding.key.trim();
    const v = adding.type === 'number' ? 0 : adding.type === 'boolean' ? false : adding.type === 'object' ? {} : '';
    set([...p, k], v);
    adding = null;
  }
</script>

<div class="cf" class:stack>
  {@render fields(value, [])}
</div>

{#snippet fields(obj: Record<string, unknown>, p: Path)}
  {#each Object.entries(obj) as [k, v] (k)}
    {@render field(k, v, [...p, k])}
  {/each}
  {#if !readonly}
    {#if adding?.at === pathKey(p)}
      <div class="add-row">
        <!-- svelte-ignore a11y_autofocus -->
        <input class="key" placeholder="field name" bind:value={adding.key} autofocus
          onkeydown={(e) => { if (e.key === 'Enter') addField(p); if (e.key === 'Escape') adding = null; }} />
        <select bind:value={adding.type}>
          <option value="text">text</option><option value="number">number</option><option value="boolean">yes / no</option><option value="object">section</option>
        </select>
        <button class="lnk" onclick={() => addField(p)}>Add</button>
        <button class="lnk mute" onclick={() => (adding = null)}>Cancel</button>
      </div>
    {:else}
      <button class="lnk add" onclick={() => (adding = { at: pathKey(p), key: '', type: 'text' })}>+ field</button>
    {/if}
  {/if}
{/snippet}

{#snippet field(label: string | number, v: unknown, p: Path)}
  {@const key = pathKey(p)}
  {#if isObj(v)}
    <fieldset>
      <legend>{label}{#if !readonly}<button class="x" title="Remove {label}" onclick={() => remove(p)}>×</button>{/if}</legend>
      {@render fields(v, p)}
    </fieldset>
  {:else if Array.isArray(v)}
    <fieldset class="list">
      <legend>{label} <span class="n">{v.length}</span>{#if !readonly}<button class="x" title="Remove {label}" onclick={() => remove(p)}>×</button>{/if}</legend>
      {#each v as item, i (i)}
        {#if isObj(item) || Array.isArray(item)}
          {@render field(`${i + 1}`, item, [...p, i])}
        {:else}
          <div class="item">
            {@render input(item, [...p, i])}
            {#if !readonly}<button class="x" title="Remove" onclick={() => remove([...p, i])}>×</button>{/if}
          </div>
        {/if}
      {/each}
      {#if !v.length}<div class="none">empty</div>{/if}
      {#if !readonly}<button class="lnk add" onclick={() => set([...p, v.length], blank(v[0] ?? ''))}>+ item</button>{/if}
    </fieldset>
  {:else}
    <label class="row" class:long={typeof v === 'string' && long(v)}>
      <span class="k">{label}{#if masked.has(key) && !replacing[key]}<em title="Shown masked by the server; saving keeps the stored value">masked</em>{/if}</span>
      <span class="v">
        {@render input(v, p)}
        {#if !readonly}<button class="x" title="Remove {label}" onclick={() => remove(p)}>×</button>{/if}
      </span>
    </label>
  {/if}
{/snippet}

{#snippet input(v: unknown, p: Path)}
  {@const key = pathKey(p)}
  {#if masked.has(key) && !replacing[key]}
    <input type="text" value={String(v ?? '')} readonly class="masked" />
    {#if !readonly}<button class="lnk" onclick={() => { replacing[key] = true; set(p, ''); }}>Replace</button>{/if}
  {:else if masked.has(key)}
    <input type="password" value={String(getAt(value, p) ?? '')} placeholder="new value" autocomplete="new-password"
      oninput={(e) => set(p, e.currentTarget.value)} />
  {:else if typeof v === 'boolean'}
    <input type="checkbox" checked={v} disabled={readonly} onchange={(e) => set(p, e.currentTarget.checked)} />
  {:else if typeof v === 'number'}
    <input type="number" value={v} readonly={readonly} oninput={(e) => set(p, e.currentTarget.value === '' ? 0 : Number(e.currentTarget.value))} />
  {:else if typeof v === 'string' && long(v)}
    <textarea rows={Math.min(12, v.split('\n').length + 1)} readonly={readonly} oninput={(e) => set(p, e.currentTarget.value)}>{v}</textarea>
  {:else}
    <input type="text" value={v === null ? '' : String(v ?? '')} placeholder={v === null ? 'null' : ''} readonly={readonly}
      oninput={(e) => set(p, e.currentTarget.value)} />
  {/if}
{/snippet}

<style>
  .cf { display: flex; flex-direction: column; gap: 6px; max-width: 760px; }
  .row { display: grid; grid-template-columns: minmax(120px, 200px) 1fr; align-items: center; gap: 10px; text-transform: none; letter-spacing: 0; font-weight: 400; }
  .row.long { align-items: start; }
  .stack .row { grid-template-columns: 1fr; gap: 3px; }
  .stack .add-row { flex-wrap: wrap; }
  .stack .add-row .key { width: 100%; }
  .k { font-family: var(--font-mono); font-size: 12px; color: var(--text-2); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .k em { font-style: normal; margin-left: 6px; font-size: 10px; padding: 1px 5px; border-radius: 3px; background: var(--warn-bg); color: var(--warn); }
  .v, .item { display: flex; align-items: center; gap: 6px; min-width: 0; }
  .v > input:not([type='checkbox']), .v > textarea, .item > input:not([type='checkbox']), .item > textarea { flex: 1; min-width: 0; }
  input:not([type='checkbox']), textarea, select {
    padding: 7px 10px; font-size: 13px; font-family: var(--font-mono); line-height: 1.5;
    border: 1px solid var(--border); border-radius: var(--r-sm); background: var(--bg-surface); color: var(--text-1);
  }
  input:focus, textarea:focus, select:focus { border-color: var(--spec); box-shadow: none; outline: none; }
  input[readonly], textarea[readonly] { background: var(--bg-page); color: var(--text-2); }
  input.masked { color: var(--text-3); }
  textarea { resize: vertical; }
  fieldset { border: 1px solid var(--border); border-radius: var(--r-sm); padding: 8px 12px 10px; margin: 4px 0; display: flex; flex-direction: column; gap: 6px; min-width: 0; }
  legend { font-family: var(--font-mono); font-size: 12px; font-weight: 600; color: var(--text-1); padding: 0 4px; display: flex; align-items: center; gap: 6px; }
  .n { font-weight: 400; color: var(--text-4); font-size: 11px; }
  .x { border: 0; background: transparent; color: var(--text-4); cursor: pointer; font-size: 14px; padding: 0 4px; line-height: 1; }
  .x:hover { color: var(--err, #b42318); }
  .lnk { border: 0; background: transparent; color: var(--spec); cursor: pointer; font-size: 12px; padding: 2px 4px; white-space: nowrap; }
  .lnk.mute { color: var(--text-3); }
  .lnk.add { align-self: flex-start; }
  .add-row { display: flex; gap: 6px; align-items: center; }
  .add-row .key { width: 200px; }
  .none { color: var(--text-4); font-size: 12px; }
</style>
