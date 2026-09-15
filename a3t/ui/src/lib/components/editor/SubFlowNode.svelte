<script lang="ts">
  import { Handle, Position } from '@xyflow/svelte';

  let { data }: { data: any } = $props();
</script>

<div class="sf-node">
  <Handle type="target" position={Position.Top} id="in" style="background: var(--checkpoint, #c47028);" />
  <div class="sf-row">
    <span class="sf-ico">F</span>
    <span class="sf-name">{data.label}</span>
    <span class="sf-count">{data.stepCount ?? '?'} steps</span>
  </div>
  {#if data.namespace}
    <div class="sf-ns">{data.namespace}</div>
  {/if}
</div>

<style>
  .sf-node {
    background: var(--bg-surface, white);
    border: 1px solid var(--checkpoint, #c47028);
    border-radius: 8px;
    padding: 6px 10px;
    min-width: 140px;
    box-shadow: 0 1px 3px rgba(0,0,0,0.05);
  }
  .sf-row { display: flex; align-items: center; gap: 5px; }
  .sf-ico {
    width: 16px; height: 16px; border-radius: 3px; display: grid; place-items: center;
    background: var(--checkpoint, #c47028); color: white;
    font-size: 7px; font-weight: 700; font-family: var(--font-mono, monospace);
  }
  .sf-name { font-size: 11px; font-weight: 550; }
  .sf-count { font-size: 9px; color: var(--text-4); font-family: var(--font-mono, monospace); margin-left: auto; }
  .sf-ns { font-size: 9px; color: var(--text-4); font-family: var(--font-mono, monospace); padding-left: 21px; }
</style>
