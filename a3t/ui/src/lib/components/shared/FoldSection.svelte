<script lang="ts">
  /// A foldable section of a section page: chevron, serif title, count, hint.
  /// The same shape as the pages' "My …" / "Shared …" groups, so every list on a
  /// page folds the same way.
  ///
  /// An empty section is dimmed and can't be opened. `open` starts from
  /// `defaultOpen` (e.g. Pinned opens only when something is pinned) and is the
  /// user's to toggle after that.
  import type { Snippet } from 'svelte';

  let {
    title,
    count = 0,
    hint = '',
    defaultOpen = true,
    children,
  }: {
    title: string;
    count?: number;
    hint?: string;
    defaultOpen?: boolean;
    children?: Snippet;
  } = $props();

  let toggled = $state<boolean | null>(null);
  const open = $derived(count > 0 && (toggled ?? defaultOpen));
</script>

<div class="fold" class:empty={count === 0}>
  <button class="fold-head" onclick={() => { if (count > 0) toggled = !open; }} aria-expanded={open} disabled={count === 0}>
    <svg class="fold-chev" class:open width="12" height="12" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="2"><path d="M6 4l4 4-4 4"/></svg>
    <h3>{title}</h3>
    <span class="fold-count">{count}</span>
    {#if hint}<span class="fold-hint">{hint}</span>{/if}
  </button>
  {#if open && children}
    <div class="fold-body">{@render children()}</div>
  {/if}
</div>

<style>
  .fold { margin-bottom: 24px; }
  .fold-head {
    display: flex; align-items: center; gap: 8px; width: 100%;
    padding: 0 0 10px; border: none; background: none; cursor: pointer; text-align: left;
  }
  .fold-head h3 { margin: 0; font-family: var(--font-serif); font-size: 16px; font-weight: 500; color: var(--text-1); }
  .fold-count { font-family: var(--font-mono); font-size: 11px; color: var(--text-4); background: var(--bg-subtle); padding: 1px 6px; border-radius: 3px; }
  .fold-hint { font-size: 11px; color: var(--text-4); font-style: italic; margin-left: auto; }
  .fold-chev { color: var(--text-4); transition: transform 0.2s; flex-shrink: 0; }
  .fold-chev.open { transform: rotate(90deg); }
  .fold.empty { opacity: 0.5; }
  .fold.empty .fold-head { cursor: default; }
</style>
