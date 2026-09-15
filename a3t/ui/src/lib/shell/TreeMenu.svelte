<script lang="ts">
  export interface TreeNode {
    id: string;
    label: string;
    icon?: string;
    children?: TreeNode[];
    badge?: string;
  }

  let {
    nodes = [],
    selectedId = '',
    onSelect,
  }: {
    nodes: TreeNode[];
    selectedId: string;
    onSelect: (id: string) => void;
  } = $props();

  let expanded = $state<Set<string>>(new Set());

  function toggle(id: string) {
    const next = new Set(expanded);
    if (next.has(id)) next.delete(id);
    else next.add(id);
    expanded = next;
  }
</script>

<div class="tree">
  {#each nodes as node}
    <div class="tree-node">
      {#if node.children && node.children.length > 0}
        <button class="tree-item parent" class:expanded={expanded.has(node.id)} onclick={() => toggle(node.id)}>
          <svg class="tree-arrow" width="10" height="10" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="2"><path d="M6 4l4 4-4 4"/></svg>
          <span class="tree-label">{node.label}</span>
          {#if node.badge}<span class="tree-badge">{node.badge}</span>{/if}
        </button>
        {#if expanded.has(node.id)}
          <div class="tree-children">
            {#each node.children as child}
              <button
                class="tree-item leaf"
                class:active={selectedId === child.id}
                onclick={() => onSelect(child.id)}
              >
                <span class="tree-label">{child.label}</span>
                {#if child.badge}<span class="tree-badge">{child.badge}</span>{/if}
              </button>
            {/each}
          </div>
        {/if}
      {:else}
        <button
          class="tree-item leaf"
          class:active={selectedId === node.id}
          onclick={() => onSelect(node.id)}
        >
          <span class="tree-label">{node.label}</span>
          {#if node.badge}<span class="tree-badge">{node.badge}</span>{/if}
        </button>
      {/if}
    </div>
  {/each}
</div>

<style>
  .tree { display: flex; flex-direction: column; gap: 1px; }

  .tree-item {
    display: flex;
    align-items: center;
    gap: 8px;
    width: 100%;
    padding: 7px 10px;
    border: none;
    background: none;
    border-radius: var(--r-sm);
    cursor: pointer;
    font-size: 13px;
    color: var(--text-2);
    text-align: left;
    transition: background 0.1s;
  }
  .tree-item:hover { background: var(--bg-subtle); color: var(--text-1); }
  .tree-item.active { background: var(--bg-subtle); color: var(--text-1); font-weight: 500; }

  .tree-item.parent { color: var(--text-3); font-size: 11px; text-transform: uppercase; letter-spacing: 0.04em; font-weight: 600; }
  .tree-item.parent:hover { color: var(--text-2); }

  .tree-arrow {
    flex-shrink: 0;
    color: var(--text-4);
    transition: transform 0.15s;
  }
  .tree-item.expanded .tree-arrow { transform: rotate(90deg); }

  .tree-children { padding-left: 18px; display: flex; flex-direction: column; gap: 1px; }

  .tree-label { flex: 1; }
  .tree-badge {
    font-family: var(--font-mono);
    font-size: 10px;
    color: var(--text-4);
    background: var(--bg-subtle);
    padding: 1px 5px;
    border-radius: 3px;
  }
</style>
