<script lang="ts">
  /// One of an agent's connectors drawn as a node, when the agent is opened on
  /// its own: its input, its output, each error it can raise, each extension.
  /// Outputs and errors keep a connector of their own, so other agents or flows
  /// can be joined to them.
  import { Handle, Position } from '@xyflow/svelte';

  let { data }: { data: any } = $props();
  const kind = $derived<'input' | 'output' | 'error' | 'extension'>(data.port);
  const ICON = { input: '→', output: '→', error: '⚠', extension: '⊕' } as const;
</script>

<div class="ap {kind}" title={data.detail ?? ''}>
  {#if kind === 'input'}
    <Handle type="source" position={Position.Right} id="out" class="ap-h" />
  {:else if kind === 'extension'}
    <Handle type="target" position={Position.Top} id="in" class="ap-h" />
    <Handle type="source" position={Position.Bottom} id="out" class="ap-h" />
  {:else}
    <Handle type="target" position={Position.Left} id="in" class="ap-h" />
    <Handle type="source" position={Position.Right} id="out" class="ap-h" />
  {/if}
  <span class="ic">{ICON[kind]}</span>
  <div class="tx">
    <span class="k">{data.label}</span>
    {#if data.sub}<span class="s">{data.sub}</span>{/if}
  </div>
</div>

<style>
  .ap { --c: #0891b2; display: flex; align-items: center; gap: 7px; min-width: 120px; max-width: 190px; padding: 6px 10px;
        border: 1.5px solid var(--c); border-radius: 4px; background: var(--bg-surface, #fff); font-size: 11.5px; }
  .ap.output { --c: #059669; }
  .ap.error { --c: #dc2626; background: #fff7f7; }
  .ap.extension { --c: #b45309; border-style: dashed; }
  .ic { color: var(--c); font-weight: 700; width: 14px; text-align: center; }
  .tx { display: flex; flex-direction: column; min-width: 0; }
  .k { font-weight: 600; color: var(--text-1, #1a1a1a); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .s { font: 9.5px var(--font-mono); color: var(--text-3, #777); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .ap :global(.ap-h) { width: 8px !important; height: 8px !important; border: 2px solid #fff !important; background: var(--c) !important; border-radius: 2px !important; }
</style>
