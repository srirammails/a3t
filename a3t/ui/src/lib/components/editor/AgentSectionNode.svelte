<script lang="ts">
  import { Handle, Position } from '@xyflow/svelte';

  let { data }: { data: any } = $props();
</script>

<div class="asn" style="--asn-color: {data.color};">
  <Handle type="target" position={Position.Left} id="in" style="background: {data.color};" />

  <div class="asn-header">
    <span class="asn-icon" style="background: {data.color};">{data.icon}</span>
    <span class="asn-label">{data.label}</span>
    <span class="asn-detail">{data.detail}</span>
    {#if data.expanded}
      <svg class="asn-chevron" width="10" height="10" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="2"><path d="M6 4l4 4-4 4"/></svg>
    {:else}
      <svg class="asn-chevron" width="10" height="10" viewBox="0 0 16 16" fill="none" stroke="var(--text-4)" stroke-width="1.5"><path d="M6 4l4 4-4 4"/></svg>
    {/if}
  </div>

  <Handle type="source" position={Position.Right} id="out" style="background: {data.color};" />
</div>

<style>
  .asn {
    background: var(--bg-surface, white);
    border: 1.5px solid var(--asn-color);
    border-left: 3px solid var(--asn-color);
    border-radius: 8px;
    min-width: 180px;
    max-width: 200px;
    box-shadow: 0 1px 4px rgba(0,0,0,0.06);
    cursor: pointer;
    transition: border-color 0.15s, box-shadow 0.15s;
  }
  .asn:hover {
    box-shadow: 0 3px 12px rgba(0,0,0,0.1);
  }

  .asn-header {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 10px 10px;
  }

  .asn-icon {
    width: 20px; height: 20px; border-radius: 4px;
    display: grid; place-items: center;
    color: white; font-size: 9px; font-weight: 700;
    font-family: var(--font-mono, monospace); flex-shrink: 0;
  }

  .asn-label {
    font-size: 11px; font-weight: 600;
    color: var(--text-1, #1a1a1a);
  }

  .asn-detail {
    font-family: var(--font-mono, monospace);
    font-size: 9px; color: var(--text-4, #9c9789);
    margin-left: auto;
  }

  .asn-chevron { flex-shrink: 0; margin-left: 4px; }
</style>
