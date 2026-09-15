<script lang="ts">
  let { type }: { type: string } = $props();
</script>

<div class="screen-header">
  <div class="breadcrumb">{type}</div>
  <div class="screen-title-row">
    <h1 class="screen-title">{type.split('-').map(w => w[0].toUpperCase() + w.slice(1)).join(' ')}</h1>
  </div>
</div>

<div class="placeholder-body">
  <div class="placeholder-inner">
    <svg width="48" height="48" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.2" opacity="0.25"><rect x="3" y="3" width="18" height="18" rx="2"/><path d="M3 9h18M9 21V9"/></svg>
    <h2>Under construction</h2>
    <p>This view will be available in a future release.</p>
  </div>
</div>

<style>
  .screen-header { padding: 22px 40px 14px; border-bottom: 1px solid var(--border); background: var(--bg-page); }
  .breadcrumb { font-size: 12px; color: var(--text-3); margin-bottom: 10px; font-family: var(--font-mono); }
  .screen-title-row { display: flex; align-items: flex-end; gap: 18px; }
  .screen-title { font-family: var(--font-serif); font-size: 28px; font-weight: 500; letter-spacing: -0.02em; }

  .placeholder-body { flex: 1; display: flex; align-items: center; justify-content: center; padding: 60px 40px; }
  .placeholder-inner { text-align: center; color: var(--text-4); display: flex; flex-direction: column; align-items: center; gap: 12px; }
  .placeholder-inner h2 { font-family: var(--font-serif); font-size: 20px; font-weight: 500; color: var(--text-3); }
  .placeholder-inner p { font-size: 14px; }
</style>
