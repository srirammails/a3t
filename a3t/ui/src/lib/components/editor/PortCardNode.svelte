<script lang="ts">
  import { Handle, Position } from '@xyflow/svelte';
  let { data }: { data: any } = $props();

  // Section styles
  const SECTION_STYLES: Record<string, { icon: string; color: string; dashed: boolean }> = {
    config:       { icon: '⚙', color: '#5530a8', dashed: false },
    integrations: { icon: '🔗', color: '#5530a8', dashed: false },
    extensions:   { icon: '🧩', color: '#5530a8', dashed: true },
    events:       { icon: '◎', color: '#5530a8', dashed: false },
  };

  const section = data.section ?? 'config';
  const style = SECTION_STYLES[section] ?? SECTION_STYLES.config;
  const items: Array<{ name: string; type: string; typeColor?: string }> = data.items ?? [];
  const addLabel = data.addLabel ?? '+ Add';
  const maxShow = 5;
  const moreCount = Math.max(0, items.length - maxShow);
  const visibleItems = items.slice(0, maxShow);
</script>

<div class="pc" class:dashed={style.dashed}>
  <Handle type="target" position={Position.Top} id="in" style="opacity:0; top:-4px;" />

  <div class="pc-titlebar">
    <span class="pc-tb-icon">{style.icon}</span>
    <span class="pc-tb-title">{data.label ?? section}</span>
    <span class="pc-tb-close">×</span>
  </div>

  <div class="pc-items">
    {#each visibleItems as item}
      <div class="pc-row">
        <span class="pc-item-name">{item.name}</span>
        <span class="pc-item-type" class:fn={item.type === 'function'}
          style="color: {item.typeColor ?? 'var(--text-3)'}; background: {item.type === 'function' ? '#f3eefb' : 'var(--bg-subtle)'}; border-color: {item.type === 'function' ? '#d4c4f0' : 'var(--border)'};">
          {item.type}
        </span>
      </div>
    {/each}
  </div>

  {#if moreCount > 0}
    <div class="pc-more">+{moreCount} more fields</div>
  {/if}

  <div class="pc-add">{addLabel}</div>
</div>

<style>
  .pc {
    background: linear-gradient(180deg, #ffffff 0%, #faf9ff 100%);
    border: 1.5px solid #d4c4f0;
    border-radius: 14px;
    min-width: 210px; max-width: 260px;
    box-shadow:
      0 1px 0 0 rgba(255,255,255,0.9) inset,
      0 2px 6px rgba(85, 48, 168, 0.05),
      0 8px 24px rgba(85, 48, 168, 0.06),
      0 16px 40px rgba(0, 0, 0, 0.02);
    cursor: pointer;
    transition: box-shadow 0.2s, transform 0.15s;
  }
  .pc:hover {
    box-shadow:
      0 1px 0 0 rgba(255,255,255,0.9) inset,
      0 4px 10px rgba(85, 48, 168, 0.08),
      0 12px 32px rgba(85, 48, 168, 0.08);
    transform: translateY(-1px);
  }
  .pc.dashed { border-style: dashed; border-color: #c4b0e8; }

  .pc-titlebar {
    display: flex; align-items: center; gap: 8px;
    padding: 8px 14px;
    background: linear-gradient(180deg, #f3eefb 0%, #ede7ff 100%);
    border-bottom: 1px solid #d4c4f0;
    border-radius: 13px 13px 0 0;
  }
  .pc-tb-icon { font-size: 14px; }
  .pc-tb-title { font-size: 13px; font-weight: 600; color: var(--text-1, #1a1a1a); flex: 1; }
  .pc-tb-close {
    font-size: 16px; color: var(--text-4); cursor: pointer;
    width: 20px; height: 20px; display: grid; place-items: center;
    border-radius: 4px; transition: background 0.1s;
  }
  .pc-tb-close:hover { background: rgba(0,0,0,0.06); color: var(--text-2); }

  .pc-items { padding: 2px 16px; }

  .pc-row {
    display: flex; align-items: center; justify-content: space-between;
    padding: 7px 0;
    border-bottom: 1px solid rgba(85, 48, 168, 0.06);
  }
  .pc-row:last-child { border-bottom: none; }

  .pc-item-name { font-size: 12.5px; color: var(--text-2, #333); font-weight: 450; }
  .pc-item-type {
    font-family: var(--font-mono, monospace);
    font-size: 9px; font-weight: 600;
    padding: 3px 8px; border-radius: 5px;
    border: 1px solid var(--border);
    letter-spacing: 0.03em;
    background: linear-gradient(180deg, #f8f8f8 0%, #f0f0f0 100%);
    box-shadow: 0 1px 2px rgba(0,0,0,0.04);
  }
  .pc-item-type.fn {
    color: var(--spec, #5530a8) !important;
    background: linear-gradient(180deg, #f3eefb 0%, #ede7ff 100%);
    border-color: #d4c4f0;
  }

  .pc-more {
    padding: 6px 16px 2px;
    font-size: 11px; color: var(--spec, #5530a8);
    font-style: italic; opacity: 0.8;
  }

  .pc-add {
    padding: 10px 16px;
    font-size: 11px; font-weight: 550;
    color: var(--spec, #5530a8);
    cursor: pointer; opacity: 0.6;
    transition: opacity 0.15s;
  }
  .pc-add:hover { opacity: 1; }
</style>
