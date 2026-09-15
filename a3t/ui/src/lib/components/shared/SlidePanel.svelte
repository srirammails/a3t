<script lang="ts">
  import type { Snippet } from 'svelte';

  let {
    open = false,
    onclose,
    onsave,
    onopenineditor,
    children,
  }: {
    open: boolean;
    onclose: () => void;
    onsave?: () => void;
    onopenineditor?: () => void;
    children: Snippet;
  } = $props();

  let closing = $state(false);

  function handleClose() {
    closing = true;
    setTimeout(() => {
      closing = false;
      onclose();
    }, 200);
  }
</script>

{#if open}
  <div class="sp-overlay" class:closing onclick={handleClose}></div>
  <div class="sp-panel" class:closing>
    <div class="sp-close-bar">
      {#if onopenineditor}
        <div class="sp-btn-wrap">
          <button class="sp-action-btn" onclick={onopenineditor}>
            <svg width="14" height="14" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5"><path d="M9 1h6v6M9 7L15 1M7 3H2a1 1 0 00-1 1v10a1 1 0 001 1h10a1 1 0 001-1V9"/></svg>
          </button>
          <span class="sp-tooltip">Open in Editor</span>
        </div>
      {/if}
      {#if onsave}
        <div class="sp-btn-wrap">
          <button class="sp-action-btn save" onclick={onsave}>
            <svg width="14" height="14" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5"><path d="M13 15H3a1 1 0 01-1-1V2a1 1 0 011-1h8l3 3v10a1 1 0 01-1 1z"/><path d="M11 15V9H5v6M5 1v4h5"/></svg>
          </button>
          <span class="sp-tooltip">Save</span>
        </div>
      {/if}
      <div class="sp-btn-wrap">
        <button class="sp-close-btn" onclick={handleClose}>
          <svg width="14" height="14" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="2"><path d="M12 4L4 12M4 4l8 8"/></svg>
        </button>
        <span class="sp-tooltip">Close</span>
      </div>
    </div>
    <div class="sp-body">
      {@render children()}
    </div>
  </div>
{/if}

<style>
  .sp-overlay {
    position: fixed; inset: 0; background: rgba(0,0,0,0.12);
    z-index: 100; animation: overlayIn 0.2s ease-out;
  }
  .sp-overlay.closing { animation: overlayOut 0.2s ease-in forwards; }
  @keyframes overlayIn { from { opacity: 0; } to { opacity: 1; } }
  @keyframes overlayOut { from { opacity: 1; } to { opacity: 0; } }

  .sp-panel {
    position: fixed; top: 42px; left: 110px; right: 6px; bottom: 6px;
    background: var(--bg-page); border: 1px solid var(--border); border-radius: 14px;
    z-index: 101; overflow: hidden;
    box-shadow: 0 8px 40px rgba(0,0,0,0.14), 0 2px 10px rgba(0,0,0,0.06), -4px 0 20px rgba(0,0,0,0.04);
    animation: panelIn 0.25s cubic-bezier(0.16, 1, 0.3, 1);
    display: flex; flex-direction: column;
  }
  .sp-panel.closing { animation: panelOut 0.2s ease-in forwards; }
  @keyframes panelIn { from { transform: translateX(100%); } to { transform: translateX(0); } }
  @keyframes panelOut { from { transform: translateX(0); } to { transform: translateX(100%); } }

  .sp-close-bar {
    display: flex; align-items: center; justify-content: flex-end; gap: 6px;
    padding: 6px 12px; height: 44px; flex-shrink: 0;
    background: var(--bg-page); border-bottom: 1px solid var(--border);
  }

  .sp-btn-wrap { position: relative; }
  .sp-tooltip {
    position: absolute; bottom: -30px; left: 50%; transform: translateX(-50%);
    padding: 4px 10px; background: var(--text-1); color: var(--bg-page);
    font-size: 11px; font-weight: 500; white-space: nowrap;
    border-radius: 5px; pointer-events: none;
    opacity: 0; transition: opacity 0.15s 0.3s; z-index: 20;
  }
  .sp-tooltip::before {
    content: ''; position: absolute; bottom: 100%; left: 50%; transform: translateX(-50%);
    border: 4px solid transparent; border-bottom-color: var(--text-1);
  }
  .sp-btn-wrap:hover .sp-tooltip { opacity: 1; }

  .sp-action-btn, .sp-close-btn {
    width: 32px; height: 32px; border: 1px solid var(--border); border-radius: 50%;
    background: var(--bg-surface); color: var(--text-3);
    cursor: pointer; display: grid; place-items: center; transition: all 0.12s;
  }
  .sp-action-btn:hover { background: var(--bg-subtle); color: var(--text-1); border-color: var(--border-strong); }
  .sp-action-btn.save:hover { background: var(--ok-bg, #e3f4ef); color: var(--ok, #23987f); border-color: var(--ok); }
  .sp-close-btn:hover { background: var(--text-1); color: var(--bg-page); border-color: var(--text-1); }

  .sp-body { flex: 1; overflow: hidden; }
</style>
