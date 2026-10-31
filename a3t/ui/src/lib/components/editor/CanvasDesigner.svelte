<script lang="ts">
  import { getProject } from '$lib/stores/context.svelte.ts';
  // Canvas editor — a canvas is ONE flow's DEPLOYMENT metadata (not a multi-agent
  // graph). See .ai/A3T_CANVAS_SPEC.md §3. It emits the a3t.io/v1alpha1 CanvasSpec:
  //   { flowRef, placement{mode,runtimeSelector,namespace}, lifecycle{state,scaling,restart}, health{dependsOn,onDependencyDown} }
  // which the backend deserializes (flowr-types::canvas::CanvasSpec) for
  // placement/deploy. Live placement preview replicates select_runtime() so you
  // see which runtime pool this canvas resolves to before saving.
  import SlideEditor from '$lib/components/shared/SlideEditor.svelte';
  import YamlEditor from '$lib/components/editor/YamlEditor.svelte';
  import CanvasDiagram from '$lib/components/editor/CanvasDiagram.svelte';
  import DesignerChat from '$lib/components/editor/DesignerChat.svelte';
  import { canvasApi, mcpCall } from '$lib/services/api';
  import * as YAML from 'yaml';

  let { open = false, canvas = null, onclose, onsaved, embedded = false, readonly = false, onsavespec }:
    {
      open?: boolean; canvas?: any; onclose: () => void; onsaved?: () => void;
      /// Render inline (e.g. the chat side pane) instead of as a slide-in panel.
      embedded?: boolean;
      /// View only — for a shared preset, which is not edited in place. The
      /// Script and Form views still show everything; nothing can be changed.
      readonly?: boolean;
      /// Save through the caller instead of the canvas registry API. The chat
      /// pane uses this to save via `update_canvas`, the same Cedar-gated tool
      /// the agent uses.
      onsavespec?: (spec: any) => Promise<{ ok: boolean; error?: string }>;
    } = $props();

  // Single "Designer" node — the whole editor is the split-pane designer
  // (config/code on the left, form + AI on the right), mirroring the flow/agent editor.
  const SECTIONS = [
    { id: 'designer', label: 'Designer', icon: '<rect x="1" y="1" width="5" height="5" rx="1"/><rect x="10" y="10" width="5" height="5" rx="1"/><path d="M6 3.5h4M8 3.5v9"/>' },
  ];
  const ON_DEP_DOWN = ['queue', 'cordon', 'circuit-break', 'fail'];

  // ── Split-pane designer state (left: config/code · right: form/AI) ──
  let leftMode = $state<'code' | 'preview'>('code');
  let rightMode = $state<'form' | 'ai'>('form');
  let splitPercent = $state(50);
  let dragging = $state(false);
  let splitContainer: HTMLDivElement;

  function onDragStart(e: MouseEvent) {
    dragging = true; e.preventDefault();
    const onMove = (ev: MouseEvent) => {
      if (!splitContainer) return;
      const rect = splitContainer.getBoundingClientRect();
      splitPercent = Math.max(25, Math.min(75, ((ev.clientX - rect.left) / rect.width) * 100));
    };
    const onUp = () => { dragging = false; window.removeEventListener('mousemove', onMove); window.removeEventListener('mouseup', onUp); };
    window.addEventListener('mousemove', onMove);
    window.addEventListener('mouseup', onUp);
  }

  let activeSection = $state('designer');
  let name = $state('');
  // A new canvas belongs to the selected project.
  let namespace = $state(getProject() || 'default');
  let description = $state('');

  // §3 CanvasSpec, held as flat form state.
  let flowRef = $state('');                 // namespace/name of the flow this canvas deploys
  let mode = $state<'shared' | 'isolated'>('shared');
  let selectorText = $state('');            // placement.runtimeSelector as "class=gpu, tier=prod"
  let placeNamespace = $state('');          // placement.namespace (empty = flow's ns)
  let lifeState = $state<'running' | 'paused'>('running');
  let session = $state<'ephemeral' | 'persistent'>('ephemeral');
  let scMin = $state(1); let scMax = $state(1);
  let scMetric = $state(''); let scTarget = $state<number | ''>('');
  let rsPolicy = $state('on-failure'); let rsBackoff = $state('exponential'); let rsMax = $state(5);
  let onDepDown = $state('queue');
  let dependsOn = $state<string[]>([]);     // resource refs (namespace/name)
  let depInput = $state('');
  // Optional fields with no dedicated form control — held so a code-pane edit
  // survives the form round-trip and stays in the full spec.
  let tolerations = $state<any[]>([]);      // placement.tolerations
  let circuit = $state<any>(null);          // health.circuit
  let agentRef = $state('');                // set when cloned from a preset for an agent
  let pinRuntime = $state('');              // placement.runtime — a runtime chosen by name
  let shared = $state(true);                // a shared preset (true) vs someone's own copy
  // The published schema (single source of truth), fetched on open — the editor
  // scaffolds the full spec and can point authors at it for verification.
  let schema = $state<any>(null);

  let flows = $state<{ ref: string; label: string }[]>([]);
  let runtimes = $state<any[]>([]);
  let saving = $state(false);
  let msg = $state('');

  // Embedded instances are mounted per canvas (the pane keys them), so they
  // initialise once on mount; the slide-in panel initialises each time it opens.
  $effect(() => { if (open || embedded) void init(); });

  async function init() {
    msg = ''; activeSection = 'designer'; depInput = ''; leftMode = 'code'; rightMode = 'form';
    const [rt, sch] = await Promise.all([canvasApi.runtimes(), canvasApi.schema()]);
    runtimes = rt.ok ? (rt.data?.runtimes ?? []) : [];
    schema = sch.ok ? sch.data : null;
    // Flows come from the workflow store (list_flows), not the resource registry.
    const seen = new Set<string>(); const acc: { ref: string; label: string }[] = [];
    for (const ns of ['a3t', 'default', 'system']) {
      const f = await mcpCall('list_flows', { namespace: ns });
      for (const w of (f.data?.workflows ?? (Array.isArray(f.data) ? f.data : []))) {
        const ref = `${w.namespace ?? ns}/${w.name}`;
        if (!seen.has(ref)) { seen.add(ref); acc.push({ ref, label: ref }); }
      }
    }
    flows = acc;

    if (canvas) {
      name = canvas.name; namespace = canvas.namespace ?? 'default';
      applySpecToForm(canvas.config ?? {});
    } else {
      name = ''; namespace = 'default';
      applySpecToForm({});
    }
  }

  function selectorToText(sel?: Record<string, string>): string {
    return sel ? Object.entries(sel).map(([k, v]) => `${k}=${v}`).join(', ') : '';
  }
  function textToSelector(t: string): Record<string, string> {
    const out: Record<string, string> = {};
    for (const p of t.split(',').map((s) => s.trim()).filter(Boolean)) {
      const [k, v] = p.split('=').map((s) => s.trim());
      if (k && v) out[k] = v;
    }
    return out;
  }

  function addDep() {
    const r = depInput.trim();
    if (r && r.includes('/') && !dependsOn.includes(r)) { dependsOn = [...dependsOn, r]; depInput = ''; }
  }
  function removeDep(r: string) { dependsOn = dependsOn.filter((d) => d !== r); }

  // The §3 CanvasSpec this form produces (stored as the canvas resource config).
  const spec = $derived(buildSpec());
  // The COMPLETE CanvasSpec — every field present (defaults materialized), with
  // `null`/`[]` where an optional field is not configured. This full skeleton is
  // what the code pane shows and what `save` persists, so each stored canvas is
  // self-documenting and Cedar sees every attribute. Mirrors flowr-types::CanvasSpec.
  function buildSpec() {
    return {
      flowRef,
      description,
      agentRef,
      // Carried through so saving from this editor never silently turns a
      // user's copy back into a protected shared preset, or vice versa.
      shared,
      placement: {
        mode,
        runtimeSelector: textToSelector(selectorText),
        namespace: placeNamespace,
        tolerations,
        // The pin must round-trip: dropping it here would erase a runtime the
        // user picked the moment they saved anything else.
        runtime: pinRuntime || null,
      },
      lifecycle: {
        state: lifeState,
        session,
        scaling: {
          min: Number(scMin),
          max: Number(scMax),
          metric: scMetric || null,
          target: scTarget !== '' ? Number(scTarget) : null,
        },
        restart: { policy: rsPolicy, backoff: rsBackoff, maxRetries: Number(rsMax) },
      },
      health: {
        dependsOn: dependsOn.map((resource) => ({ resource })),
        onDependencyDown: onDepDown,
        circuit,
      },
    };
  }

  // ── Two-way sync: the config/code editor (left) ⇄ the form (right) ──
  // The form is the source of truth. `yamlSource` is regenerated from the form,
  // except when the current code already parses to the same spec — that guard
  // means typing in the code editor updates the form WITHOUT reformatting the
  // text under the cursor (the classic FlowEditorPane round-trip).
  let yamlSource = $state('');
  $effect(() => {
    const s = spec; // track form changes
    let cur: any = null;
    try { cur = YAML.parse(yamlSource); } catch { cur = undefined; }
    if (JSON.stringify(cur) === JSON.stringify(s)) return; // code already matches the form
    yamlSource = renderYaml(s);
  });

  // Render the full spec as a self-documenting YAML doc: a header pointing at the
  // published schema + short comments on each section. Comments don't affect the
  // round-trip (parse strips them), so the guard above still holds.
  function renderYaml(s: any): string {
    const doc = new YAML.Document(s);
    doc.commentBefore =
      ` a3t.io/v1alpha1 · CanvasSpec — full spec (unset optional fields shown as null/[]).\n` +
      ` Schema: GET /canvas/schema · every key is present so you can see what's configurable.`;
    const comment = (key: string, text: string) => {
      const node: any = doc.get(key, true);
      if (node) node.commentBefore = ' ' + text;
    };
    comment('placement', 'where the flow loads: shared pool or its own isolated sandbox');
    comment('lifecycle', 'how it runs: state (running|paused=hibernate), session (ephemeral|persistent), scaling, restart');
    comment('health', 'dependent resources to watch + what to do when one is down');
    return String(doc);
  }

  function onCodeChange(val: string) {
    yamlSource = val;
    let d: any;
    try { d = YAML.parse(val); } catch { return; } // hold last good form on invalid YAML
    if (d && typeof d === 'object') applySpecToForm(d);
  }

  // Populate the form state from a CanvasSpec object (code → form, and on load).
  function applySpecToForm(c: any) {
    flowRef = c.flowRef ?? '';
    description = c.description ?? '';
    agentRef = c.agentRef ?? '';
    const p = c.placement ?? {}, l = c.lifecycle ?? {}, h = c.health ?? {};
    mode = p.mode ?? 'shared';
    selectorText = selectorToText(p.runtimeSelector);
    placeNamespace = p.namespace ?? '';
    lifeState = l.state ?? 'running';
    session = l.session ?? 'ephemeral';
    const sc = l.scaling ?? {}; scMin = sc.min ?? 1; scMax = sc.max ?? 1;
    scMetric = sc.metric ?? ''; scTarget = sc.target ?? '';
    const rs = l.restart ?? {}; rsPolicy = rs.policy ?? 'on-failure'; rsBackoff = rs.backoff ?? 'exponential'; rsMax = rs.maxRetries ?? 5;
    onDepDown = h.onDependencyDown ?? 'queue';
    dependsOn = Array.isArray(h.dependsOn) ? h.dependsOn.map((d: any) => d.resource).filter(Boolean) : [];
    tolerations = Array.isArray(p.tolerations) ? p.tolerations : [];
    pinRuntime = p.runtime ?? '';
    shared = c.shared ?? true;
    circuit = h.circuit ?? null;
  }

  // Client-side mirror of flowr-types::canvas::select_runtime for a live preview.
  const placedRuntime = $derived.by(() => {
    const sel = textToSelector(selectorText);
    const ns = placeNamespace.trim();
    const nsOk = (r: any) => !ns || r.namespace === ns;
    const pool = runtimes.filter(nsOk);
    const labelsOk = (r: any) => Object.entries(sel).every(([k, v]) => (r.labels ?? {})[k] === v);
    // A pin names the pool, but must still satisfy the selector — same rule as
    // the server, so the preview never promises a placement deploy would refuse.
    if (pinRuntime) return pool.find((r) => r.name === pinRuntime && labelsOk(r)) ?? null;
    if (Object.keys(sel).length === 0) return pool.find((r) => r.default) ?? pool[0] ?? null;
    return pool.find((r) => Object.entries(sel).every(([k, v]) => (r.labels ?? {})[k] === v)) ?? null;
  });

  // Local validation mirroring CanvasSpec::validate.
  const problems = $derived.by(() => {
    const e: string[] = [];
    // A clone may be saved before it is bound to a flow, so the flow is only
    // required on the Canvas page, where every canvas is created for a flow.
    if (!flowRef && !embedded) e.push('Flow is required');
    else if (flowRef && !flowRef.includes('/')) e.push("Flow must be 'namespace/name'");
    if (Number(scMax) < Number(scMin)) e.push('scaling max must be ≥ min');
    for (const d of dependsOn) if (!d.includes('/')) e.push(`dependsOn '${d}' must be 'namespace/name'`);
    return e;
  });

  async function save() {
    if (!name.trim()) { msg = 'Name is required'; return; }
    if (problems.length) { msg = problems[0]; return; }
    saving = true; msg = '';
    const body = buildSpec();
    if (onsavespec) {
      const r = await onsavespec(body);
      saving = false;
      msg = r.ok ? 'Saved' : (r.error || 'Save failed');
      if (r.ok) onsaved?.();
      return;
    }
    const res = canvas ? await canvasApi.update(namespace, name.trim(), body) : await canvasApi.create(namespace, name.trim(), body);
    saving = false;
    if (res.ok) { onsaved?.(); onclose(); } else msg = res.error || `Save failed (HTTP ${res.status})`;
  }
