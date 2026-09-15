<script lang="ts">
  import { Handle, Position } from '@xyflow/svelte';

  let { data }: { data: any } = $props();
</script>

<div class="ps-node" class:disabled={!data.enabled} class:locked={data.locked} class:skip-branch={data.on_hit}>
  <Handle type="target" position={Position.Top} id="in" style="background: var(--text-3);" />

  <div class="ps-row">
    <span class="ps-ico" style="background: {data.color ?? 'var(--text-3)'};">{data.icon ?? '?'}</span>
    <span class="ps-name">{data.label}</span>
    {#if data.locked}<span class="ps-badge lock">locked</span>{/if}
    {#if !data.enabled}<span class="ps-badge off">off</span>{/if}
  </div>

  {#if data.subtitle}
    <div class="ps-sub">{data.subtitle}</div>
  {/if}

  <Handle type="source" position={Position.Bottom} id="out" style="background: var(--text-3);" />

  {#if data.on_hit}
    <Handle type="source" position={Position.Right} id="hit" style="background: var(--ok, #23987f);" />
  {/if}
</div>

<style>
  .ps-node {
    background: var(--bg-surface, white);
    border: 1px solid var(--border, #eae6dc);
    border-radius: 8px;
    padding: 6px 10px;
    min-width: 180px;
    box-shadow: 0 1px 3px rgba(0,0,0,0.05);
    font-family: var(--font-sans, system-ui);
  }
  .ps-node.disabled { opacity: 0.35; border-style: dashed; }
  .ps-node.locked { opacity: 0.8; }
  .ps-node.skip-branch { border-right: 3px solid var(--ok, #23987f); }

  .ps-row { display: flex; align-items: center; gap: 5px; }
  .ps-ico {
    width: 16px; height: 16px; border-radius: 3px; display: grid; place-items: center;
    font-size: 7px; font-weight: 700; color: white; font-family: var(--font-mono, monospace); flex-shrink: 0;
  }
  .ps-name { font-size: 11px; font-weight: 550; }
  .ps-badge { font-size: 8px; font-family: var(--font-mono, monospace); padding: 1px 4px; border-radius: 2px; margin-left: auto; }
  .ps-badge.lock { background: var(--bg-subtle, #f4f2ed); color: var(--text-4); }
  .ps-badge.off { background: var(--err-bg, #fde8e8); color: var(--err); }

  .ps-sub { font-size: 9px; color: var(--text-4, #9c9789); font-family: var(--font-mono, monospace); margin-top: 2px; padding-left: 21px; }
</style>
