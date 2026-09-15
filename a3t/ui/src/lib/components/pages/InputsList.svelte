<script lang="ts">
  import { mockInputs } from '$lib/mock/data';
  import DocsPanel from '$lib/shell/DocsPanel.svelte';
  let docsOpen = $state(false);
</script>

<div class="screen-header">
  <div class="breadcrumb">Operate <span class="sep">/</span> Pending Inputs</div>
  <div class="screen-title-row">
    <div class="title-block">
      <h1 class="screen-title">Pending Inputs</h1>
      <button class="help-btn" onclick={() => docsOpen = true} title="Learn more about Inputs"><svg width="14" height="14" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5"><circle cx="8" cy="8" r="7"/><path d="M6 6a2 2 0 114 0c0 1-1.5 1.5-2 2M8 11.5v.5"/></svg></button>
    </div>
  </div>
</div>

<DocsPanel open={docsOpen} pageKey="inputs" onClose={() => docsOpen = false} />

<div class="screen-body">
  {#each mockInputs as input}
    <div class="input-card">
      <div class="input-top">
        <div>
          <h3>{input.title}</h3>
          <p class="input-msg">{input.message}</p>
        </div>
        <span class="badge warn"><span class="dot"></span>{input.status}</span>
      </div>

      {#if input.options}
        <div class="input-actions">
          {#each input.options as opt}
            <button class="btn" class:primary={opt.style==='primary'} class:danger={opt.style==='danger'}>{opt.label}</button>
          {/each}
        </div>
      {/if}

      {#if input.fields}
        <div class="input-form">
          {#each input.fields as field}
            <div class="ff">
              <label for="field-{field.name}">{field.label} {#if field.required}<span style="color: var(--err);">*</span>{/if}</label>
              {#if field.type === 'select'}
                <select id="field-{field.name}">
                  {#each field.options || [] as opt}<option>{opt}</option>{/each}
                </select>
              {:else}
                <input id="field-{field.name}" type={field.type === 'number' ? 'number' : 'text'} placeholder={field.label} />
              {/if}
            </div>
          {/each}
          <button class="btn primary">Submit</button>
        </div>
      {/if}

      <div class="input-footer mono dim">
        Created: {new Date(input.created_at).toLocaleString()} &middot; Expires: {new Date(input.expires_at).toLocaleString()}
        {#if input.agent_id} &middot; Agent: {input.agent_id}{/if}
      </div>
    </div>
  {/each}
</div>

<style>
  .screen-header { padding: 22px 40px 14px; border-bottom: 1px solid var(--border); background: var(--bg-page); }
  .breadcrumb { font-size: 12px; color: var(--text-3); margin-bottom: 10px; font-family: var(--font-mono); }
  .breadcrumb .sep { color: var(--text-5); }
  .screen-title-row { display: flex; align-items: center; gap: 18px; }
  .title-block { flex: 1; display: flex; align-items: center; gap: 14px; }
  .screen-title { font-family: var(--font-serif); font-size: 28px; font-weight: 500; letter-spacing: -0.02em; }
  .help-btn { width: 24px; height: 24px; border: 1px solid var(--border); border-radius: 50%; background: var(--bg-surface); color: var(--text-4); display: grid; place-items: center; cursor: pointer; transition: all 0.15s; flex-shrink: 0; }
  .help-btn:hover { background: var(--runtime-bg); border-color: var(--runtime); color: var(--runtime); }

  .screen-body { flex: 1; overflow: auto; padding: 24px 40px 40px; }

  .input-card { background: var(--bg-surface); border: 1px solid var(--border); border-radius: var(--r); padding: 20px; margin-bottom: 14px; box-shadow: var(--shadow-sm); }
  .input-top { display: flex; justify-content: space-between; align-items: flex-start; margin-bottom: 12px; }
  .input-top h3 { font-family: var(--font-serif); font-size: 16px; font-weight: 500; }
  .input-msg { font-size: 13px; color: var(--text-2); margin-top: 4px; line-height: 1.4; }
  .input-actions { display: flex; gap: 8px; margin-top: 12px; }

  .input-form { margin-top: 16px; }
  .ff { margin-bottom: 12px; }
  .ff label { font-size: 12px; font-weight: 500; display: block; margin-bottom: 4px; color: var(--text-2); }
  .ff input, .ff select { width: 100%; padding: 8px 12px; border: 1px solid var(--border); border-radius: var(--r-sm); font-size: 13px; font-family: var(--font-sans); background: var(--bg-surface); }
  .ff input:focus, .ff select:focus { outline: none; border-color: var(--runtime); box-shadow: 0 0 0 2px var(--runtime-bg); }

  .input-footer { font-size: 11px; margin-top: 16px; padding-top: 12px; border-top: 1px solid var(--divider); }
</style>
