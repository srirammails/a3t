<script lang="ts">
  import { Handle, Position } from '@xyflow/svelte';

  let { data }: { data: any } = $props();
</script>

<div class="res-node" class:vis-hidden={data.vis === 'hidden'} class:vis-locked={data.vis === 'locked'} style="border-color: {data.color ?? 'var(--border)'};">
  <Handle type="target" position={Position.Left} id="in" style="background: {data.color ?? 'var(--text-3)'};" />
  <div class="rn-icon" style="background: {data.color ?? 'var(--text-3)'};">{data.icon ?? '?'}</div>
  <div class="rn-info">
    <div class="rn-label">{data.label}</div>
    <div class="rn-type">{data.subtype ?? ''}</div>
  </div>
  {#if data.vis}<span class="rn-vis">{data.vis}</span>{/if}
</div>

<style>
  .res-node {
    display: flex; align-items: center; gap: 8px;
    padding: 8px 12px; background: var(--bg-surface, white);
    border: 1px solid; border-radius: 8px;
    min-width: 140px; box-shadow: 0 1px 4px rgba(0,0,0,0.06);
  }
  .rn-icon {
    width: 24px; height: 24px; border-radius: 5px; display: grid; place-items: center;
    color: white; font-size: 10px; font-weight: 700; font-family: var(--font-mono, monospace); flex-shrink: 0;
  }
  .rn-label { font-size: 11px; font-weight: 550; }
  .rn-type { font-size: 9px; color: var(--text-4, #9c9789); font-family: var(--font-mono, monospace); }
  .rn-vis { font-size: 8px; font-family: var(--font-mono, monospace); padding: 1px 4px; border-radius: 2px; background: rgba(0,0,0,0.05); color: var(--text-4, #9c9789); margin-left: auto; }

  /* Visibility states */
  .res-node.vis-hidden { border-style: dashed; opacity: 0.4; }
  .res-node.vis-locked { opacity: 0.65; }
</style>
