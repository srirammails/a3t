<script lang="ts">
  import { Handle, Position } from '@xyflow/svelte';
  let { data }: { data: any } = $props();

  function onDelete(e: MouseEvent) {
    e.stopPropagation();
    // A host that passes `onremove` handles it; otherwise the Triggers page's window event.
    if (typeof data.onremove === 'function') { data.onremove(data.targetIdx); return; }
    if (typeof window !== 'undefined') {
      window.dispatchEvent(new CustomEvent('trigger-disconnect', { detail: data.targetIdx }));
    }
  }
</script>

<div class="tgt-node">
  <Handle type="target" position={Position.Left} />
  <span class="tgt-label">{data.label}</span>
  <button class="tgt-rm" onclick={onDelete} title="Disconnect">
    <svg width="10" height="10" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="2"><path d="M12 4L4 12M4 4l8 8"/></svg>
  </button>
</div>

<style>
  .tgt-node {
    display: flex; align-items: center; gap: 6px;
    background: var(--spec-bg, #f0ebfa); border: 2px solid var(--spec, #5530a8);
    border-radius: 10px; padding: 8px 10px 8px 14px;
    font-size: 12px; font-family: var(--font-mono); color: var(--spec-ink, #5530a8);
    min-width: 140px;
  }
  .tgt-label { flex: 1; font-weight: 550; }
  .tgt-rm {
    width: 20px; height: 20px; border: 1px solid var(--border); border-radius: 4px;
    background: var(--bg-surface); color: var(--text-4); cursor: pointer;
    display: grid; place-items: center; flex-shrink: 0; transition: all 0.12s;
  }
  .tgt-rm:hover { background: var(--err-bg, #fde8e8); color: var(--err, #dc2626); border-color: var(--err); }
</style>
