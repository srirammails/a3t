<script lang="ts">
  import { API_BASE } from '$lib/config';
  /**
   * SchemaConfigForm — recursive JSON Schema form renderer.
   * No hardcoded field names. Renders any valid JSON Schema (2020-12).
   *
   * Props:
   *   schema: JSONSchema — the JSON Schema to render
   *   value: Record<string, unknown> — current values
   *   path: string[] — breadcrumb path (for nested recursion)
   *   onchange: (path: string[], value: unknown) => void
   *   readonly: boolean
   */

  import * as YAML from 'yaml';
  import { onMount } from 'svelte';

  let {
    schema = {},
    value = {},
    path = [] as string[],
    onchange,
    readonly = false,
    depth = 0,
    errors = [] as { path: string; message: string }[],
  }: {
    schema: Record<string, unknown>;
    value: Record<string, unknown>;
    path?: string[];
    onchange?: (path: string[], value: unknown) => void;
    readonly?: boolean;
    depth?: number;
    errors?: { path: string; message: string }[];
  } = $props();

  // Extract schema parts
  let properties = $derived((schema.properties ?? {}) as Record<string, Record<string, unknown>>);
  let required = $derived(new Set((schema.required ?? []) as string[]));
  let sortedKeys = $derived(Object.keys(properties));

  function getType(prop: Record<string, unknown>): string {
    return (prop.type as string) ?? 'string';
  }

  function getEnum(prop: Record<string, unknown>): string[] | null {
    return (prop.enum as string[]) ?? null;
  }

  function getDefault(prop: Record<string, unknown>): unknown {
    return prop.default;
  }

  function getDescription(prop: Record<string, unknown>): string {
    return (prop.description as string) ?? '';
  }

  function getMin(prop: Record<string, unknown>): number | undefined {
    return prop.minimum as number | undefined;
  }

  function getMax(prop: Record<string, unknown>): number | undefined {
    return prop.maximum as number | undefined;
  }

  function isObjectType(prop: Record<string, unknown>): boolean {
    return getType(prop) === 'object' && !!prop.properties;
  }

  function isArrayType(prop: Record<string, unknown>): boolean {
    return getType(prop) === 'array';
  }

  function getItems(prop: Record<string, unknown>): Record<string, unknown> | null {
    return (prop.items as Record<string, unknown>) ?? null;
  }

  function getValue(key: string): unknown {
    return value?.[key];
  }

  function emit(key: string, val: unknown) {
    onchange?.([...path, key], val);
  }

  function emitNested(nestedPath: string[], val: unknown) {
    onchange?.(nestedPath, val);
  }

  // For array of strings — add/remove
  function addArrayItem(key: string) {
    const arr = (getValue(key) as unknown[] ?? []).slice();
    arr.push('');
    emit(key, arr);
  }

  function removeArrayItem(key: string, idx: number) {
    const arr = (getValue(key) as unknown[] ?? []).slice();
    arr.splice(idx, 1);
    emit(key, arr);
  }

  function updateArrayItem(key: string, idx: number, val: string) {
    const arr = (getValue(key) as unknown[] ?? []).slice();
    arr[idx] = val;
    emit(key, arr);
  }

  // For object with additionalProperties — key-value pairs
  function isAdditionalPropsObject(prop: Record<string, unknown>): boolean {
    return getType(prop) === 'object' && !prop.properties && !!prop.additionalProperties;
  }

  function addKVPair(key: string) {
    const obj = { ...(getValue(key) as Record<string, unknown> ?? {}) };
    const newKey = `key_${Object.keys(obj).length + 1}`;
    obj[newKey] = {};
    emit(key, obj);
  }

  function removeKVPair(key: string, subKey: string) {
    const obj = { ...(getValue(key) as Record<string, unknown> ?? {}) };
    delete obj[subKey];
    emit(key, obj);
  }

  // Section open state for nested objects
  let openSections = $state<Record<string, boolean>>({});

  function toggleSection(key: string) {
    openSections[key] = !openSections[key];
  }

  function formatLabel(key: string): string {
    return key.replace(/_/g, ' ');
  }

  // ── Resource picker support ──
  // Detect fields that reference resources by name pattern or description
  function isResourceField(key: string, prop: Record<string, unknown>): boolean {
    if (key.endsWith('_resource') || key === '_resource') return true;
    const desc = getDescription(prop).toLowerCase();
    return desc.includes('resource name') || desc.includes('resource ref');
  }

  // Detect array fields that hold resource names (e.g. mcp_servers)
  function isResourceArrayField(key: string, prop: Record<string, unknown>): boolean {
    if (getType(prop) !== 'array') return false;
    const desc = getDescription(prop).toLowerCase();
    return key === 'mcp_servers' || desc.includes('resource name') || desc.includes('server resource');
  }

  // Fetch available resources once per form instance
  let availableResources = $state<{ name: string; type: string; namespace: string }[]>([]);
  let resourcesLoaded = $state(false);

  onMount(async () => {
    // Only fetch if this form has resource-like fields
    const hasResourceFields = sortedKeys.some(k => {
      const prop = properties[k];
      return isResourceField(k, prop) || isResourceArrayField(k, prop);
    });
    if (!hasResourceFields) return;

    try {
      const base = API_BASE;
      const res = await fetch(`${base}/registry/resources`);
      if (res.ok) {
        const data = await res.json();
        availableResources = (data ?? []).map((r: any) => ({
          name: r.name, type: r.type, namespace: r.namespace ?? 'default',
        }));
      }
    } catch {}
    resourcesLoaded = true;
  });

  function getFieldErrors(key: string): string[] {
    if (!errors.length) return [];
    const fieldPath = [...path, key].join('/');
    return errors
      .filter(e => e.path === `/${fieldPath}` || e.path.startsWith(`/${fieldPath}/`))
      .map(e => e.message);
  }

  function getChildErrors(key: string): { path: string; message: string }[] {
    if (!errors.length) return [];
    const prefix = `/${[...path, key].join('/')}`;
    return errors.filter(e => e.path.startsWith(prefix));
  }
