<script lang="ts">
  import { Handle, Position } from '@xyflow/svelte';

  let { data, id }: { data: Record<string, any>; id: string } = $props();

  // Task type → icon and accent
  const TYPE_STYLES: Record<string, { icon: string; accent: string }> = {
    'call:http':       { icon: '⊕',  accent: '#059669' },
    'call:grpc':       { icon: '⇌',  accent: '#059669' },
    'call:openapi':    { icon: '◎',  accent: '#059669' },
    'call:asyncapi':   { icon: '⇶',  accent: '#0891b2' },
    'call:a2a':        { icon: '⬡',  accent: '#7c3aed' },
    'call:llm':        { icon: '✦',  accent: '#8b5cf6' },
    'call:a3t:agent':  { icon: '⬢',  accent: '#5530a8' },
    'call:agent':      { icon: '⬢',  accent: '#5530a8' },
    'call:mcp':        { icon: '⊙',  accent: '#6d28d9' },
    'call:lancedb':    { icon: '▽',  accent: '#0369a1' },
    'call:clawdb':     { icon: '⊛',  accent: '#0369a1' },
    'set':             { icon: '≔',  accent: '#64748b' },
    'wait':            { icon: '◷',  accent: '#d97706' },
    'raise':           { icon: '⚠',  accent: '#dc2626' },
    'emit':            { icon: '◈',  accent: '#0891b2' },
    'listen':          { icon: '◉',  accent: '#0891b2' },
    'run:shell':       { icon: '▸',  accent: '#4f46e5' },
    'run:script':      { icon: '▹',  accent: '#4f46e5' },
    'run:workflow':    { icon: '⊳',  accent: '#c47028' },
    'run:container':   { icon: '☐',  accent: '#4f46e5' },
    'state:get':       { icon: '↩',  accent: '#64748b' },
    'state:set':       { icon: '↪',  accent: '#64748b' },
    'pass':            { icon: '·',  accent: '#94a3b8' },
  };

  interface BottomPort {
    id: string;
    label: string;
    kind: 'closed' | 'open';   // closed = config/integration fork, open = extension fork
  }

  const callType: string = data.callType ?? 'step';
  const style = TYPE_STYLES[callType] ?? { icon: '•', accent: '#64748b' };
  const bottomPorts: BottomPort[] = data.bottomPorts ?? [];
  const hasError = data.hasError ?? false;

  function shortType(t: string): string {
    if (t.startsWith('call:'))   return t.slice(5);
    if (t.startsWith('run:'))    return t.slice(4);
    return t;
  }
</script>

<div class="wfn" style="--accent: {style.accent};">
  <!-- Error port on top -->
  {#if hasError}
    <div class="wfn-port-top">
      <Handle type="source" position={Position.Top} id="error" style="background: #dc2626;" />
      <span class="wfn-port-label top">Error</span>
    </div>
  {/if}

  <!-- Input plug (left) -->
  <div class="wfn-plug left">
    <Handle type="target" position={Position.Left} id="in" style="background: var(--accent);" />
  </div>

  <!-- Main box -->
  <div class="wfn-box">
    <span class="wfn-icon" style="background: var(--accent);">{style.icon}</span>
    <div class="wfn-text">
      <span class="wfn-name">{data.label || id}</span>
      {#if data.subtitle}
        <span class="wfn-sub">{data.subtitle}</span>
      {:else}
        <span class="wfn-sub">{shortType(callType)}</span>
      {/if}
    </div>
  </div>

  <!-- Output socket (right) -->
  <div class="wfn-plug right">
    <Handle type="source" position={Position.Right} id="out" style="background: var(--accent);" />
  </div>

  <!-- Bottom coupling ports -->
  {#if bottomPorts.length > 0}
    <div class="wfn-bottom-ports">
      {#each bottomPorts as port}
        <div class="wfn-bport">
          <div class="wfn-bport-stem"></div>
          <div class="wfn-bport-fork {port.kind}">
            {#if port.kind === 'open'}⏚{:else}⏚{/if}
          </div>
          <span class="wfn-bport-label">{port.label}</span>
          <Handle type="source" position={Position.Bottom} id={port.id} style="background: var(--accent); bottom: -4px;" />
        </div>
      {/each}
    </div>
  {/if}
</div>

<style>
  .wfn {
    position: relative;
    display: flex;
    flex-direction: column;
    align-items: center;
  }

  /* ── Main box ── */
  .wfn-box {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 12px 16px;
    background: linear-gradient(180deg, #ffffff 0%, #f9f9fb 100%);
    border: 1.5px solid var(--accent);
    border-radius: 12px;
    min-width: 160px;
    box-shadow:
      0 1px 0 0 rgba(255,255,255,0.8) inset,
      0 2px 4px rgba(0,0,0,0.04),
      0 6px 16px rgba(0,0,0,0.05);
    position: relative;
    z-index: 1;
    cursor: pointer;
    transition: box-shadow 0.2s, transform 0.15s;
  }
  .wfn-box:hover {
    box-shadow:
      0 1px 0 0 rgba(255,255,255,0.8) inset,
      0 4px 8px rgba(0,0,0,0.06),
      0 10px 24px rgba(0,0,0,0.07);
    transform: translateY(-1px);
  }

  .wfn-icon {
    width: 28px; height: 28px; border-radius: 7px;
    display: grid; place-items: center;
    color: white; font-size: 14px; font-weight: 700;
    flex-shrink: 0; line-height: 1;
  }

  .wfn-text { display: flex; flex-direction: column; overflow: hidden; }
  .wfn-name {
    font-size: 13px; font-weight: 600; color: var(--text-1, #1a1a1a);
    white-space: nowrap; overflow: hidden; text-overflow: ellipsis;
  }
  .wfn-sub {
    font-family: var(--font-mono, monospace); font-size: 9px;
    color: var(--text-3, #999); letter-spacing: 0.02em;
  }

  /* ── Input/Output plugs ── */
  .wfn-plug {
    position: absolute; top: 50%; z-index: 2;
  }
  .wfn-plug.left { left: -8px; transform: translateY(-50%); }
  .wfn-plug.right { right: -8px; transform: translateY(-50%); }

  /* ── Error port top ── */
  .wfn-port-top {
    display: flex; flex-direction: column; align-items: center;
    margin-bottom: 4px; position: relative;
  }
  .wfn-port-label.top {
    font-family: var(--font-mono); font-size: 8px; color: #dc2626;
    margin-bottom: 2px;
  }

  /* ── Bottom coupling ports ── */
  .wfn-bottom-ports {
    display: flex;
    justify-content: center;
    gap: 16px;
    margin-top: 0;
    position: relative;
  }

  .wfn-bport {
    display: flex; flex-direction: column; align-items: center;
    position: relative;
  }

  .wfn-bport-stem {
    width: 2px; height: 12px;
    background: var(--accent);
    opacity: 0.4;
  }

  .wfn-bport-fork {
    width: 20px; height: 14px;
    display: grid; place-items: center;
    font-size: 14px; line-height: 1;
    color: var(--accent); opacity: 0.6;
    position: relative;
  }
  .wfn-bport-fork.open {
    /* open coupling = dashed style for extension hooks */
    opacity: 0.5;
  }
  .wfn-bport-fork.closed {
    /* closed coupling = solid for integrations/config */
    opacity: 0.8;
  }

  .wfn-bport-label {
    font-family: var(--font-mono, monospace);
    font-size: 8px; color: var(--text-4, #9c9789);
    white-space: nowrap; margin-top: 1px;
    max-width: 60px; overflow: hidden; text-overflow: ellipsis;
  }
</style>
