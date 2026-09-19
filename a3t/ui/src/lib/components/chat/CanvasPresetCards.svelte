<script lang="ts">
  /// Deployment presets, rendered for the user to DECIDE on.
  ///
  /// Choosing how something runs in production is the user's call, and a preset
  /// name is not enough to make it — "prod-ha" says nothing about whether it takes
  /// a dedicated runtime or how far it scales. Each card shows exactly what is
  /// weighed when choosing.
  ///
  /// The buttons do not act directly. They send the choice back to the agent as a
  /// message, so reuse and clone go through the same tools, the same validation
  /// and the same policy gates as anything typed — a card is a shortcut for
  /// saying it, not a side door.
  import { mcpCall } from '$lib/services/api';
  import { toEditorText } from './artifacts';

  let { data, onchoose, onview }: {
    data: { presets: any[]; cloned?: boolean };
    onchoose?: (text: string) => void;
    /// Open the canvas in the side pane (Script + Form). Deciding to reuse or
    /// clone needs the whole definition, not a card's summary.
    onview?: (a: { kind: 'canvas'; namespace: string; name: string; text: string }) => void;
  } = $props();

  let viewing = $state('');
  let viewError = $state('');

  async function view(p: any) {
    viewing = `${p.namespace}/${p.name}`; viewError = '';
    const res = await mcpCall('get_canvas', { namespace: p.namespace, name: p.name });
    viewing = '';
    if (!res.ok) { viewError = res.error ?? `Could not open ${p.name}`; return; }
    onview?.({ kind: 'canvas', namespace: p.namespace, name: p.name, text: toEditorText(res.data, 'json') });
  }

  const presets = $derived(Array.isArray(data?.presets) ? data.presets : []);

  function scaleText(s: any): string {
    if (!s) return '1 replica';
    const { min = 1, max = 1, metric } = s;
    const range = min === max ? `${min} replica${min === 1 ? '' : 's'}` : `${min}–${max} replicas`;
    return metric ? `${range} on ${metric}` : range;
  }

  function selectorText(sel: any): string {
    const entries = sel && typeof sel === 'object' ? Object.entries(sel) : [];
    return entries.length ? entries.map(([k, v]) => `${k}=${v}`).join(', ') : 'any pool';
  }

  /// Placement that needs Admin under the baseline policy. Shown up front so the
  /// approval prompt later is not a surprise.
  function needsApproval(p: any): boolean {
    return p?.mode === 'isolated' || p?.selector?.tier === 'prod';
  }
</script>

<div class="cpc">
  {#if viewError}<div class="cpc-banner err">{viewError}</div>{/if}
  {#if data?.cloned}
    <div class="cpc-banner">New canvas created from a preset — not deployed yet.</div>
  {/if}
  {#each presets as p (`${p.namespace}/${p.name}`)}
    <div class="cpc-card">
      <div class="cpc-head">
        <span class="cpc-name">{p.name}</span>
        <span class="cpc-mode" class:iso={p.mode === 'isolated'}>
          {p.mode === 'isolated' ? 'dedicated runtime' : 'shared pool'}
        </span>
        {#if p.shared === false}
          <!-- An unbound clone is listed alongside presets because it deploys
               nothing yet; say whose it is, so it is not mistaken for a shared one. -->
          <span class="cpc-own" title="Your own copy — editable, not bound to a flow yet">your copy</span>
        {/if}
        {#if needsApproval(p)}<span class="cpc-flag" title="Deploying with this needs Admin approval">needs approval</span>{/if}
      </div>
      {#if p.description}<p class="cpc-desc">{p.description}</p>{/if}

      <dl class="cpc-facts">
        <dt>Runs on</dt><dd>{selectorText(p.selector)}{p.targetNamespace ? ` · ns ${p.targetNamespace}` : ''}</dd>
        <dt>Scale</dt><dd>{scaleText(p.scaling)}</dd>
        <dt>On failure</dt><dd>{p.restart?.policy ?? 'on-failure'}{p.restart?.maxRetries ? `, up to ${p.restart.maxRetries} retries` : ''}</dd>
        <dt>State</dt><dd>{p.session === 'persistent' ? 'kept between triggers' : 'fresh each trigger'}</dd>
      </dl>

      {#if onchoose && !data?.cloned}
        <div class="cpc-actions">
          <!-- View first: reuse-or-clone is decided on the full definition. -->
          <button class="cpc-btn primary" onclick={() => view(p)} disabled={viewing === `${p.namespace}/${p.name}`}>
            {viewing === `${p.namespace}/${p.name}` ? 'Opening…' : 'View'}
          </button>
          <button class="cpc-btn" onclick={() => onchoose?.(`Use the ${p.namespace}/${p.name} preset as it is.`)}>
            Use this
          </button>
          <button class="cpc-btn" onclick={() => onchoose?.(`Clone the ${p.namespace}/${p.name} preset so I can change it.`)}>
            Clone and edit
          </button>
        </div>
      {/if}
    </div>
  {/each}
</div>

<style>
  .cpc { display: flex; flex-direction: column; gap: 8px; margin: 10px 0; max-width: 680px; }
  .cpc-banner { font-size: 12px; color: var(--text-3); padding: 2px 2px 0; }
  .cpc-banner.err { color: #b91c1c; }
  .cpc-btn:disabled { opacity: 0.6; cursor: default; }
  .cpc-card {
    border: 1px solid var(--border); border-radius: var(--r, 10px);
    background: var(--bg-surface); padding: 12px 14px;
  }
  .cpc-head { display: flex; align-items: center; gap: 8px; flex-wrap: wrap; }
  .cpc-name { font-family: var(--font-mono, ui-monospace, monospace); font-size: 13px; font-weight: 600; color: var(--text-1); }
  .cpc-mode {
    font-size: 10.5px; padding: 1px 7px; border-radius: 999px;
    background: var(--bg-subtle); color: var(--text-3); border: 1px solid var(--border);
  }
  .cpc-mode.iso { color: var(--spec, #5530a8); border-color: var(--spec-bg, #ede7ff); background: var(--spec-bg, #ede7ff); }
  .cpc-own {
    font-size: 10.5px; padding: 1px 7px; border-radius: 999px;
    color: var(--runtime, #1a7158); background: var(--runtime-bg, #e3f4ef);
  }
  .cpc-flag {
    font-size: 10.5px; padding: 1px 7px; border-radius: 999px;
    color: var(--control, #c97c2e); background: var(--control-bg, #fbf0e0);
  }
  .cpc-desc { margin: 6px 0 8px; font-size: 12.5px; color: var(--text-2); line-height: 1.45; }
  .cpc-facts {
    display: grid; grid-template-columns: max-content 1fr; gap: 3px 12px;
    margin: 0; font-size: 12px;
  }
  .cpc-facts dt { color: var(--text-4); }
  .cpc-facts dd { margin: 0; color: var(--text-1); }
  .cpc-actions { display: flex; gap: 7px; margin-top: 10px; flex-wrap: wrap; }
  .cpc-btn {
    padding: 6px 12px; font-size: 12px; border-radius: var(--r-sm, 6px);
    border: 1px solid var(--border-strong); background: var(--bg-page); color: var(--text-1); cursor: pointer;
  }
  .cpc-btn:hover { border-color: var(--spec, #5530a8); }
  .cpc-btn.primary { background: var(--text-1); color: var(--bg-page); border-color: var(--text-1); }
  .cpc-btn.primary:hover { opacity: 0.9; }
</style>
