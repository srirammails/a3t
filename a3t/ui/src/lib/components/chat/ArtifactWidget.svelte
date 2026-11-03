<script lang="ts">
  // A single artifact (flow, agent, template, resource, trigger) as a compact
  // handoff card. The body opens in the right-hand pane, which has the width.
  import { openInStudio } from '$lib/stores/panels.svelte';
  import { ARTIFACTS, toEditorText, type ArtifactKind } from './artifacts';

  let { kind, data, onopen }: {
    kind: ArtifactKind;
    data: { namespace?: string; name?: string; raw?: string };
    onopen?: (a: { kind: ArtifactKind; namespace: string; name: string; text: string; id?: string }) => void;
  } = $props();

  const spec = $derived(ARTIFACTS[kind]);
  const ns = $derived(data?.namespace ?? 'default');
  const name = $derived(data?.name ?? '');
  const text = $derived(toEditorText(data?.raw ?? '', spec.format));

  // Everything in the registry opens in the Studio; a draft is not there yet.
  const hasPanel = $derived(kind !== 'flow_draft' && !!name);
</script>

<div class="widget">
  <div class="widget-head">
    <div class="widget-id">
      <span class="widget-kind">{spec.label}</span>
      <span class="widget-name">{ns}/{name}</span>
    </div>
    <div class="widget-actions">
      <button class="wbtn" class:accent={spec.draft} onclick={() => onopen?.({ kind, namespace: ns, name, text })}>
        {spec.draft ? 'Review' : 'Open'}
      </button>
      {#if hasPanel}
        <button class="wbtn" title="Open in Studio"
          onclick={() => openInStudio(kind as Exclude<ArtifactKind, 'flow_draft'>, ns, name)}>↗</button>
      {/if}
    </div>
  </div>
</div>

<style>
  .widget { margin-top: 8px; border: 1px solid var(--border); border-radius: 8px;
            background: var(--bg-subtle); overflow: hidden; }
  .widget-head { display: flex; align-items: center; justify-content: space-between;
                 gap: 8px; padding: 7px 10px; }
  .widget-id { display: flex; align-items: center; gap: 8px; min-width: 0; }
  .widget-kind { font-size: 10px; text-transform: uppercase; letter-spacing: 0.05em;
                 color: var(--text-3); border: 1px solid var(--border); border-radius: 4px; padding: 1px 5px; }
  .widget-name { font-family: var(--font-mono); font-size: 12px; color: var(--text-1);
                 white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .widget-actions { display: flex; gap: 5px; flex-shrink: 0; }
  .wbtn { font-size: 11px; padding: 3px 8px; cursor: pointer; border-radius: 5px;
          border: 1px solid var(--border); background: var(--bg-surface); color: var(--text-2); }
  .wbtn:hover { color: var(--text-1); border-color: var(--border-strong); }
  .wbtn.accent { border-color: var(--ephemeral, #7c5cff); color: var(--ephemeral, #7c5cff); }
</style>
