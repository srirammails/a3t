<script lang="ts">
  /// Agent fields (right region, templates): for each field of the template's
  /// agent config, what agents may do with it — required (the template's value
  /// is a placeholder for testing it), optional (a default agents may override),
  /// locked (final, shown read-only), hidden (final, not shown). Changing one
  /// edits the template (`document.metadata.a3t.fields`), saved with it.
  import { agentModel, type Doc, type Visibility } from '$lib/studio-core';

  let { doc }: { doc: Doc | null } = $props();
  const fields = $derived(doc?.text ? agentModel.templateFields(doc.text) : []);
  const absent = $derived(doc?.text ? agentModel.ENGINE_FIELDS.filter((k) => !fields.some((f) => f.key === k)) : []);
  const HELP: Record<Visibility, string> = {
    required: 'every agent must set it; the value here is a placeholder for testing',
    optional: 'a default; agents may override it',
    locked: 'final; agents see it read-only',
    hidden: 'final; agents do not see it',
  };
  const show = (v: unknown) => (v === undefined ? '—' : typeof v === 'string' ? v : JSON.stringify(v));
  function setVisibility(key: string, v: Visibility) {
    if (doc?.text) doc.text = agentModel.withVisibility(doc.text, key, v);
  }
</script>

{#if !doc || !doc.loaded}
  <div class="note">Open an agent template.</div>
{:else}
  <div class="tf">
    <p class="intro">What agents built from this template may do with each field of its agent config.</p>
    {#if !fields.length}<p class="note">This template's agent step has no literal config to expose.</p>{/if}
    {#each fields as f (f.key)}
      <div class="field">
        <div class="top">
          <span class="k">{f.key}</span>
          <select value={f.visibility} onchange={(e) => setVisibility(f.key, e.currentTarget.value as Visibility)} title={HELP[f.visibility]}>
            {#each agentModel.VISIBILITIES as v (v)}<option value={v}>{v}</option>{/each}
          </select>
        </div>
        <div class="v" title={show(f.value)}>{f.visibility === 'required' ? 'placeholder: ' : ''}{show(f.value)}</div>
        <div class="help">{HELP[f.visibility]}</div>
      </div>
    {/each}
    {#if absent.length}
      <p class="intro">Engine settings its config does not list{fields.length ? '' : ' (its config is worked out at run time)'} — agents may set them unless you say otherwise:</p>
      {#each absent as k (k)}
        <div class="field dim">
          <div class="top">
            <span class="k">{k}</span>
            <select value="optional" onchange={(e) => setVisibility(k, e.currentTarget.value as Visibility)}>
              {#each agentModel.VISIBILITIES as v (v)}<option value={v}>{v}</option>{/each}
            </select>
          </div>
        </div>
      {/each}
    {/if}
  </div>
{/if}

<style>
  .tf { padding: 10px 12px 30px; display: flex; flex-direction: column; gap: 8px; overflow-y: auto; height: 100%; }
  .intro { margin: 4px 0; font-size: 12px; color: var(--text-3); }
  .field { border: 1px solid var(--border); border-radius: var(--r-sm); padding: 6px 8px; display: flex; flex-direction: column; gap: 3px; }
  .field.dim { opacity: 0.75; }
  .top { display: flex; align-items: center; gap: 8px; }
  .k { flex: 1; font-family: var(--font-mono); font-size: 12px; font-weight: 600; }
  select { width: auto; padding: 2px 22px 2px 6px; font-size: 11.5px; }
  .v { font-family: var(--font-mono); font-size: 11px; color: var(--text-2); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .help { font-size: 10.5px; color: var(--text-4); }
  .note { padding: 14px; color: var(--text-4); font-size: 12.5px; }
</style>
