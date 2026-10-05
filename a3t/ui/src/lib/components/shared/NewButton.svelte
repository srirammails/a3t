<script lang="ts">
  /// The one "+ New …" button every section page uses: top-right of the page
  /// header, solid, in the section's accent colour (the same accent as its
  /// banner). Kept as a component because pages had each restyled `.btn.spec` /
  /// `.btn.runtime` locally — pale here, solid there — so the same class looked
  /// different on every page. Scoped styles here cannot be overridden that way.
  let {
    label, onclick, accent = 'spec', disabled = false, size = 'md', title = '',
  }: {
    /// e.g. "New template" — the "+" is added.
    label: string;
    onclick: () => void;
    accent?: 'spec' | 'runtime' | 'control' | 'ephemeral';
    disabled?: boolean;
    size?: 'md' | 'sm';
    title?: string;
  } = $props();
</script>

<button class="nb {accent} {size}" {onclick} {disabled} title={title || label}>
  <svg width="12" height="12" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="2.2" aria-hidden="true"><path d="M8 3v10M3 8h10" /></svg>
  {label}
</button>

<style>
  .nb {
    display: inline-flex; align-items: center; gap: 7px;
    padding: 8px 14px; border-radius: var(--r-sm);
    font-family: var(--font-sans); font-size: 13px; font-weight: 500; line-height: 1.2;
    color: #fff; border: 1px solid transparent; cursor: pointer;
    box-shadow: var(--shadow-sm); white-space: nowrap;
    transition: filter 0.12s ease;
  }
  .nb.sm { padding: 6px 12px; font-size: 12px; }
  .nb:hover:not(:disabled) { filter: brightness(0.9); }
  .nb:disabled { opacity: 0.5; cursor: default; }
  .nb.spec { background: var(--spec, #5530a8); }
  .nb.runtime { background: var(--runtime, #1a7158); }
  .nb.control { background: var(--control, #c97c2e); }
  /* The blue accent is too light for white text on its own. */
  .nb.ephemeral { background: color-mix(in srgb, var(--ephemeral, #4a8eff) 78%, #000); }
</style>