</script>

<div class="scf" class:nested={depth > 0}>
  {#each sortedKeys as key}
    {@const prop = properties[key]}
    {@const type = getType(prop)}
    {@const enumVals = getEnum(prop)}
    {@const desc = getDescription(prop)}
    {@const isReq = required.has(key)}
    {@const val = getValue(key)}
    {@const dflt = getDefault(prop)}
    {@const fieldErrs = getFieldErrors(key)}

    {#if isObjectType(prop)}
      <!-- Nested object — collapsible section -->
      <div class="scf-section" class:open={openSections[key]}>
        <button class="scf-section-head" onclick={() => toggleSection(key)}>
          <svg class="scf-chev" class:open={openSections[key]} width="10" height="10" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="2"><path d="M6 4l4 4-4 4"/></svg>
          <span class="scf-key">{formatLabel(key)}</span>
          {#if isReq}<span class="scf-req">*</span>{/if}
          {#if desc}<span class="scf-desc-inline">{desc}</span>{/if}
        </button>
        {#if openSections[key]}
          <div class="scf-section-body">
            <svelte:self
              schema={prop}
              value={(val ?? {}) as Record<string, unknown>}
              path={[...path, key]}
              {onchange}
              {readonly}
              depth={depth + 1}
              errors={getChildErrors(key)}
            />
          </div>
        {/if}
      </div>

    {:else if isAdditionalPropsObject(prop)}
      <!-- Object with additionalProperties — key-value map -->
      <div class="scf-section" class:open={openSections[key]}>
        <button class="scf-section-head" onclick={() => toggleSection(key)}>
          <svg class="scf-chev" class:open={openSections[key]} width="10" height="10" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="2"><path d="M6 4l4 4-4 4"/></svg>
          <span class="scf-key">{formatLabel(key)}</span>
          <span class="scf-cnt">{Object.keys((val ?? {}) as Record<string, unknown>).length}</span>
          {#if desc}<span class="scf-desc-inline">{desc}</span>{/if}
        </button>
        {#if openSections[key]}
          <div class="scf-section-body">
            {#each Object.entries((val ?? {}) as Record<string, unknown>) as [subKey, subVal]}
              {@const apSchema = prop.additionalProperties as Record<string, unknown>}
              {#if apSchema?.properties}
                <!-- Each entry is an object with known shape -->
                <div class="scf-kv-entry">
                  <div class="scf-kv-head">
                    <input class="scf-kv-key" type="text" value={subKey} readonly={readonly}
                      oninput={(e) => {
                        const newKey = e.currentTarget.value;
                        if (newKey && newKey !== subKey) {
                          const obj = { ...(val as Record<string, unknown>) };
                          obj[newKey] = obj[subKey];
                          delete obj[subKey];
                          emit(key, obj);
                        }
                      }} />
                    {#if !readonly}
                      <button class="scf-rm" onclick={() => removeKVPair(key, subKey)}>&times;</button>
                    {/if}
                  </div>
                  <svelte:self
                    schema={apSchema}
                    value={(subVal ?? {}) as Record<string, unknown>}
                    path={[...path, key, subKey]}
                    {onchange}
                    {readonly}
                    depth={depth + 1}
                  />
                </div>
              {:else}
                <!-- Simple value entries -->
                <div class="scf-kv-row">
                  <input class="scf-kv-key" type="text" value={subKey} readonly={readonly}
                    oninput={(e) => {
                      const newKey = e.currentTarget.value;
                      if (newKey && newKey !== subKey) {
                        const obj = { ...(val as Record<string, unknown>) };
                        obj[newKey] = obj[subKey];
                        delete obj[subKey];
                        emit(key, obj);
                      }
                    }} />
                  <input class="scf-kv-val" type="text" value={String(subVal ?? '')} readonly={readonly}
                    oninput={(e) => {
                      const obj = { ...(val as Record<string, unknown>) };
                      obj[subKey] = e.currentTarget.value;
                      emit(key, obj);
                    }} />
                  {#if !readonly}
                    <button class="scf-rm" onclick={() => removeKVPair(key, subKey)}>&times;</button>
                  {/if}
                </div>
              {/if}
            {/each}
            {#if !readonly}
              <button class="scf-add" onclick={() => addKVPair(key)}>+ Add {formatLabel(key).replace(/s$/, '')}</button>
            {/if}
          </div>
        {/if}
      </div>

    {:else if isArrayType(prop)}
      <!-- Array -->
      {@const items = getItems(prop)}
      {@const arrVal = (val ?? []) as unknown[]}
      <div class="scf-field">
        <div class="scf-label-row">
          <span class="scf-key">{formatLabel(key)}</span>
          {#if isReq}<span class="scf-req">*</span>{/if}
          <span class="scf-cnt">{arrVal.length}</span>
          {#if desc}<span class="scf-desc-inline">{desc}</span>{/if}
        </div>
        {#if items?.type === 'string' && isResourceArrayField(key, prop)}
          <!-- Resource array — select + text combo per item -->
          {#each arrVal as item, i}
            <div class="scf-arr-row">
              <select value={String(item ?? '')}
                onchange={(e) => updateArrayItem(key, i, e.currentTarget.value)}>
                <option value="">-- pick --</option>
                {#each availableResources as r}
                  <option value={r.name}>{r.name} ({r.type})</option>
                {/each}
              </select>
              <input type="text" value={String(item ?? '')} placeholder="or type name..."
                readonly={readonly}
                oninput={(e) => updateArrayItem(key, i, e.currentTarget.value)} />
              {#if !readonly}
                <button class="scf-rm" onclick={() => removeArrayItem(key, i)}>&times;</button>
              {/if}
            </div>
          {/each}
        {:else if items?.type === 'string'}
          <!-- Simple string array -->
          {#each arrVal as item, i}
            <div class="scf-arr-row">
              <input type="text" value={String(item ?? '')} readonly={readonly}
                oninput={(e) => updateArrayItem(key, i, e.currentTarget.value)} />
              {#if !readonly}
                <button class="scf-rm" onclick={() => removeArrayItem(key, i)}>&times;</button>
              {/if}
            </div>
          {/each}
        {:else if items?.properties}
          <!-- Array of objects -->
          {#each arrVal as item, i}
            <div class="scf-arr-obj">
              <div class="scf-arr-obj-head">
                <span class="scf-arr-idx">{i + 1}</span>
                {#if !readonly}
                  <button class="scf-rm" onclick={() => removeArrayItem(key, i)}>&times;</button>
                {/if}
              </div>
              <svelte:self
                schema={items}
                value={(item ?? {}) as Record<string, unknown>}
                path={[...path, key, String(i)]}
                {onchange}
                {readonly}
                depth={depth + 1}
              />
            </div>
          {/each}
        {:else}
          <!-- Unknown array items — show as YAML textarea -->
          <textarea class="scf-raw" rows="3" readonly={readonly}
            oninput={(e) => {
              try {
                const parsed = YAML.parse(e.currentTarget.value);
                if (Array.isArray(parsed)) emit(key, parsed);
              } catch {}
            }}>{arrVal.length ? YAML.stringify(arrVal, { indent: 2 }).trim() : ''}</textarea>
        {/if}
        {#if !readonly}
          <button class="scf-add" onclick={() => addArrayItem(key)}>+ Add</button>
        {/if}
      </div>

    {:else if isResourceField(key, prop) && type === 'string'}
      <!-- Resource picker (string) -->
      <label class="scf-field">
        <div class="scf-label-row">
          <span class="scf-key">{formatLabel(key)}</span>
          {#if isReq}<span class="scf-req">*</span>{/if}
          <span class="scf-badge res">resource</span>
        </div>
        <div class="scf-res-combo">
          <select value={String(val ?? '')} disabled={readonly}
            onchange={(e) => emit(key, e.currentTarget.value)}>
            <option value="">-- select resource --</option>
            {#each availableResources as r}
              <option value={r.name}>{r.name} ({r.type})</option>
            {/each}
          </select>
          <input type="text" value={String(val ?? '')} placeholder="or type name..."
            readonly={readonly}
            oninput={(e) => emit(key, e.currentTarget.value || undefined)} />
        </div>
        {#if desc}<span class="scf-desc">{desc}</span>{/if}
        {#each fieldErrs as err}<span class="scf-err">{err}</span>{/each}
      </label>

    {:else if enumVals}
      <!-- Enum select -->
      <label class="scf-field">
        <div class="scf-label-row">
          <span class="scf-key">{formatLabel(key)}</span>
          {#if isReq}<span class="scf-req">*</span>{/if}
        </div>
        <select value={String(val ?? dflt ?? '')} disabled={readonly}
          onchange={(e) => emit(key, e.currentTarget.value)}>
          <option value="">--</option>
          {#each enumVals as opt}<option value={opt}>{opt}</option>{/each}
        </select>
        {#if desc}<span class="scf-desc">{desc}</span>{/if}
        {#each fieldErrs as err}<span class="scf-err">{err}</span>{/each}
      </label>

    {:else if type === 'boolean'}
      <label class="scf-field scf-bool">
        <div class="scf-label-row">
          <input type="checkbox" checked={val != null ? Boolean(val) : Boolean(dflt)}
            disabled={readonly}
            onchange={(e) => emit(key, e.currentTarget.checked)} />
          <span class="scf-key">{formatLabel(key)}</span>
          {#if isReq}<span class="scf-req">*</span>{/if}
        </div>
        {#if desc}<span class="scf-desc">{desc}</span>{/if}
        {#each fieldErrs as err}<span class="scf-err">{err}</span>{/each}
      </label>

    {:else if type === 'integer' || type === 'number'}
      <label class="scf-field">
        <div class="scf-label-row">
          <span class="scf-key">{formatLabel(key)}</span>
          {#if isReq}<span class="scf-req">*</span>{/if}
          {#if dflt != null}<span class="scf-default">= {dflt}</span>{/if}
        </div>
        <input type="number"
          value={val != null ? Number(val) : (dflt != null ? Number(dflt) : '')}
          min={getMin(prop)} max={getMax(prop)}
          step={type === 'number' ? 0.1 : 1}
          readonly={readonly}
          oninput={(e) => emit(key, e.currentTarget.value ? Number(e.currentTarget.value) : undefined)} />
        {#if desc}<span class="scf-desc">{desc}</span>{/if}
        {#each fieldErrs as err}<span class="scf-err">{err}</span>{/each}
      </label>

    {:else}
      <!-- String (default) -->
      {@const isLong = key.includes('prompt') || key.includes('description') || key.includes('code') || key.includes('query')}
      <label class="scf-field" class:full={isLong}>
        <div class="scf-label-row">
          <span class="scf-key">{formatLabel(key)}</span>
          {#if isReq}<span class="scf-req">*</span>{/if}
          {#if dflt != null}<span class="scf-default">= {String(dflt)}</span>{/if}
        </div>
        {#if isLong}
          <textarea rows="4" readonly={readonly}
            oninput={(e) => emit(key, e.currentTarget.value || undefined)}
            placeholder={desc || ''}>{String(val ?? dflt ?? '')}</textarea>
        {:else}
          <input type="text"
            value={String(val ?? dflt ?? '')}
            placeholder={desc || ''}
            readonly={readonly}
            oninput={(e) => emit(key, e.currentTarget.value || undefined)} />
        {/if}
        {#each fieldErrs as err}<span class="scf-err">{err}</span>{/each}
      </label>
    {/if}
  {/each}
</div>

<style>
  .scf { display: flex; flex-direction: column; gap: 6px; }
  .scf.nested { padding-left: 0; }

  /* Field layout */
  .scf-field { display: flex; flex-direction: column; gap: 2px; }
  .scf-field.full { grid-column: 1 / -1; }
  .scf-field.scf-bool .scf-label-row { gap: 6px; }

  .scf-label-row {
    display: flex; align-items: center; gap: 4px;
  }
  .scf-key {
    font-size: 12px; font-weight: 550; color: var(--text-2);
    text-transform: capitalize; font-family: var(--font-mono);
  }
  .scf-req { color: var(--err, #dc2626); font-size: 12px; font-weight: 700; }
  .scf-default { font-size: 10px; color: var(--text-4); font-family: var(--font-mono); }
  .scf-desc { font-size: 10px; color: var(--text-4); line-height: 1.3; }
  .scf-desc-inline { font-size: 10px; color: var(--text-4); margin-left: auto; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; max-width: 200px; }
  .scf-cnt { font-family: var(--font-mono); font-size: 10px; color: var(--text-4); background: var(--bg-subtle); padding: 0 4px; border-radius: 2px; }

  /* Inputs */
  .scf input[type="text"], .scf input[type="number"], .scf select, .scf textarea {
    padding: 7px 10px; border: 1px solid var(--border); border-radius: var(--r-sm, 4px);
    background: var(--bg-surface); color: var(--text-1); font-size: 13px;
    font-family: var(--font-mono); outline: none; min-width: 0; width: 100%;
  }
  .scf input:focus, .scf select:focus, .scf textarea:focus { border-color: var(--spec, #5530a8); }
  .scf textarea { resize: vertical; }
  .scf input[type="checkbox"] { accent-color: var(--spec, #5530a8); }

  /* Section (nested object) */
  .scf-section { border: 1px solid var(--border); border-radius: 4px; overflow: hidden; }
  .scf-section-head {
    display: flex; align-items: center; gap: 5px; width: 100%;
    padding: 5px 8px; border: none; background: var(--bg-surface); cursor: pointer;
    font-size: 11px; text-align: left;
  }
  .scf-section-head:hover { background: var(--bg-subtle); }
  .scf-chev { color: var(--text-4); transition: transform 0.15s; flex-shrink: 0; }
  .scf-chev.open { transform: rotate(90deg); }
  .scf-section-body { padding: 6px 8px 8px; border-top: 1px solid var(--border); background: var(--bg-page); }

  /* Array rows */
  .scf-arr-row { display: flex; gap: 4px; align-items: center; margin-bottom: 2px; }
  .scf-arr-row input { flex: 1; }
  .scf-arr-obj { border: 1px solid var(--border); border-radius: 3px; padding: 4px 6px; margin-bottom: 3px; background: var(--bg-surface); }
  .scf-arr-obj-head { display: flex; align-items: center; gap: 4px; margin-bottom: 4px; }
  .scf-arr-idx { font-family: var(--font-mono); font-size: 9px; color: var(--text-4); background: var(--bg-subtle); width: 16px; height: 16px; border-radius: 2px; display: grid; place-items: center; }

  /* KV rows */
  .scf-kv-row { display: flex; gap: 4px; align-items: center; margin-bottom: 2px; }
  .scf-kv-key { width: 120px; flex-shrink: 0; padding: 4px 7px; border: 1px solid var(--border); border-radius: 3px; background: var(--bg-subtle); color: var(--text-2); font-family: var(--font-mono); font-size: 11px; outline: none; }
  .scf-kv-val { flex: 1; padding: 4px 7px; border: 1px solid var(--border); border-radius: 3px; background: var(--bg-surface); color: var(--text-1); font-family: var(--font-mono); font-size: 11px; outline: none; }
  .scf-kv-entry { border: 1px solid var(--border); border-radius: 3px; padding: 4px 6px; margin-bottom: 3px; background: var(--bg-surface); }
  .scf-kv-head { display: flex; gap: 4px; align-items: center; margin-bottom: 4px; }

  /* Buttons */
  .scf-rm { width: 18px; height: 18px; border: none; border-radius: 2px; background: none; color: var(--text-5); cursor: pointer; display: grid; place-items: center; font-size: 13px; flex-shrink: 0; }
  .scf-rm:hover { color: var(--err, #dc2626); background: var(--err-bg, #fde8e8); }
  .scf-add { border: 1px dashed var(--border); border-radius: 3px; padding: 3px 8px; font-size: 10px; color: var(--text-4); background: none; cursor: pointer; margin-top: 2px; align-self: flex-start; }
  .scf-add:hover { border-color: var(--spec); color: var(--spec); }

  .scf-raw {
    width: 100%; padding: 6px 8px; border: 1px solid var(--border); border-radius: 3px;
    background: var(--bg-surface); color: var(--text-1); font-family: var(--font-mono); font-size: 11px;
    outline: none; resize: vertical;
  }

  /* Resource combo */
  .scf-res-combo { display: flex; gap: 4px; }
  .scf-res-combo select { flex: 1; }
  .scf-res-combo input { flex: 1; }
  .scf-badge { font-size: 8px; padding: 0 4px; border-radius: 2px; font-family: var(--font-mono); text-transform: uppercase; letter-spacing: 0.03em; }
  .scf-badge.res { color: var(--runtime, #1a7158); background: var(--runtime-bg, #e6f4ef); }

  /* Validation errors */
  .scf-err {
    font-size: 9px; color: var(--err, #dc2626); line-height: 1.3;
    padding: 1px 4px; background: var(--err-bg, #fde8e8); border-radius: 2px;
    display: inline-block; margin-top: 1px;
  }
</style>
