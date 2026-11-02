<script lang="ts">
  /// The runs of an agent's chat flow (bottom region): debug, timeline,
  /// journal, checkpoints — the Runs panel flows have. The chat flow is shared
  /// by the agents it serves, so its runs are theirs together.
  import FlowRunPanel from '$lib/components/run/FlowRunPanel.svelte';
  import { fromText, type Doc } from '$lib/studio-core';

  let { doc }: { doc: Doc | null } = $props();
  const target = $derived(String((fromText(doc?.text).value ?? {})?.serving?.target ?? ''));
  const [ns, name] = $derived(target.includes('/') ? [target.split('/')[0], target.split('/').slice(1).join('/')] : ['', '']);
</script>

{#if !doc || doc.isNew}
  <div class="note">Save the agent to see its runs.</div>
{:else if !name}
  <div class="note">This agent has no chat flow (Serving, in the Form panel).</div>
{:else}
  <div class="rp">
    <div class="shared">Runs of <code>{target}</code> — shared by every agent it serves.</div>
    {#key target}<FlowRunPanel {ns} {name} schema={null} steps={[]} />{/key}
  </div>
{/if}

<style>
  .rp { display: flex; flex-direction: column; height: 100%; min-height: 0; }
  .rp > :global(section) { flex: 1; min-height: 0; }
  .shared { flex: 0 0 auto; padding: 4px 12px; font-size: 11.5px; color: var(--text-3); border-bottom: 1px solid var(--border); }
  code { font-family: var(--font-mono); }
  .note { padding: 14px; color: var(--text-4); font-size: 12.5px; }
</style>
