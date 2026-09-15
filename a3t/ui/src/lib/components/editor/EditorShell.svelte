<script lang="ts">
  /**
   * EditorShell — unified wrapper for all editor surfaces.
   * Ensures EditorContext (registry + ajv) is initialized before rendering.
   * Shows loading/error states.
   *
   * Usage:
   *   <EditorShell>
   *     <FlowEditorPane ... />
   *   </EditorShell>
   */
  import type { Snippet } from 'svelte';
  import { onMount } from 'svelte';
  import { initEditorContext, getEditorContext } from '$lib/editor/context';

  let { children }: { children: Snippet } = $props();

  let status = $state<'loading' | 'ready' | 'error'>('loading');
  let errorMsg = $state('');
  let fnCount = $state(0);

  onMount(async () => {
    // Check if already initialized
    const existing = getEditorContext();
    if (existing?.ready) {
      fnCount = existing.functions.size;
      status = 'ready';
      return;
    }

    try {
      const ctx = await initEditorContext();
      fnCount = ctx.functions.size;
      if (ctx.error) {
        console.warn('[EditorShell] Context init warning:', ctx.error);
        // Still render — partial init is OK (validation may not work)
      }
      status = 'ready';
    } catch (e) {
      errorMsg = String(e);
      status = 'error';
    }
  });
</script>

{#if status === 'loading'}
  <div class="es-loading">
    <div class="es-spinner"></div>
    <span>Loading editor...</span>
  </div>
{:else if status === 'error'}
  <div class="es-error">
    <span class="es-err-icon">!</span>
    <span>Editor init failed: {errorMsg}</span>
    <button class="es-retry" onclick={() => { status = 'loading'; initEditorContext().then(() => status = 'ready').catch(e => { errorMsg = String(e); status = 'error'; }); }}>Retry</button>
  </div>
{:else}
  {#if fnCount > 0}
    <div class="es-status-bar">
      <span class="es-badge">{fnCount} functions</span>
    </div>
  {/if}
  {@render children()}
{/if}

<style>
  .es-loading {
    display: flex; align-items: center; justify-content: center; gap: 10px;
    height: 100%; color: var(--text-4); font-size: 13px;
  }
  .es-spinner {
    width: 16px; height: 16px; border: 2px solid var(--border);
    border-top-color: var(--spec, #5530a8); border-radius: 50%;
    animation: spin 0.8s linear infinite;
  }
  @keyframes spin { to { transform: rotate(360deg); } }

  .es-error {
    display: flex; align-items: center; justify-content: center; gap: 8px;
    height: 100%; color: var(--err, #dc2626); font-size: 12px; padding: 20px;
  }
  .es-err-icon {
    width: 24px; height: 24px; border-radius: 50%; background: var(--err-bg, #fde8e8);
    display: grid; place-items: center; font-weight: 700; font-size: 14px; flex-shrink: 0;
  }
  .es-retry {
    padding: 4px 10px; border: 1px solid var(--border); border-radius: 4px;
    background: var(--bg-surface); color: var(--text-2); font-size: 11px; cursor: pointer;
  }
  .es-retry:hover { background: var(--bg-subtle); }

  .es-status-bar {
    display: flex; align-items: center; gap: 6px;
    padding: 2px 10px; background: var(--bg-subtle); border-bottom: 1px solid var(--border);
    font-size: 9px; flex-shrink: 0;
  }
  .es-badge {
    font-family: var(--font-mono); color: var(--text-4);
    background: var(--bg-surface); padding: 0 5px; border-radius: 2px;
    border: 1px solid var(--border);
  }
</style>
