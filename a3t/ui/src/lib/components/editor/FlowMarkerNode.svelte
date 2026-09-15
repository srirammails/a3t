<script lang="ts">
  import { Handle, Position } from '@xyflow/svelte';

  let { data }: { data: Record<string, any> } = $props();

  const isStart = data.marker === 'start';
</script>

<div class="marker-node" class:start={isStart} class:end={!isStart}>
  {#if !isStart}
    <Handle type="target" position={Position.Top} id="in" style="background: {isStart ? 'var(--ok, #23987f)' : 'var(--text-4, #999)'};" />
  {/if}

  <div class="marker-circle">
    {#if isStart}
      <svg width="12" height="12" viewBox="0 0 16 16" fill="currentColor"><path d="M4 2l10 6-10 6V2z"/></svg>
    {:else}
      <svg width="12" height="12" viewBox="0 0 16 16" fill="currentColor"><rect x="3" y="3" width="10" height="10" rx="2"/></svg>
    {/if}
  </div>
  <span class="marker-label">{data.label || (isStart ? 'start' : 'end')}</span>

  {#if isStart}
    <Handle type="source" position={Position.Bottom} id="out" style="background: var(--ok, #23987f);" />
  {/if}
</div>

<style>
  .marker-node {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 4px 0;
  }

  .marker-circle {
    width: 32px;
    height: 32px;
    border-radius: 50%;
    display: grid;
    place-items: center;
    transition: box-shadow 0.15s;
  }

  .start .marker-circle {
    background: var(--ok, #23987f);
    color: white;
    box-shadow: 0 0 0 3px rgba(35, 152, 127, 0.15);
  }
  .end .marker-circle {
    background: var(--text-4, #9c9789);
    color: white;
    box-shadow: 0 0 0 3px rgba(156, 151, 137, 0.15);
  }

  .marker-label {
    font-family: var(--font-mono, monospace);
    font-size: 10px;
    font-weight: 600;
    color: var(--text-3, #7a756c);
    text-transform: uppercase;
    letter-spacing: 0.05em;
  }
</style>