</script>

{#snippet body()}
  {#if msg}<div class="cd-error">{msg}</div>{/if}

  <div class="cd-split" class:dragging bind:this={splitContainer} style="grid-template-columns: {splitPercent}% 6px 1fr;">
    <!-- LEFT: config (code) or placement preview -->
    <div class="cd-pane">
      <div class="cd-toolbar">
        <div class="cd-tabs">
          <button class="cd-tab" class:active={leftMode === 'code'} onclick={() => (leftMode = 'code')}>
            <svg width="13" height="13" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5"><path d="M5 3L1 8l4 5M11 3l4 5-4 5"/></svg>
            Config
          </button>
          <button class="cd-tab" class:active={leftMode === 'preview'} onclick={() => (leftMode = 'preview')}>
            <svg width="13" height="13" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5"><rect x="1" y="1" width="5" height="5" rx="1"/><rect x="10" y="10" width="5" height="5" rx="1"/><path d="M6 3.5h4M8 3.5v9"/></svg>
            Preview
          </button>
        </div>
        <span class="cd-crd">a3t.io/v1alpha1 · Canvas</span>
      </div>
      <div class="cd-pane-body">
        {#if leftMode === 'code'}
          <YamlEditor value={yamlSource} onchange={onCodeChange} readOnly={readonly} />
        {:else}
          <!-- The canvas as a logical system: triggers → runtime → container →
               flow/agent → what it uses, plus where else it could (not) go. -->
          <CanvasDiagram
            {flowRef} {agentRef} {mode} {placeNamespace} {pinRuntime} {runtimes}
            selector={textToSelector(selectorText)} {lifeState} {session}
            scMin={Number(scMin)} scMax={Number(scMax)} {dependsOn} {problems} />
        {/if}
      </div>
    </div>

    <!-- Drag handle -->
    <div class="cd-divider" onmousedown={onDragStart} role="separator" aria-orientation="vertical"></div>

    <!-- RIGHT: config form or AI chat -->
    <div class="cd-pane">
      <div class="cd-toolbar">
        <div class="cd-tabs">
          <button class="cd-tab" class:active={rightMode === 'form'} onclick={() => (rightMode = 'form')}>
            <svg width="13" height="13" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5"><rect x="2" y="2" width="12" height="12" rx="1.5"/><path d="M5 6h6M5 9h4"/></svg>
            Form
          </button>
          {#if !embedded}
          <button class="cd-tab" class:active={rightMode === 'ai'} onclick={() => (rightMode = 'ai')}>
            <svg width="13" height="13" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5"><path d="M2 3h12a1 1 0 011 1v7a1 1 0 01-1 1H6l-3 2v-2H2a1 1 0 01-1-1V4a1 1 0 011-1z"/></svg>
            AI
          </button>
          {/if}
        </div>
      </div>
      <div class="cd-pane-body">
        {#if rightMode === 'form'}
          <fieldset class="cd-fieldset" disabled={readonly}>
          <div class="cd-formcol">
            <div class="cd-sub">Canvas — the flow this deploys</div>
            <label class="cf-field"><span>Name</span><input bind:value={name} placeholder="orders-canvas" readonly={!!canvas} /></label>
            <label class="cf-field"><span>Namespace</span><input bind:value={namespace} readonly={!!canvas} /></label>
            <label class="cf-field"><span>Flow (flowRef)</span>
              <select bind:value={flowRef}>
                <option value="">— select the flow this canvas deploys —</option>
                {#if flowRef && !flows.some((f) => f.ref === flowRef)}<option value={flowRef}>{flowRef}</option>{/if}
                {#each flows as f}<option value={f.ref}>{f.label}</option>{/each}
              </select>
            </label>
            <label class="cf-field"><span>Description</span><input bind:value={description} placeholder="what this canvas deploys" /></label>

            <div class="cd-sub">Placement — where the flow loads</div>
            <label class="cf-field"><span>Isolation mode</span>
              <select bind:value={mode}><option value="shared">shared — into a runtime pool</option><option value="isolated">isolated — dedicated sandbox</option></select>
            </label>
            <label class="cf-field"><span>Runtime selector (labels)</span><input bind:value={selectorText} placeholder="class=gpu, tier=prod" /></label>
            <label class="cf-field"><span>Namespace override</span><input bind:value={placeNamespace} placeholder="(flow's namespace)" /></label>
            <label class="cf-field"><span>Pinned runtime</span>
              <select bind:value={pinRuntime}>
                <option value="">— none: select by labels —</option>
                {#if pinRuntime && !runtimes.some((r) => r.name === pinRuntime)}<option value={pinRuntime}>{pinRuntime} (not registered)</option>{/if}
                {#each runtimes as r}<option value={r.name}>{r.name}</option>{/each}
              </select>
            </label>

            <div class="cd-sub">Lifecycle — how it runs</div>
            <div class="cf-grid">
              <label class="cf-field"><span>State</span>
                <select bind:value={lifeState}><option value="running">running</option><option value="paused">paused (hibernate)</option></select>
              </label>
              <label class="cf-field"><span>Session</span>
                <select bind:value={session}>
                  <option value="ephemeral">ephemeral — no carried state</option>
                  <option value="persistent">persistent — checkpoint &amp; resume</option>
                </select>
              </label>
            </div>
            <div class="cf-grid">
              <label class="cf-field"><span>Scale min</span><input type="number" min="0" bind:value={scMin} /></label>
              <label class="cf-field"><span>Scale max</span><input type="number" min="0" bind:value={scMax} /></label>
            </div>
            <div class="cf-grid">
              <label class="cf-field"><span>Scale metric</span><input bind:value={scMetric} placeholder="queue_depth" /></label>
              <label class="cf-field"><span>Target</span><input type="number" min="0" bind:value={scTarget} placeholder="20" /></label>
            </div>
            <div class="cf-grid3">
              <label class="cf-field"><span>Restart</span>
                <select bind:value={rsPolicy}><option value="on-failure">on-failure</option><option value="always">always</option><option value="never">never</option></select>
              </label>
              <label class="cf-field"><span>Backoff</span>
                <select bind:value={rsBackoff}><option value="exponential">exponential</option><option value="linear">linear</option><option value="fixed">fixed</option></select>
              </label>
              <label class="cf-field"><span>Max retries</span><input type="number" min="0" bind:value={rsMax} /></label>
            </div>

            <div class="cd-sub">Health — dependent resources to watch</div>
            <p class="sf-note">Auto-derived from the flow's tasks (call:llm/mcp/adb) when empty. Override here.</p>
            <div class="dep-add">
              <input bind:value={depInput} placeholder="namespace/resource-name" onkeydown={(e) => { if (e.key === 'Enter') { e.preventDefault(); addDep(); } }} />
              <button class="btn sm" onclick={addDep}>Add</button>
            </div>
            {#each dependsOn as d}
              <div class="dep-row"><span class="mono">{d}</span><button class="dep-x" onclick={() => removeDep(d)} aria-label="Remove">✕</button></div>
            {/each}
            <label class="cf-field" style="margin-top:10px;"><span>On dependency down</span>
              <select bind:value={onDepDown}>{#each ON_DEP_DOWN as o}<option value={o}>{o}</option>{/each}</select>
            </label>
          </div>
          </fieldset>
        {:else}
          <!-- A chat agent working on this canvas: changes land in the editor
               (Config and Form update together) and are saved with Save. -->
          <DesignerChat kind="canvas" name={name ? `${namespace}/${name}` : ''} {readonly}
            gettext={() => yamlSource}
            onapply={(t) => onCodeChange(t)} />
        {/if}
      </div>
    </div>
  </div>
{/snippet}

{#if embedded}
  <!-- Inline in the chat pane: same Script and Form views, no slide-in chrome. -->
  <div class="cd-embedded">
    <div class="cd-embar">
      <span class="cd-embar-note">{readonly ? "Shared preset — view only. Clone it to make your own." : (flowRef ? `Deploys ${flowRef}` : "Your copy — not bound to a flow yet")}</span>
      {#if !readonly && onsavespec}
        <button class="btn sm primary" onclick={save} disabled={saving}>{saving ? "Saving…" : "Save"}</button>
      {/if}
    </div>
    {@render body()}
  </div>
{:else}
<SlideEditor
  {open}
  icon="CV"
  color="var(--control-ink, #3a6ea5)"
  name={name || 'New Canvas'}
  meta={canvas ? namespace : 'a3t.io/v1alpha1 · Canvas'}
  sections={SECTIONS}
  active={activeSection}
  onselect={(id) => (activeSection = id)}
  onsave={save}
  {saving}
  saveLabel={canvas ? 'Save canvas' : 'Create canvas'}
  {onclose}
>
  {@render body()}
</SlideEditor>
{/if}

<style>
  .cd-embedded { display: flex; flex-direction: column; height: 100%; min-height: 0; }
  .cd-embedded .cd-split { flex: 1; min-height: 0; }
  .cd-embar { display: flex; align-items: center; justify-content: space-between; gap: 8px; padding: 6px 10px; border-bottom: 1px solid var(--border); }
  .cd-embar-note { font-size: 11.5px; color: var(--text-3); }
  .cd-fieldset { border: 0; margin: 0; padding: 0; min-width: 0; }
  .cd-fieldset:disabled { opacity: 0.85; }

  .cd-error { position: absolute; top: 48px; right: 16px; z-index: 30; background: var(--warn-bg, #fde8e8); color: var(--warn, #d66); border: 1px solid var(--warn, #d66); border-radius: 6px; padding: 6px 12px; font-size: 12px; }
  .sf-note { font-size: 11.5px; color: var(--text-4); margin: 4px 0 8px; }
  .cf-field { display: flex; flex-direction: column; gap: 5px; margin-bottom: 12px; font-size: 12px; color: var(--text-3); }
  .cf-field input, .cf-field select { padding: 8px 10px; border: 1px solid var(--border); border-radius: 6px; background: var(--bg-sunken, var(--bg-surface)); color: var(--text-1); font-size: 13px; width: 100%; }
  .cf-field input[readonly] { opacity: 0.7; }
  .cf-grid { display: grid; grid-template-columns: 1fr 1fr; gap: 10px; }
  .cf-grid3 { display: grid; grid-template-columns: 1fr 1fr 1fr; gap: 10px; }

  /* Split-pane designer — left config/code · right form/AI (matches the flow/agent editor) */
  .cd-split { display: grid; height: 100%; overflow: hidden; }
  .cd-split.dragging { user-select: none; cursor: col-resize; }
  .cd-pane { display: flex; flex-direction: column; overflow: hidden; min-width: 0; }
  .cd-toolbar { display: flex; align-items: center; gap: 8px; padding: 5px 10px; border-bottom: 1px solid var(--border); background: var(--bg-surface); flex-shrink: 0; }
  .cd-tabs { display: flex; gap: 2px; background: var(--bg-subtle); border-radius: var(--r-sm, 6px); padding: 2px; }
  .cd-tab { display: flex; align-items: center; gap: 4px; padding: 4px 10px; border: none; border-radius: 4px; background: none; color: var(--text-3); font-size: 11px; cursor: pointer; transition: all 0.12s; }
  .cd-tab.active { background: var(--bg-surface); color: var(--text-1); box-shadow: var(--shadow-sm); }
  .cd-tab:hover:not(.active) { color: var(--text-2); }
  .cd-crd { margin-left: auto; font-family: var(--font-mono, monospace); font-size: 10px; color: var(--text-4); }
  .cd-pane-body { flex: 1; overflow: hidden; min-height: 0; }
  .cd-pane-body :global(.cm-editor) { height: 100%; }

  .cd-divider { width: 6px; cursor: col-resize; background: var(--border); position: relative; transition: background 0.15s; }
  .cd-divider:hover, .cd-split.dragging .cd-divider { background: var(--control-ink, #3a6ea5); }
  .cd-divider::after { content: ''; position: absolute; top: 50%; left: 50%; transform: translate(-50%, -50%); width: 2px; height: 24px; border-radius: 1px; background: var(--text-5); transition: background 0.15s; }
  .cd-divider:hover::after, .cd-split.dragging .cd-divider::after { background: white; }

  .cd-formcol { padding: 18px 22px; overflow-y: auto; height: 100%; }
  .cd-preview { padding: 18px 20px; overflow-y: auto; height: 100%; display: flex; flex-direction: column; gap: 12px; }
  .cd-sub { font-size: 10px; text-transform: uppercase; letter-spacing: 0.05em; color: var(--text-4); margin: 12px 0 8px; }
  .cd-formcol .cd-sub:first-child, .cd-preview .cd-sub:first-child { margin-top: 0; }

  /* AI chat pane */
  .cd-ai { display: flex; flex-direction: column; height: 100%; }
  .cd-ai-messages { flex: 1; overflow-y: auto; padding: 16px; }
  .cd-ai-msg { display: flex; gap: 10px; margin-bottom: 12px; }
  .cd-ai-mark { width: 28px; height: 28px; border-radius: 6px; background: var(--bg-subtle); display: grid; place-items: center; font-family: var(--font-serif); font-size: 10px; font-weight: 600; color: var(--text-3); flex-shrink: 0; }
  .cd-ai-msg p { font-size: 13px; color: var(--text-2); line-height: 1.5; margin: 0; padding-top: 4px; }
  .cd-ai-input { padding: 10px 12px; border-top: 1px solid var(--border); background: var(--bg-surface); }
  .cd-ai-input input { width: 100%; padding: 8px 12px; border: 1px solid var(--border); border-radius: 8px; background: var(--bg-page); font-size: 12px; color: var(--text-1); outline: none; }
  .cd-ai-input input:disabled { opacity: 0.5; }

  .dep-add { display: flex; gap: 6px; margin-bottom: 8px; }
  .dep-add input { flex: 1; padding: 7px 10px; border: 1px solid var(--border); border-radius: 6px; background: var(--bg-sunken, var(--bg-surface)); color: var(--text-1); font-size: 12.5px; }
  .btn.sm { padding: 6px 12px; border: 1px solid var(--border); border-radius: 6px; background: var(--bg-surface); color: var(--text-2); font-size: 12px; cursor: pointer; }
  .dep-row { display: flex; align-items: center; justify-content: space-between; padding: 6px 10px; border: 1px solid var(--border); border-radius: 6px; margin-bottom: 4px; font-size: 12px; }
  .dep-x { background: none; border: none; color: var(--text-4); cursor: pointer; }

  .place-card { border: 1px solid var(--border); border-radius: 8px; padding: 12px; }
  .place-card.matched { border-color: rgba(60,180,110,0.4); background: rgba(60,180,110,0.06); }
  .place-card.nomatch { border-color: rgba(210,160,60,0.4); background: rgba(210,160,60,0.06); }
  .place-line { display: flex; justify-content: space-between; gap: 10px; font-size: 12px; margin-bottom: 4px; }
  .pl-k { color: var(--text-4); }
  .pl-v { color: var(--text-1); }
  .pl-v.ok { color: #2fa46b; font-weight: 600; }
  .pl-v.warn { color: var(--warn, #d66); font-weight: 600; }
  .prob-box { margin-top: 10px; }
  .prob { font-size: 11.5px; color: var(--warn, #d66); margin-bottom: 3px; }
  .mono { font-family: var(--font-mono, monospace); }
</style>
