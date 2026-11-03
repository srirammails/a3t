<script lang="ts">
  /// An agent on the graph: a square block, its name and version on a label
  /// above it. Its connectors: input (left), output (right), errors (right,
  /// red) and extensions (bottom). Opened on its own (a template), each of
  /// those is drawn as a node of its own; inside a flow the agent is one unit
  /// and the connectors are what other steps join to.
  import { Handle, Position } from '@xyflow/svelte';

  let { data }: { data: any } = $props();
  const errors = $derived<number>(data.errorCount ?? 0);
  const exts = $derived<number>(data.extensionCount ?? 0);
  const compact = $derived(!data.detailed);
</script>

<div class="ab" class:compact>
  <div class="lbl" title={data.label}>
    <span class="nm">{data.label ?? 'agent'}</span>
    {#if data.version}<span class="ver">{data.version}</span>{/if}
  </div>
  <div class="sq">
    <Handle type="target" position={Position.Left} id="in" class="ab-h ab-in" />
    <svg class="ic" width="26" height="26" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" aria-hidden="true">
      <rect x="4" y="7" width="16" height="12" rx="3" /><circle cx="9.5" cy="13" r="1.3" fill="currentColor" /><circle cx="14.5" cy="13" r="1.3" fill="currentColor" />
      <path d="M12 3v4M9 3h6" />
    </svg>
    <span class="kind">{data.kindLabel ?? 'agent'}</span>
    {#if data.model}<span class="model" title={data.model}>{data.model}</span>{/if}
    <Handle type="source" position={Position.Right} id="out" class="ab-h ab-out" />
    <Handle type="source" position={Position.Right} id="err" class="ab-h ab-err" />
    <Handle type="source" position={Position.Bottom} id="ext" class="ab-h ab-ext" />
    {#if compact}
      <span class="tag t-in">in</span>
      <span class="tag t-out">out</span>
      <span class="tag t-err" class:has={errors}>err{errors ? ` ${errors}` : ''}</span>
      <span class="tag t-ext" class:has={exts}>ext{exts ? ` ${exts}` : ''}</span>
    {/if}
  </div>
</div>

<style>
  .ab { --c: #5530a8; display: flex; flex-direction: column; align-items: center; gap: 6px; width: 160px; }
  .lbl { max-width: 160px; display: flex; align-items: center; gap: 6px; padding: 3px 9px; border: 1px solid #d4c4f0; border-radius: 4px;
         background: var(--bg-surface, #fff); font-size: 11.5px; white-space: nowrap; }
  .nm { font-weight: 600; color: var(--text-1, #1a1a1a); overflow: hidden; text-overflow: ellipsis; }
  .ver { font: 10px var(--font-mono); color: var(--c); background: #f3effc; padding: 0 4px; border-radius: 3px; flex-shrink: 0; }
  .sq { position: relative; width: 104px; height: 104px; border: 2px solid var(--c); border-radius: 6px;
        background: linear-gradient(180deg, #ffffff 0%, #f6f2ff 100%); box-shadow: 0 2px 6px rgba(85, 48, 168, 0.1);
        display: flex; flex-direction: column; align-items: center; justify-content: center; gap: 3px; color: var(--c); }
  .kind { font: 600 10px var(--font-mono); text-transform: uppercase; letter-spacing: 0.06em; }
  .model { font: 9.5px var(--font-mono); color: var(--text-3, #777); max-width: 92px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }

  .ab :global(.ab-h) { width: 9px !important; height: 9px !important; border: 2px solid #fff !important; background: var(--c) !important; border-radius: 2px !important; }
  .ab :global(.ab-out) { top: 38% !important; }
  .ab :global(.ab-err) { top: 72% !important; background: #dc2626 !important; }
  .ab :global(.ab-ext) { background: #b45309 !important; }

  .tag { position: absolute; font: 600 8.5px var(--font-mono); color: var(--text-4, #999); pointer-events: none; }
  .t-in { left: 8px; top: 50%; transform: translateY(-50%); }
  .t-out { right: 8px; top: 38%; transform: translateY(-50%); }
  .t-err { right: 8px; top: 72%; transform: translateY(-50%); }
  .t-err.has { color: #dc2626; }
  .t-ext { bottom: 5px; left: 50%; transform: translateX(-50%); }
  .t-ext.has { color: #b45309; }
  .compact .model { max-width: 70px; }
</style>
