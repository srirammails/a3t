<script lang="ts">
  let {
    open = false,
    entityType = 'item',
    entityName = '',
    onconfirm,
    oncancel,
  }: {
    open: boolean;
    entityType: string;
    entityName: string;
    onconfirm: () => void;
    oncancel: () => void;
  } = $props();

  let confirmName = $state('');
  let deleting = $state(false);

  $effect(() => { if (open) confirmName = ''; });

  async function handleConfirm() {
    if (confirmName !== entityName) return;
    deleting = true;
    onconfirm();
    deleting = false;
  }
</script>

{#if open}
  <div class="del-overlay" onclick={oncancel}></div>
  <div class="del-dialog">
    <div class="del-head">
      <svg width="20" height="20" viewBox="0 0 16 16" fill="none" stroke="var(--err, #d44)" stroke-width="1.5"><path d="M8 1L2 4v4c0 4 3 6.5 6 7.5 3-1 6-3.5 6-7.5V4z"/><path d="M8 5v4M8 11h.01" stroke-width="2" stroke-linecap="round"/></svg>
      <h3>Delete {entityType}</h3>
    </div>
    <p class="del-msg">This will permanently delete <strong>{entityName}</strong> and cannot be undone.</p>
    <p class="del-msg">Type the name to confirm:</p>
    <input class="del-input" type="text" bind:value={confirmName} placeholder={entityName} />
    <div class="del-actions">
      <button class="btn ghost" onclick={oncancel}>Cancel</button>
      <button class="btn danger" disabled={deleting || confirmName !== entityName} onclick={handleConfirm}>
        {deleting ? 'Deleting...' : `Delete ${entityType}`}
      </button>
    </div>
  </div>
{/if}

<style>
  .del-overlay { position: fixed; inset: 0; background: rgba(0,0,0,0.3); z-index: 200; animation: fadeIn 0.15s ease-out; }
  @keyframes fadeIn { from { opacity: 0; } to { opacity: 1; } }
  .del-dialog {
    position: fixed; top: 50%; left: 50%; transform: translate(-50%, -50%);
    background: var(--bg-surface); border: 1px solid var(--border);
    border-radius: 12px; padding: 24px 28px; z-index: 201;
    width: 400px; max-width: 90vw;
    box-shadow: 0 12px 40px rgba(0,0,0,0.15), 0 2px 8px rgba(0,0,0,0.08);
    animation: dialogIn 0.2s cubic-bezier(0.16, 1, 0.3, 1);
  }
  @keyframes dialogIn { from { transform: translate(-50%, -48%) scale(0.96); opacity: 0; } to { transform: translate(-50%, -50%) scale(1); opacity: 1; } }
  .del-head { display: flex; align-items: center; gap: 10px; margin-bottom: 14px; }
  .del-head h3 { font-family: var(--font-serif); font-size: 16px; font-weight: 500; }
  .del-msg { font-size: 13px; color: var(--text-2); line-height: 1.5; margin-bottom: 10px; }
  .del-msg strong { color: var(--text-1); }
  .del-input {
    width: 100%; padding: 9px 12px; border: 1px solid var(--border); border-radius: var(--r-sm);
    font-size: 13px; font-family: var(--font-mono); background: var(--bg-page); color: var(--text-1);
    outline: none; margin-bottom: 16px;
  }
  .del-input:focus { border-color: var(--err, #d44); box-shadow: 0 0 0 2px rgba(212,68,68,0.1); }
  .del-actions { display: flex; gap: 8px; justify-content: flex-end; }
  :global(.btn.danger) {
    background: var(--err, #d44); color: white; border: none; padding: 8px 16px;
    border-radius: var(--r-sm); font-size: 13px; font-weight: 500; cursor: pointer;
  }
  :global(.btn.danger:hover) { opacity: 0.9; }
  :global(.btn.danger:disabled) { opacity: 0.4; cursor: default; }
</style>
