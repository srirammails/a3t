<script lang="ts">
  // The canonical create/edit popin used everywhere (Template → Resource → Agent
  // → Policy → Identity → Canvas): a SlidePanel with a PropertiesTree "designer"
  // rail on the LEFT and the tabbed form on the RIGHT. Pass tree `nodes` +
  // `activeNode`/`onselect`; render the matching form section as children.
  import SlidePanel from './SlidePanel.svelte';
  import PropertiesTree from './PropertiesTree.svelte';
  import type { Snippet } from 'svelte';

  let {
    open = false,
    onclose,
    onsave,
    icon = '+',
    color = 'var(--spec-ink, #6a75c0)',
    name = '',
    meta = '',
    nodes = [],
    activeNode = '',
    onselect,
    children,
  }: {
    open: boolean;
    onclose: () => void;
    onsave?: () => void;
    icon?: string;
    color?: string;
    name?: string;
    meta?: string;
    nodes: { id: string; label: string; icon: string }[];
    activeNode: string;
    onselect: (id: string) => void;
    children: Snippet;
  } = $props();
</script>

<SlidePanel {open} {onclose} {onsave}>
  <div class="panel-layout">
    <PropertiesTree {icon} {color} {name} {meta} {nodes} {activeNode} {onselect} />
    <div class="panel-content">{@render children()}</div>
  </div>
</SlidePanel>

<style>
  .panel-layout { display: grid; grid-template-columns: 200px 1fr; height: 100%; }
  .panel-content { overflow-y: auto; }

  /* Shared form styling so every popin's right-hand form is identical. Global so
     it applies to the children rendered in the consumer's scope. */
  :global(.pf) { padding: 24px 32px; max-width: 660px; }
  :global(.pf h3) { font-family: var(--font-serif, serif); font-size: 18px; font-weight: 500; margin-bottom: 16px; }
  :global(.pf-hint) { font-size: 12px; color: var(--text-4); margin: -4px 0 14px; }
  :global(.cf-grid) { display: grid; grid-template-columns: 1fr 1fr; gap: 12px; }
  :global(.cf-field) { display: flex; flex-direction: column; gap: 5px; margin-bottom: 14px; font-size: 12px; color: var(--text-3); }
  :global(.cf-field > span) { color: var(--text-3); }
  :global(.cf-field input),
  :global(.cf-field select),
  :global(.cf-field textarea) {
    padding: 8px 10px; border: 1px solid var(--border); border-radius: 6px;
    background: var(--bg-sunken, var(--bg-surface)); color: var(--text-1); font-size: 13px; width: 100%; outline: none;
  }
  :global(.cf-field textarea.code) { font-family: var(--font-mono, monospace); font-size: 12.5px; line-height: 1.5; resize: vertical; }
  :global(.cf-field input[readonly]) { opacity: 0.7; }
  :global(.cf-check) { flex-direction: row; align-items: center; gap: 8px; }
  :global(.cf-check input) { width: auto; }
  :global(.pf-row) { display: flex; justify-content: space-between; align-items: center; padding: 9px 0; border-bottom: 1px solid var(--divider); font-size: 13px; gap: 12px; }
  :global(.pf-row:last-child) { border-bottom: 0; }
  :global(.pf-k) { color: var(--text-3); flex-shrink: 0; }
  :global(.pf-v) { color: var(--text-1); text-align: right; word-break: break-word; }
  :global(.pf-code) { font-family: var(--font-mono, monospace); font-size: 12px; line-height: 1.6; padding: 12px 14px; margin: 0; background: var(--bg-subtle); border: 1px solid var(--border); border-radius: 6px; white-space: pre-wrap; overflow: auto; max-height: 320px; }
  :global(.create-actions) { display: flex; gap: 10px; align-items: center; margin-top: 20px; padding-top: 16px; border-top: 1px solid var(--divider); }
  :global(.create-actions .grow) { flex: 1; }
  :global(.cf-msg) { font-size: 12px; color: var(--text-3); }
  :global(.cf-msg.err) { color: var(--warn, #d66); }
  :global(.cf-msg.ok) { color: #3cb46e; }
</style>
