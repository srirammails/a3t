<script lang="ts">
  import { Handle, Position } from '@xyflow/svelte';

  let { data, id }: { data: Record<string, any>; id: string } = $props();

  const TYPE_STYLES: Record<string, { icon: string; color: string; bg: string; label: string }> = {
    switch: { icon: '◇',  color: '#7c4bbb', bg: '#f3eefb', label: 'SWITCH' },
    fork:   { icon: '⫘',  color: '#1a7158', bg: '#edf7f3', label: 'FORK' },
    for:    { icon: '↻',  color: '#c47028', bg: '#fef6ee', label: 'FOR' },
    try:    { icon: '⛨',  color: '#2563eb', bg: '#eff4ff', label: 'TRY' },
    do:     { icon: '▸▸', color: '#64748b', bg: '#f1f5f9', label: 'DO' },
  };

  const style = TYPE_STYLES[data.taskType] ?? TYPE_STYLES.do;
</script>

<div class="ctrl-node" style="--ctrl-color: {style.color}; --ctrl-bg: {style.bg};">
  <Handle type="target" position={Position.Top} id="in" style="background: {style.color};" />

  <div class="ctrl-header">
    <span class="ctrl-icon">{style.icon}</span>
    <span class="ctrl-label">{data.label || id}</span>
    <span class="ctrl-badge">{style.label}</span>
  </div>

  <div class="ctrl-details">
    {#if data.taskType === 'switch'}
      <span class="ctrl-meta">{data.branchCount ?? 0} branches</span>
    {:else if data.taskType === 'fork'}
      <span class="ctrl-meta">{data.branchCount ?? 0} parallel</span>
    {:else if data.taskType === 'for'}
      <span class="ctrl-meta">each <code>{data.each ?? 'item'}</code> in <code>{data.inExpr ? (data.inExpr.length > 20 ? data.inExpr.slice(0, 18) + '…' : data.inExpr) : '…'}</code></span>
    {:else if data.taskType === 'try'}
      {#if data.hasCatch}<span class="ctrl-meta catch-badge">catch</span>{/if}
    {/if}
    {#if data.hasIf}<span class="ctrl-cond">if</span>{/if}
  </div>

  <Handle type="source" position={Position.Bottom} id="out" style="background: {style.color};" />
</div>

<style>
  .ctrl-node {
    background: var(--ctrl-bg);
    border: 2px solid var(--ctrl-color);
    border-radius: 10px;
    min-width: 180px;
    box-shadow: 0 2px 8px rgba(0, 0, 0, 0.08);
    cursor: pointer;
    transition: box-shadow 0.15s;
  }
  .ctrl-node:hover {
    box-shadow: 0 4px 16px rgba(0, 0, 0, 0.12);
  }

  .ctrl-header {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 8px 12px 2px;
  }

  .ctrl-icon {
    font-size: 14px;
    color: var(--ctrl-color);
    line-height: 1;
    flex-shrink: 0;
  }

  .ctrl-label {
    font-size: 13px;
    font-weight: 600;
    color: var(--text-1, #1a1a1a);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .ctrl-badge {
    margin-left: auto;
    font-family: var(--font-mono, monospace);
    font-size: 9px;
    font-weight: 700;
    letter-spacing: 0.06em;
    color: var(--ctrl-color);
    background: white;
    border: 1px solid var(--ctrl-color);
    padding: 1px 5px;
    border-radius: 3px;
    flex-shrink: 0;
  }

  .ctrl-details {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 2px 12px 8px;
    flex-wrap: wrap;
  }

  .ctrl-meta {
    font-family: var(--font-mono, monospace);
    font-size: 10px;
    color: var(--text-3, #7a756c);
  }
  .ctrl-meta code {
    color: var(--ctrl-color);
    font-weight: 500;
  }

  .catch-badge {
    background: #fef2f2;
    color: #dc2626;
    padding: 0 4px;
    border-radius: 2px;
    border: 1px solid #fecaca;
    font-weight: 600;
  }

  .ctrl-cond {
    font-family: var(--font-mono, monospace);
    font-size: 9px;
    font-weight: 600;
    color: #d97706;
    background: #fffbeb;
    border: 1px solid #fde68a;
    padding: 0 4px;
    border-radius: 2px;
  }
</style>
