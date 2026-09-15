<script lang="ts">
  import { Handle, Position } from '@xyflow/svelte';

  let { data, id }: { data: Record<string, any>; id: string } = $props();

  // Task type → icon and accent color
  const TYPE_STYLES: Record<string, { icon: string; accent: string }> = {
    'call:http':     { icon: '⊕',  accent: '#059669' },
    'call:grpc':     { icon: '⇌',  accent: '#059669' },
    'call:openapi':  { icon: '◎',  accent: '#059669' },
    'call:asyncapi': { icon: '⇶',  accent: '#0891b2' },
    'call:a2a':      { icon: '⬡',  accent: '#7c3aed' },
    'call:llm':      { icon: '✦',  accent: '#8b5cf6' },
    'call:agent':    { icon: '⬢',  accent: '#7c3aed' },
    'call:sql':      { icon: '⊞',  accent: '#0369a1' },
    'call:mcp':      { icon: '⊙',  accent: '#6d28d9' },
    'call:redis':    { icon: '⬤',  accent: '#dc2626' },
    'call:qdrant':   { icon: '△',  accent: '#7c3aed' },
    'call:lancedb':  { icon: '▽',  accent: '#0369a1' },
    'call:clawdb':   { icon: '⊛',  accent: '#0369a1' },
    'call:human':    { icon: '☻',  accent: '#ca8a04' },
    'set':           { icon: '≔',  accent: '#64748b' },
    'wait':          { icon: '◷',  accent: '#d97706' },
    'raise':         { icon: '⚠',  accent: '#dc2626' },
    'emit':          { icon: '◈',  accent: '#0891b2' },
    'listen':        { icon: '◉',  accent: '#0891b2' },
    'run:shell':     { icon: '▸',  accent: '#4f46e5' },
    'run:script':    { icon: '▹',  accent: '#4f46e5' },
    'run:workflow':  { icon: '⊳',  accent: '#c47028' },
    'run:container': { icon: '☐',  accent: '#4f46e5' },
    'inline:lua':    { icon: 'L',  accent: '#2563eb' },
    'inline:js':     { icon: 'J',  accent: '#eab308' },
    'inline:python': { icon: 'P',  accent: '#2563eb' },
    'state:get':     { icon: '↩',  accent: '#64748b' },
    'state:set':     { icon: '↪',  accent: '#64748b' },
    'input:file':    { icon: '⤓',  accent: '#64748b' },
    'output:file':   { icon: '⤒',  accent: '#64748b' },
    'pass':          { icon: '·',  accent: '#94a3b8' },
  };

  const callType = data.callType ?? 'step';
  const style = TYPE_STYLES[callType] ?? { icon: '•', accent: '#64748b' };

  // Short type label for display
  function shortType(t: string): string {
    if (t.startsWith('call:'))   return t.slice(5).toUpperCase();
    if (t.startsWith('run:'))    return t.slice(4).toUpperCase();
    if (t.startsWith('inline:')) return t.slice(7).toUpperCase();
    return t.toUpperCase();
  }
</script>

<div class="step-node" class:checkpoint={data.checkpoint} style="--step-accent: {style.accent};">
  <Handle type="target" position={Position.Top} id="in" style="background: {style.accent};" />

  <div class="sn-header">
    <span class="sn-icon" style="background: {style.accent};">{style.icon}</span>
    <span class="sn-label">{data.label || id}</span>
  </div>

  <div class="sn-body">
    <span class="sn-type">{shortType(callType)}</span>
    {#if data.server}<span class="sn-resource">{data.server}</span>{/if}
  </div>

  <div class="sn-badges">
    {#if data.hasIf}
      <span class="sn-badge cond" title={data.ifExpr ?? ''}>if</span>
    {/if}
    {#if data.hasTimeout}
      <span class="sn-badge timeout" title={"timeout: " + data.timeout}>⏱ {data.timeout}</span>
    {/if}
    {#if data.checkpoint}
      <span class="sn-badge chk">checkpoint</span>
    {/if}
    {#if data.exportAs}
      <span class="sn-badge export" title={"export: " + data.exportAs}>export</span>
    {/if}
  </div>

  <Handle type="source" position={Position.Bottom} id="out" style="background: {style.accent};" />
</div>

<style>
  .step-node {
    background: var(--bg-surface, #fff);
    border: 1.5px solid var(--border, #eae6dc);
    border-left: 3px solid var(--step-accent);
    border-radius: 8px;
    min-width: 180px;
    max-width: 220px;
    box-shadow: 0 1px 4px rgba(30, 25, 15, 0.06);
    cursor: pointer;
    transition: border-color 0.15s, box-shadow 0.15s;
  }
  .step-node:hover {
    border-color: var(--step-accent);
    box-shadow: 0 3px 12px rgba(30, 25, 15, 0.1);
  }
  .step-node.checkpoint {
    border-left-width: 4px;
    border-left-color: var(--checkpoint, #c97c2e);
  }

  .sn-header {
    display: flex;
    align-items: center;
    gap: 7px;
    padding: 8px 10px 3px;
  }

  .sn-icon {
    width: 20px;
    height: 20px;
    border-radius: 4px;
    display: grid;
    place-items: center;
    color: white;
    font-size: 11px;
    font-weight: 700;
    flex-shrink: 0;
    line-height: 1;
  }

  .sn-label {
    font-size: 12px;
    font-weight: 600;
    color: var(--text-1, #1a1a1a);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .sn-body {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 0 10px 4px;
    padding-left: 37px;
  }

  .sn-type {
    font-family: var(--font-mono, monospace);
    font-size: 9px;
    font-weight: 600;
    color: var(--step-accent);
    letter-spacing: 0.04em;
  }

  .sn-resource {
    font-family: var(--font-mono, monospace);
    font-size: 9px;
    color: var(--text-3, #7a756c);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    max-width: 100px;
  }

  .sn-badges {
    display: flex;
    flex-wrap: wrap;
    gap: 3px;
    padding: 0 10px 6px;
    padding-left: 37px;
  }

  .sn-badge {
    font-family: var(--font-mono, monospace);
    font-size: 8px;
    font-weight: 600;
    padding: 1px 5px;
    border-radius: 2px;
    letter-spacing: 0.03em;
  }
  .sn-badge.cond {
    background: #fffbeb;
    color: #d97706;
    border: 1px solid #fde68a;
  }
  .sn-badge.timeout {
    background: #fef3c7;
    color: #b45309;
    border: 1px solid #fde68a;
  }
  .sn-badge.chk {
    background: #fbf0e0;
    color: #8a5112;
    border: 1px solid #f5dbb8;
  }
  .sn-badge.export {
    background: #f0fdf4;
    color: #15803d;
    border: 1px solid #bbf7d0;
  }
</style>
