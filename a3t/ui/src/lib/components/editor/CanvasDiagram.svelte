<script lang="ts">
  /// The canvas as a logical system diagram: what feeds the flow (triggers), where
  /// it lands (runtime → container → flow/agent), what it uses (resources and the
  /// kinds of calls its definition makes), and which other runtimes would or would
  /// not take it, with the reason. When the canvas is deployed, the live state is
  /// shown on it with a link to the Runtime console.
  ///
  /// Placement is recomputed here from the form state (the same rule as the
  /// server's select_runtime), so the diagram follows edits before they are saved.
  import { mcpCall, opsApi } from '$lib/services/api';
  import { navigateTo } from '$lib/stores/panels.svelte.ts';

  let {
    flowRef = '', agentRef = '', mode = 'shared', placeNamespace = '', selector = {},
    pinRuntime = '', runtimes = [], lifeState = 'running', session = 'ephemeral',
    scMin = 1, scMax = 1, dependsOn = [], problems = [], controller = false,
  }: {
    flowRef?: string; agentRef?: string; mode?: string; placeNamespace?: string;
    selector?: Record<string, string>; pinRuntime?: string; runtimes?: any[];
    lifeState?: string; session?: string; scMin?: number; scMax?: number;
    dependsOn?: string[]; problems?: string[]; controller?: boolean;
  } = $props();

  // ── Placement, with a reason for every runtime that does not match ──
  type Verdict = { rt: any; ok: boolean; why: string };
  const verdicts = $derived.by<Verdict[]>(() => {
    const ns = placeNamespace.trim();
    return runtimes.map((r) => {
      if (ns && r.namespace !== ns) return { rt: r, ok: false, why: `namespace ${r.namespace} ≠ ${ns}` };
      for (const [k, v] of Object.entries(selector)) {
        const have = (r.labels ?? {})[k];
        if (have !== v) return { rt: r, ok: false, why: have ? `${k}=${have}, needs ${v}` : `no ${k} label` };
      }
      return { rt: r, ok: true, why: '' };
    });
  });
  const placed = $derived.by(() => {
    const ok = verdicts.filter((v) => v.ok).map((v) => v.rt);
    if (pinRuntime) return ok.find((r) => r.name === pinRuntime) ?? null;
    if (Object.keys(selector).length === 0) return ok.find((r) => r.default) ?? ok[0] ?? null;
    return ok[0] ?? null;
  });
  const effNs = $derived(placeNamespace.trim() || placed?.namespace || '—');
  const agentId = $derived(agentRef ? agentRef.split('/').pop() : '');

  // ── What feeds it, what it uses, whether it is live ──
  let triggers = $state<any[]>([]);
  let calls = $state<string[]>([]);
  let live = $state<any>(null);

  $effect(() => {
    const ref = flowRef;
    const agent = agentId;
    if (!ref) { triggers = []; calls = []; live = null; return; }
    const [ns, name] = ref.split('/');
    mcpCall('list_triggers', {}).then((res) => {
      let d: any = res.ok ? res.data : [];
      if (typeof d === 'string') { try { d = JSON.parse(d); } catch { d = []; } }
      const all: any[] = Array.isArray(d) ? d : (d?.triggers ?? []);
      triggers = all.filter((t) =>
        (t.targets ?? []).some((tg: any) =>
          (tg.type === 'workflow' && `${tg.namespace}/${tg.name}` === ref) ||
          (agent && tg.type === 'agent' && tg.agent_id === agent)) ||
        (agent && String(t.name ?? '').startsWith(agent)));
    });
    mcpCall('get_flow', { namespace: ns, name }).then((res) => {
      const text = typeof res.data === 'string' ? res.data : JSON.stringify(res.data ?? '');
      const found = new Set<string>();
      for (const m of text.matchAll(/call:\s*["']?([\w:.\-]+)/g)) found.add(m[1]);
      calls = [...found];
    });
    opsApi.overview().then((res) => {
      if (!res.ok) { live = null; return; }
      const rows = [...(res.data.runtimes ?? []).flatMap((r: any) => r.deployments), ...(res.data.unplaced ?? [])];
      live = rows.find((row: any) =>
        agent ? row.deployment.target === agent : row.deployment.flowRef === ref) ?? null;
    });
  });

  const SERVING: Record<string, { label: string; cls: string }> = {
    serving: { label: 'Serving', cls: 'ok' },
    paused: { label: 'Paused', cls: 'bad' },
    'runtime-paused': { label: 'Runtime paused', cls: 'bad' },
    hibernated: { label: 'Hibernated', cls: 'idle' },
    unplaced: { label: 'Unplaced', cls: 'warn' },
  };

  function trigLabel(t: any) {
    const c = t.config ?? {};
    if (c.type === 'webhook') return `${c.method ?? 'POST'} /webhook${c.path ?? ''}`;
    if (c.type === 'cron') return `cron ${c.expression ?? ''}`;
    return `${c.type ?? 'event'} ${c.source ?? ''}`;
  }
  // The container this lands in, named for what actually exists: in CE the
  // runtime is logical and the work runs inside the flowr pod.
  const container = $derived(
    !controller
      ? { name: 'flowr pod', note: 'CE: logical placement — runs in the shared flowr process' }
      : mode === 'isolated'
        ? { name: `sandbox · ${flowRef.split('/').pop() || 'flow'}`, note: 'dedicated sandbox per deployment' }
        : { name: `rt-${placed?.name ?? '?'}`, note: 'shared runtime pool (Deployment + HPA)' });
</script>

<div class="dg">
  {#if live}
    {@const sv = SERVING[live.serving] ?? SERVING.serving}
    <div class="live">
      <span class="dot {sv.cls}"></span>
      Deployed on <b>{live.deployment.runtime}</b> · <span class="chip {sv.cls}">{sv.label}</span>
      {#if live.deployment.runtime !== placed?.name && placed}
        <span class="warn-t">· the canvas now resolves to {placed.name} — restart to move it</span>
      {/if}
      <button class="lnk" onclick={() => navigateTo('runtime', 'Runtimes')}>Open in Runtimes →</button>
    </div>
  {:else if flowRef}
    <div class="live muted"><span class="dot idle"></span>Not deployed yet</div>
  {/if}

  <div class="graph">
    <!-- Inputs -->
    <div class="col">
      <div class="col-h">Triggers</div>
      {#each triggers as t}
        <div class="node trig" class:off={!t.enabled}>
          <span class="mono">{trigLabel(t)}</span>
          <span class="sub">{t.name}{t.enabled ? '' : ' · disabled'}</span>
        </div>
      {:else}
        <div class="node ghost">no triggers<span class="sub">runs on demand / chat</span></div>
      {/each}
    </div>

    <div class="wire" aria-hidden="true"></div>

    <!-- Runtime → container → flow -->
    <div class="col mid">
      {#if placed}
        <div class="rt-box">
          <div class="rt-h">
            <span class="k">runtime</span> <b>{placed.name}</b>
            {#if placed.state && placed.state !== 'active'}<span class="chip {placed.state === 'paused' ? 'bad' : 'idle'}">{placed.state}</span>{/if}
          </div>
          <div class="rt-meta">
            <span class="ns">ns {effNs}</span>
            {#each Object.entries(placed.labels ?? {}) as [k, v]}<span class="lbl">{k}={v}</span>{/each}
            {#if pinRuntime}<span class="lbl pin">pinned</span>{/if}
          </div>
          <div class="ct-box" class:isolated={mode === 'isolated'}>
            <div class="ct-h"><span class="k">container</span> {container.name}</div>
            <div class="ct-note">{container.note}</div>
            <div class="wl">
              <span class="kind {agentRef ? 'agent' : 'flow'}">{agentRef ? 'agent' : 'flow'}</span>
              <b class="mono">{agentRef ? agentId : flowRef || '—'}</b>
              {#if agentRef}<div class="sub mono">runs {flowRef}</div>{/if}
              <div class="sub">{mode} · {session} · scale {scMin}–{scMax}{lifeState === 'paused' ? ' · starts paused' : ''}</div>
            </div>
          </div>
        </div>
      {:else}
        <div class="rt-box none">
          <b>No runtime matches</b>
          <div class="sub">Change the selector or namespace, or create a runtime that carries these labels.</div>
        </div>
      {/if}
    </div>

    <div class="wire" aria-hidden="true"></div>

    <!-- Outputs -->
    <div class="col">
      <div class="col-h">Uses</div>
      {#each dependsOn as d}
        <div class="node res"><span class="mono">{d}</span><span class="sub">declared dependency</span></div>
      {/each}
      {#each calls as c}
        <div class="node call"><span class="mono">{c}</span><span class="sub">called by the flow</span></div>
      {/each}
      {#if !dependsOn.length && !calls.length}
        <div class="node ghost">nothing declared</div>
      {/if}
    </div>
  </div>

  <!-- Where else it could go -->
  {#if runtimes.length > 1 || (runtimes.length && !placed)}
    <div class="others">
      <div class="col-h">Other runtimes</div>
      <div class="chips">
        {#each verdicts.filter((v) => v.rt.name !== placed?.name) as v}
          <span class="rt-chip" class:ok={v.ok} title={v.ok ? 'Eligible — pin it to use it' : v.why}>
            {v.ok ? '✓' : '✕'} {v.rt.name}{#if !v.ok}<em>{v.why}</em>{/if}
          </span>
        {/each}
      </div>
    </div>
  {/if}

  {#if problems.length}
    <div class="probs">{#each problems as p}<div>⚠ {p}</div>{/each}</div>
  {/if}
</div>

<style>
  .dg { padding: 16px 18px; display: flex; flex-direction: column; gap: 14px; overflow-y: auto; height: 100%; font-size: 13px; }
  .muted { color: var(--text-4); }
  .mono { font-family: var(--font-mono); font-size: 12px; }
  .sub { display: block; font-size: 11px; color: var(--text-4); margin-top: 2px; }
  .k { font-size: 10px; text-transform: uppercase; letter-spacing: .06em; color: var(--text-4); }

  .live { display: flex; align-items: center; gap: 6px; flex-wrap: wrap; padding: 8px 10px; border-radius: var(--r-sm); background: var(--bg-subtle); }
  .warn-t { color: var(--warn); font-size: 12px; }
  .lnk { margin-left: auto; background: none; border: 0; color: var(--runtime, #0f766e); cursor: pointer; font-size: 12px; }

  .graph { display: grid; grid-template-columns: minmax(120px, 1fr) 24px minmax(200px, 1.6fr) 24px minmax(120px, 1fr); align-items: center; }
  @media (max-width: 720px) { .graph { grid-template-columns: 1fr; } .wire { height: 16px; width: 2px; margin: 0 auto; } }
  .col { display: flex; flex-direction: column; gap: 6px; }
  .col-h { font-size: 11px; text-transform: uppercase; letter-spacing: .06em; color: var(--text-4); margin-bottom: 2px; }
  .wire { height: 2px; background: repeating-linear-gradient(90deg, var(--border-strong) 0 6px, transparent 6px 10px); position: relative; }
  .wire::after { content: ''; position: absolute; right: -2px; top: -4px; border: 5px solid transparent; border-left-color: var(--border-strong); }

  .node { padding: 7px 9px; border: 1px solid var(--border); border-radius: var(--r-sm); background: var(--bg-surface); }
  .node.trig { border-left: 3px solid var(--signal, #2563eb); }
  .node.trig.off { opacity: .55; }
  .node.res { border-left: 3px solid var(--control, #7c3aed); }
  .node.call { border-left: 3px solid var(--text-5, #bbb); }
  .node.ghost { border-style: dashed; color: var(--text-4); }

  .rt-box { border: 2px solid var(--runtime, #0f766e); border-radius: var(--r); padding: 10px; background: var(--runtime-bg, #eef8f6); }
  .rt-box.none { border-color: var(--warn); background: var(--warn-bg, #fff8e6); }
  .rt-h { display: flex; align-items: center; gap: 6px; }
  .rt-meta { display: flex; flex-wrap: wrap; gap: 4px; margin: 6px 0 8px; }
  .ns, .lbl { font-family: var(--font-mono); font-size: 10.5px; padding: 1px 6px; border-radius: 4px; background: var(--bg-surface); color: var(--text-3); }
  .lbl.pin { color: var(--runtime, #0f766e); }
  .ct-box { border: 1px dashed var(--border-strong); border-radius: var(--r-sm); padding: 8px; background: var(--bg-surface); }
  .ct-box.isolated { border-style: solid; border-color: var(--ephemeral, #b45309); }
  .ct-h { font-size: 12.5px; }
  .ct-note { font-size: 11px; color: var(--text-4); margin-bottom: 6px; }
  .wl { padding: 8px; border-radius: var(--r-sm); background: var(--bg-subtle); }
  .kind { font-size: 10px; text-transform: uppercase; letter-spacing: .05em; padding: 1px 6px; border-radius: 4px; background: var(--spec-bg); color: var(--spec); margin-right: 4px; }
  .kind.agent { background: var(--ephemeral-bg, #fff4e5); color: var(--ephemeral, #b45309); }

  .others .chips { display: flex; flex-wrap: wrap; gap: 6px; }
  .rt-chip { font-size: 12px; padding: 3px 9px; border-radius: 999px; border: 1px solid var(--border); color: var(--text-3); }
  .rt-chip.ok { color: var(--ok); border-color: var(--ok); }
  .rt-chip em { font-style: normal; color: var(--text-4); margin-left: 6px; font-size: 11px; }

  .dot { width: 8px; height: 8px; border-radius: 50%; display: inline-block; background: var(--text-5); }
  .dot.ok { background: var(--ok); } .dot.bad { background: var(--err); } .dot.warn { background: var(--warn); } .dot.idle { background: var(--text-5, #bbb); }
  .chip { font-size: 11px; padding: 1px 8px; border-radius: 999px; border: 1px solid currentColor; }
  .chip.ok { color: var(--ok); } .chip.bad { color: var(--err); } .chip.warn { color: var(--warn); } .chip.idle { color: var(--text-4); }
  .probs { color: var(--warn); font-size: 12.5px; }
</style>
