<script lang="ts">
  // A registry-backed dropdown — no loose strings. Renders a <select> populated
  // from `options`, with an optional "+ New…" entry that fires `onnew`.
  let {
    value = '',
    options = [],
    placeholder = '— select —',
    allowNew = false,
    newLabel = '+ New…',
    disabled = false,
    onchange,
    onnew,
  }: {
    value?: string;
    options?: { value: string; label: string }[];
    placeholder?: string;
    allowNew?: boolean;
    newLabel?: string;
    disabled?: boolean;
    onchange?: (value: string) => void;
    onnew?: () => void;
  } = $props();

  function handle(e: Event) {
    const v = (e.target as HTMLSelectElement).value;
    if (v === '__new__') {
      (e.target as HTMLSelectElement).value = value;
      onnew?.();
      return;
    }
    onchange?.(v);
  }

  // Show the current value even if it isn't in the options (e.g. legacy/custom).
  let missing = $derived(value && !options.some(o => o.value === value));
</script>

<select class="res-select" {value} {disabled} onchange={handle}>
  <option value="">{placeholder}</option>
  {#if missing}<option value={value}>{value} (unregistered)</option>{/if}
  {#each options as o}<option value={o.value}>{o.label}</option>{/each}
  {#if allowNew}<option value="__new__">{newLabel}</option>{/if}
</select>

<style>
  .res-select {
    width: 100%;
    font-size: 13px;
    padding: 7px 9px;
    border: 1px solid var(--border);
    border-radius: var(--r-sm);
    background: var(--bg-surface);
    color: var(--text-1);
    font-family: var(--font-mono);
    cursor: pointer;
  }
  .res-select:disabled { opacity: 0.7; cursor: default; }
  .res-select:focus { outline: none; border-color: var(--spec); }
</style>
