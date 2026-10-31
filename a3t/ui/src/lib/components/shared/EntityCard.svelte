<script lang="ts">
  import ProjectPill from './ProjectPill.svelte';
  let {
    icon = '?',
    color = '#999',
    name = '',
    description = '',
    tags = [] as string[],
    meta = '',
    actionLabel = '',
    onclick,
    onaction,
    ondelete,
    project = '',
    iconStyle = '',
    pinned,
    onpin,
  }: {
    icon: string;
    color: string;
    name: string;
    description: string;
    tags: string[];
    meta: string;
    actionLabel: string;
    onclick: () => void;
    onaction?: () => void;
    ondelete?: () => void;
    /// The item's project (namespace); shown as a pill that switches to it.
    project?: string;
    /// Overrides the icon's colours (a badge's own style); `color` otherwise.
    iconStyle?: string;
    /// With `onpin`, a star to pin or unpin the item.
    pinned?: boolean;
    onpin?: () => void;
  } = $props();
</script>

<div class="entity-card">
  {#if ondelete}
    <button class="ec-delete" onclick={ondelete} title="Delete">
      <svg width="12" height="12" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5"><path d="M4 4l8 8M12 4l-8 8"/></svg>
    </button>
  {/if}
  {#if onpin}
    <button class="ec-pin" class:on={pinned} class:shifted={!!ondelete} onclick={onpin}
            title={pinned ? 'Unpin' : 'Pin'} aria-label={pinned ? 'Unpin' : 'Pin'}>{pinned ? '★' : '☆'}</button>
  {/if}
  <div class="ec-click" role="button" tabindex="0" {onclick} onkeydown={(e) => { if (e.key === 'Enter') onclick(); }}>
    <div class="ec-icon" style={iconStyle || `background: ${color}`}>{icon}</div>
    <div class="ec-body">
      <div class="ec-name">{name}</div>
      {#if description}<div class="ec-desc">{description}</div>{/if}
      {#if tags.length > 0}
        <div class="ec-tags">{#each tags as tag}<span class="ec-tag">{tag}</span>{/each}</div>
      {/if}
      {#if meta || project}
        <div class="ec-foot">
          {#if project}<ProjectPill {project} />{/if}
          {#if meta}<span class="ec-meta">{meta}</span>{/if}
        </div>
      {/if}
    </div>
  </div>
  {#if onaction && actionLabel}
    <button class="ec-action" onclick={onaction}>{actionLabel}</button>
  {/if}
</div>

<style>
  .entity-card { background: var(--bg-surface); border: 1px solid var(--border); border-radius: var(--r); box-shadow: var(--shadow-sm); transition: all 0.15s; overflow: hidden; position: relative; }
  .entity-card:hover { box-shadow: var(--shadow-lg); border-color: var(--border-strong); transform: translateY(-1px); }

  .ec-click { display: grid; grid-template-columns: 40px 1fr; gap: 14px; padding: 16px 18px 12px; cursor: pointer; text-align: left; }
  .ec-icon { width: 40px; height: 40px; border-radius: var(--r-sm); color: white; display: grid; place-items: center; font-size: 12px; font-weight: 700; font-family: var(--font-mono); }
  .ec-body { display: flex; flex-direction: column; gap: 5px; min-width: 0; }
  .ec-name { font-size: 14px; font-weight: 550; }
  .ec-desc { font-size: 12px; color: var(--text-2); line-height: 1.4; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .ec-tags { display: flex; flex-wrap: wrap; gap: 3px; }
  .ec-tag { font-family: var(--font-mono); font-size: 10px; padding: 1px 6px; border-radius: 3px; background: var(--bg-subtle); color: var(--text-3); }
  .ec-meta { font-size: 11px; color: var(--text-4); font-family: var(--font-mono); }
  .ec-foot { display: flex; align-items: center; gap: 8px; margin-top: 2px; min-width: 0; }

  .ec-pin {
    position: absolute; top: 8px; right: 8px; z-index: 1;
    width: 24px; height: 24px; border: none; border-radius: 4px;
    background: none; color: var(--text-5); cursor: pointer; font-size: 13px; line-height: 1;
    opacity: 0; transition: opacity 0.1s;
  }
  .ec-pin.shifted { right: 36px; }
  .ec-pin.on { opacity: 1; color: var(--spec-ink, var(--text-1)); }
  .entity-card:hover .ec-pin { opacity: 1; }
  .ec-pin:hover { background: var(--bg-subtle); color: var(--text-1); }

  .ec-delete {
    position: absolute; top: 8px; right: 8px;
    width: 24px; height: 24px; border: 1px solid transparent; border-radius: 4px;
    background: none; color: var(--text-5); cursor: pointer; display: grid; place-items: center;
    opacity: 0; transition: opacity 0.1s;
  }
  .entity-card:hover .ec-delete { opacity: 1; }
  .ec-delete:hover { background: var(--err-bg, #fde8e8); color: var(--err, #d44); border-color: var(--err); }

  .ec-action {
    display: block; width: 100%; padding: 8px; border: none;
    border-top: 1px solid var(--divider); background: none;
    font-size: 12px; font-weight: 500; color: var(--text-3);
    cursor: pointer; transition: all 0.12s;
  }
  .ec-action:hover { background: var(--spec-bg, #fbf0e0); color: var(--spec, #c47028); }
</style>
