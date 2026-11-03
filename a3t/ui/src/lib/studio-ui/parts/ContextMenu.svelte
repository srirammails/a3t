<script lang="ts" module>
  export type MenuItem = { label: string; icon?: string; kbd?: string; danger?: boolean; disabled?: boolean; run: () => void } | 'sep';
</script>

<script lang="ts">
  /// The Studio's context menu: a list of actions at the pointer. The Studio
  /// shows it instead of the browser's, wherever the user right-clicks.
  let { x, y, items, onclose }: { x: number; y: number; items: MenuItem[]; onclose: () => void } = $props();

  // Separators only between actions.
  const shown = $derived(items.filter((it, i, all) => it !== 'sep' || (i > 0 && i < all.length - 1 && all[i - 1] !== 'sep')));
  const left = $derived(Math.min(x, window.innerWidth - 250));
  const top = $derived(Math.min(y, window.innerHeight - shown.length * 30 - 16));
  function pick(it: Exclude<MenuItem, 'sep'>) { onclose(); it.run(); }
</script>

<svelte:window onkeydown={(e) => e.key === 'Escape' && onclose()} />

<div class="cm-overlay" role="presentation" onclick={onclose} oncontextmenu={(e) => { e.preventDefault(); onclose(); }}></div>
<div class="cm" role="menu" style="left: {left}px; top: {Math.max(4, top)}px">
  {#each shown as it, i (i)}
    {#if it === 'sep'}<hr />{:else}
      <button role="menuitem" class:danger={it.danger} disabled={it.disabled} onclick={() => pick(it)}>
        <span class="ic">{it.icon ?? ''}</span>{it.label}{#if it.kbd}<kbd>{it.kbd}</kbd>{/if}
      </button>
    {/if}
  {/each}
</div>

<style>
  .cm-overlay { position: fixed; inset: 0; z-index: 190; }
  .cm { position: fixed; z-index: 191; min-width: 210px; max-width: 300px; display: flex; flex-direction: column; padding: 4px;
        background: var(--bg-surface); border: 1px solid var(--border); border-radius: var(--r-sm); box-shadow: var(--shadow-lg); font-size: 12.5px; }
  button { display: flex; align-items: center; gap: 8px; border: 0; background: transparent; padding: 5px 8px; border-radius: 4px; cursor: pointer; text-align: left; color: var(--text-1); font-size: 12.5px; }
  button:hover:not(:disabled) { background: var(--bg-subtle); }
  button:disabled { opacity: 0.45; cursor: default; }
  button.danger { color: var(--err, #b42318); }
  .ic { width: 16px; text-align: center; color: var(--text-3); font-size: 11px; }
  kbd { margin-left: auto; font-family: var(--font-mono); font-size: 10.5px; color: var(--text-4); }
  hr { border: 0; border-top: 1px solid var(--border); margin: 3px 0; }
</style>
