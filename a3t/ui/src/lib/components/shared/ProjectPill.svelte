<script lang="ts">
  /// The project an item belongs to, as a pill on its card. Clicking it filters
  /// the dash to that project; clicking the selected project's pill clears the
  /// filter. The card's own click still opens the item.
  import { getProject, isPlatform, toggleProject } from '$lib/stores/context.svelte.ts';

  let { project }: { project?: string } = $props();

  const current = $derived(getProject() === project);
  const platform = $derived(isPlatform(project));

  function pick(e: Event) {
    e.stopPropagation();
    if (project) toggleProject(project);
  }
</script>

{#if project}
  <button class="pill" class:current class:platform onclick={pick} onkeydown={(e) => e.stopPropagation()}
          title={current ? `Filtered to ${project} — click to show all projects` : `Show only ${project}`}>
    <span class="dot" aria-hidden="true"></span>{project}
  </button>
{/if}

<style>
  .pill {
    display: inline-flex; align-items: center; gap: 5px; max-width: 100%;
    height: 20px; padding: 0 8px; border-radius: 10px;
    border: 1px solid var(--border); background: var(--bg-subtle);
    font-family: var(--font-mono); font-size: 10px; color: var(--text-3);
    cursor: pointer; white-space: nowrap; overflow: hidden; text-overflow: ellipsis;
  }
  .pill:hover { border-color: var(--spec); color: var(--spec); }
  .pill.current { background: var(--spec-bg); border-color: var(--spec); color: var(--spec); }
  .pill.platform { font-style: italic; }
  .dot { width: 6px; height: 6px; border-radius: 50%; background: currentColor; opacity: 0.7; flex: none; }
</style>
