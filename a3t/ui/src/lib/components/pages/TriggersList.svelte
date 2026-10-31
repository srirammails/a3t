<script lang="ts">
  import { openPublish } from '$lib/stores/publish.svelte.ts';
  import ExposedSections from '$lib/components/shared/ExposedSections.svelte';
  import { inContext, namespacesFor, isPlatform, itemProject, getProject } from '$lib/stores/context.svelte.ts';
  import FoldSection from '$lib/components/shared/FoldSection.svelte';
  import ProjectPill from '$lib/components/shared/ProjectPill.svelte';
  import { contextRev_ } from '$lib/stores/apps.svelte.ts';
  import SubmenuToggle from '$lib/components/shared/SubmenuToggle.svelte';
  import NewButton from '$lib/components/shared/NewButton.svelte';
  import { API_BASE } from '$lib/config';
  import { mcpCall } from '$lib/services/api';
  import { SvelteFlow, Background, Controls, type Node, type Edge, type Connection, Position } from '@xyflow/svelte';
  import '@xyflow/svelte/dist/style.css';
  import TargetNode from '$lib/components/editor/TargetNode.svelte';

  const nodeTypes = { target: TargetNode };
  import SlidePanel from '$lib/components/shared/SlidePanel.svelte';
  import PageBanner from '$lib/components/shared/PageBanner.svelte';
  import AppHome from '$lib/shell/AppHome.svelte';
  import DeleteConfirm from '$lib/components/shared/DeleteConfirm.svelte';
  import { onMount } from 'svelte';

  interface TriggerTarget { type: string; namespace?: string; name?: string; agent_id?: string; url?: string }
  interface Trigger {
    id: string; name: string; namespace: string;
    config: { type: string; path?: string; method?: string; expression?: string; source?: string; message_template?: string; message?: string; timezone?: string; filter?: string; [k: string]: any };
    targets: TriggerTarget[]; enabled: boolean; status: string; description?: string;
  }
  interface FlowSummary { name: string; namespace: string; version?: string; type?: string }

  // ── View state ──
  let view = $state<'list' | 'editor'>('list');
  let loading = $state(true);
  let triggers = $state<Trigger[]>([]);
  const myTriggers = $derived(triggers.filter((t: any) => !isPlatform(t.project)));
  const sharedTriggers = $derived(triggers.filter((t: any) => isPlatform(t.project)));
  let availableFlows = $state<FlowSummary[]>([]);
  let editingTrigger = $state<Trigger | null>(null);
  let deleteTarget = $state<Trigger | null>(null);

  // ── Form state ──
  let formName = $state('');
  let formDesc = $state('');
  let formType = $state<'webhook' | 'cron' | 'event'>('webhook');
  let formEnabled = $state(true);
  let formNs = $state(getProject() || 'default');
  let formPath = $state('/');
  let formMethod = $state('POST');
  let formMsgTemplate = $state('');
  let formCron = $state('*/5 * * * *');
  let formCronMsg = $state('');
  let formTimezone = $state('');
  let formEventSource = $state('');
  let formEventFilter = $state('');
  let formTargets = $state<TriggerTarget[]>([]);

  // ── Test state ──
  let testPayload = $state('{"message": "Hello from test!"}');
  let testRunning = $state(false);
  let testResult = $state<any>(null);
  let testError = $state('');
  let showOutput = $state(false);

  async function runTest() {
    if (formType !== 'webhook' || !formPath) return;
    testRunning = true; testResult = null; testError = ''; showOutput = true;

    // Step 1: save trigger first if it hasn't been saved
    if (!editingTrigger) {
      await saveTrigger();
      await loadTriggers();
      // Find the trigger we just created
      const found = triggers.find(t => t.name === formName);
      if (found) editingTrigger = found;
    }

    // Step 2: fire the webhook
    try {
      let payload: any;
      try { payload = JSON.parse(testPayload); } catch { testError = 'Invalid JSON payload'; testRunning = false; return; }

      const base = API_BASE;
      const path = formPath.startsWith('/') ? formPath : '/' + formPath;
      const webhookRes = await fetch(`${base}/webhook${path}`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(payload),
      });
      const webhookData = await webhookRes.json();

      if (!webhookRes.ok || webhookData.error) {
        testError = webhookData.error ?? `HTTP ${webhookRes.status}`;
        testRunning = false;
        return;
      }

      if (!webhookData.executions?.length) {
        testError = 'Trigger fired but no executions were created';
        testResult = webhookData;
        testRunning = false;
        return;
      }

      // Step 3: poll for execution result
      const instanceId = webhookData.executions[0].instance_id;
      testResult = { phase: 'running', instanceId, trigger: webhookData };

      for (let i = 0; i < 30; i++) {
        await new Promise(r => setTimeout(r, 1000));
        const statusRes = await mcpCall('get_execution_status', { instance_id: instanceId });
        if (!statusRes.ok) continue;
        const d = typeof statusRes.data === 'string' ? JSON.parse(statusRes.data) : statusRes.data;
        if (d.status === 'completed' || d.status === 'Completed') {
          testResult = { phase: 'completed', instanceId, duration: d.duration_us, output: d.output, trigger: webhookData };
          testRunning = false;
          return;
        }
        if (d.status === 'failed' || d.status === 'Failed') {
          testResult = { phase: 'failed', instanceId, trigger: webhookData };
          testError = d.error ?? 'Execution failed';
          testRunning = false;
          return;
        }
        testResult = { phase: 'running', instanceId, poll: i + 1, trigger: webhookData };
      }
      testError = 'Execution timed out (30s)';
      testRunning = false;
    } catch (e) {
      testError = String(e);
      testRunning = false;
    }
  }

  // ── Sidebar filter ──
  let sidebarSearch = $state('');
  let sidebarFilter = $state<'all' | 'flows' | 'agents'>('all');

  // ── Resize state ──
  let flowListWidth = $state(220);
  let outputHeight = $state(200);
  let draggingSide = $state(false);
  let draggingBottom = $state(false);
  let bottomContainer: HTMLDivElement;
  let editorContainer: HTMLDivElement;

  let filteredSidebarItems = $derived.by(() => {
    let items = availableFlows;
    if (sidebarFilter === 'agents') items = items.filter(f => f.type === 'agent' || f.name.includes('agent'));
    else if (sidebarFilter === 'flows') items = items.filter(f => f.type !== 'agent' && !f.name.includes('agent'));
    if (sidebarSearch) {
      const q = sidebarSearch.toLowerCase();
      items = items.filter(f => f.name.toLowerCase().includes(q) || f.namespace.toLowerCase().includes(q));
    }
    return items;
  });

  let connectedKeys = $derived(new Set(formTargets.filter(t => t.type === 'workflow').map(t => `${t.namespace}/${t.name}`)));

  // ── Load ──
  async function loadTriggers() {
    loading = true;
    const res = await mcpCall('list_triggers', {});
    if (res.ok && res.data) {
      let d: any = res.data;
      if (typeof d === 'string') { try { d = JSON.parse(d); } catch { d = []; } }
      // list_triggers returns { count, triggers: [...] }
      const all: Trigger[] = Array.isArray(d) ? d : (d?.triggers ?? d?.rows ?? []);
      // Users' triggers saved in `a3t` before projects (agent endpoints) are `default`'s.
      triggers = all
        .map((t: any) => ({ ...t, project: itemProject('trigger', t.namespace, t) }))
        .filter((t: any) => inContext(t.project));
    } else triggers = [];
    loading = false;
  }

  // A different project (or "show platform") is a different list.
  let loadedRev = contextRev_();
  $effect(() => {
    const rev = contextRev_();
    if (rev === loadedRev) return;
    loadedRev = rev;
    void loadTriggers();
  });


  async function loadFlows() {
    const all: FlowSummary[] = [];
    // Targets may be platform flows (an agent's chat wrapper), so the shared set.
    for (const ns of namespacesFor('picker')) {
      const res = await mcpCall('list_flows', { namespace: ns });
      if (res.ok && res.data) {
        const raw = typeof res.data === 'string' ? JSON.parse(res.data) : res.data;
        const flows = raw.workflows ?? raw.flows ?? (Array.isArray(raw) ? raw : []);
        for (const f of flows) {
          all.push({ name: f.name, namespace: f.namespace ?? ns, version: f.version, type: f.name.includes('agent') ? 'agent' : 'flow' });
        }
      }
    }
    availableFlows = all;
  }

  function handleDisconnect(e: Event) {
    const idx = (e as CustomEvent).detail;
    if (typeof idx === 'number') removeTarget(idx);
  }

  onMount(() => {
    loadTriggers(); loadFlows();
    window.addEventListener('trigger-disconnect', handleDisconnect);
    return () => window.removeEventListener('trigger-disconnect', handleDisconnect);
  });

  // ── Form helpers ──
  function resetForm() {
    formName = ''; formDesc = ''; formType = 'webhook'; formEnabled = true; formNs = getProject() || 'default';
    formPath = '/'; formMethod = 'POST'; formMsgTemplate = '';
    formCron = '*/5 * * * *'; formCronMsg = ''; formTimezone = '';
    formEventSource = ''; formEventFilter = '';
    formTargets = []; sidebarSearch = '';
  }

  function openCreate() { editingTrigger = null; resetForm(); view = 'editor'; }

  function openEdit(t: Trigger) {
    editingTrigger = t;
    formName = t.name; formDesc = t.description ?? ''; formType = t.config.type as any;
    formEnabled = t.enabled; formNs = t.namespace;
    formPath = t.config.path ?? '/'; formMethod = t.config.method ?? 'POST'; formMsgTemplate = t.config.message_template ?? '';
    formCron = t.config.expression ?? '*/5 * * * *'; formCronMsg = t.config.message ?? ''; formTimezone = t.config.timezone ?? '';
    formEventSource = t.config.source ?? ''; formEventFilter = t.config.filter ?? '';
    formTargets = [...t.targets]; sidebarSearch = '';
    view = 'editor';
  }

  function closeEditor() { view = 'list'; }

  async function saveTrigger() {
    const c: any = { type: formType };
    if (formType === 'webhook') { c.path = formPath; c.method = formMethod; if (formMsgTemplate) c.message_template = formMsgTemplate; }
    else if (formType === 'cron') { c.expression = formCron; c.message = formCronMsg; if (formTimezone) c.timezone = formTimezone; }
    else if (formType === 'event') { c.source = formEventSource; if (formEventFilter) c.filter = formEventFilter; }
    const triggerObj = { name: formName, namespace: formNs, config: c, targets: formTargets, enabled: formEnabled, description: formDesc };
    if (editingTrigger) await mcpCall('update_trigger', { id: editingTrigger.id, trigger: triggerObj });
    else await mcpCall('create_trigger', { trigger: triggerObj });
    view = 'list';
    await loadTriggers();
  }

  async function deleteTrigger(t: Trigger) { await mcpCall('delete_trigger', { id: t.id }); deleteTarget = null; await loadTriggers(); }
  async function toggleEnabled(t: Trigger) { await mcpCall(t.enabled ? 'pause_trigger' : 'enable_trigger', { id: t.id }); await loadTriggers(); }

  // ── Side panel resize (flow list width) ──
  function onSideDragStart(e: MouseEvent) {
    draggingSide = true; e.preventDefault();
    const onMove = (ev: MouseEvent) => {
      if (!bottomContainer) return;
      const rect = bottomContainer.getBoundingClientRect();
      const fromRight = rect.right - ev.clientX;
      flowListWidth = Math.max(140, Math.min(400, fromRight));
    };
    const onUp = () => { draggingSide = false; window.removeEventListener('mousemove', onMove); window.removeEventListener('mouseup', onUp); };
    window.addEventListener('mousemove', onMove); window.addEventListener('mouseup', onUp);
  }

  // ── Output panel resize (height) ──
  function onBottomDragStart(e: MouseEvent) {
    draggingBottom = true; e.preventDefault();
    const onMove = (ev: MouseEvent) => {
      if (!editorContainer) return;
      const rect = editorContainer.getBoundingClientRect();
      const fromBottom = rect.bottom - ev.clientY;
      outputHeight = Math.max(80, Math.min(500, fromBottom));
    };
    const onUp = () => { draggingBottom = false; window.removeEventListener('mousemove', onMove); window.removeEventListener('mouseup', onUp); };
    window.addEventListener('mousemove', onMove); window.addEventListener('mouseup', onUp);
  }

  // ── Graph ──
  let graphNodes = $derived.by<Node[]>(() => {
    const nodes: Node[] = [];
    // Trigger node center-left
    nodes.push({
      id: 'trigger', position: { x: 80, y: 180 },
      data: { label: formName || 'Trigger' },
      sourcePosition: Position.Right, type: 'default',
      style: 'background: var(--runtime-bg, #e6f4ef); border: 2px solid var(--runtime, #1a7158); border-radius: 10px; padding: 12px 20px; font-weight: 600; font-size: 14px; color: var(--runtime, #1a7158); min-width: 140px; text-align: center;',
    });
    // Connected flow nodes
    formTargets.forEach((t, i) => {
      const label = t.type === 'workflow' ? `${t.namespace}/${t.name}` : t.type === 'agent' ? `agent:${t.agent_id}` : t.url ?? '';
      nodes.push({
        id: `target-${i}`, position: { x: 450, y: 80 + i * 90 },
        data: { label, targetIdx: i },
        targetPosition: Position.Left, type: 'target',
      });
    });
    return nodes;
  });

  let graphEdges = $derived.by<Edge[]>(() => {
    return formTargets.map((_, i) => ({
      id: `e-${i}`, source: 'trigger', target: `target-${i}`,
      type: 'smoothstep', animated: true,
      style: 'stroke: var(--runtime, #1a7158); stroke-width: 2;',
    }));
  });

  // Drop a flow onto the canvas from sidebar
  function onDropFlow(e: DragEvent) {
    e.preventDefault();
    const data = e.dataTransfer?.getData('application/json');
    if (!data) return;
    try {
      const flow = JSON.parse(data) as FlowSummary;
      if (connectedKeys.has(`${flow.namespace}/${flow.name}`)) return;
      formTargets = [...formTargets, { type: 'workflow', namespace: flow.namespace, name: flow.name }];
    } catch {}
  }

  function onDragOver(e: DragEvent) { e.preventDefault(); if (e.dataTransfer) e.dataTransfer.dropEffect = 'link'; }

  function onDragFlowStart(e: DragEvent, f: FlowSummary) {
    e.dataTransfer?.setData('application/json', JSON.stringify(f));
    if (e.dataTransfer) e.dataTransfer.effectAllowed = 'link';
  }

  function removeTarget(idx: number) { formTargets = formTargets.filter((_, i) => i !== idx); }

  function typeIcon(t: string) { return t === 'webhook' ? 'W' : t === 'cron' ? 'C' : t === 'event' ? 'E' : '?'; }
  function typeStyle(t: string) {
    return t === 'webhook' ? 'background: var(--runtime-bg, #e6f4ef); color: var(--runtime, #1a7158);'
      : t === 'cron' ? 'background: var(--control-bg, #fdf0e0); color: var(--control, #c97c2e);'
      : 'background: var(--signal-bg, #f0ebfa); color: var(--signal, #7d4bbb);';
  }
</script>

{#if view === 'list'}
<!-- ═══════════ LIST VIEW ═══════════ -->
<!-- Page Header — same shape as Templates / Agents -->
<div class="screen-header">
  <div class="breadcrumb">Triggers <span class="sep">/</span> All</div>
  <div class="screen-title-row">
    <div class="title-block"><SubmenuToggle appType="triggers" /><h1 class="screen-title">Triggers</h1></div>
    <div class="screen-actions"><NewButton label="New trigger" accent="runtime" onclick={openCreate} /></div>
  </div>
</div>
<div class="trg-page">
  <PageBanner headline="Route inputs to flows." highlightWord="inputs"
    subtitle="HTTP endpoints, cron schedules, and event listeners that trigger workflows and agents."
    description="Triggers are independent. Any trigger can fan out to multiple flows. Pause, throttle, or rewire without changing the flow itself."
    accent="runtime" />
  <!-- Pinned and latest, under the banner: the section page leads with what
       it is for, then the items you actually return to. -->
  <AppHome appType="triggers" embedded />


  {#if loading}
    <div class="trg-empty">Loading...</div>
  {:else}
    {#snippet triggerRows(list: Trigger[])}
    <div class="trg-list">
      {#each list as t}
        <div class="trg-card" role="button" tabindex="0" onclick={() => openEdit(t)} onkeydown={(e) => { if (e.key === 'Enter') openEdit(t); }}>
          <span class="trg-icon" style={typeStyle(t.config.type)}>{typeIcon(t.config.type)}</span>
          <div class="trg-body">
            <div class="trg-row1">
              <span class="trg-name">{t.name}</span>
              <ProjectPill project={(t as any).project ?? t.namespace} />
              <span class="trg-badge">{t.config.type}</span>
              {#if t.config.type === 'webhook'}<span class="trg-detail">{t.config.method ?? 'POST'} {t.config.path}</span>{/if}
              {#if t.config.type === 'cron'}<span class="trg-detail">{t.config.expression}</span>{/if}
              {#if t.config.type === 'event'}<span class="trg-detail">{t.config.source}</span>{/if}
            </div>
            {#if t.targets.length > 0}
              <div class="trg-targets">
                {#each t.targets as tgt}
                  <span class="trg-target">{tgt.type === 'workflow' ? `${tgt.namespace}/${tgt.name}` : tgt.type === 'agent' ? `agent:${tgt.agent_id}` : tgt.url}</span>
                {/each}
              </div>
            {/if}
          </div>
          <span class="trg-status" class:active={t.enabled}>{t.enabled ? 'active' : 'paused'}</span>
          <div class="trg-actions" onclick={(e) => e.stopPropagation()}>
            <button class="trg-act" onclick={() => toggleEnabled(t)} title={t.enabled ? 'Pause' : 'Resume'}>
              {#if t.enabled}<svg width="12" height="12" viewBox="0 0 16 16" fill="currentColor"><rect x="3" y="2" width="3.5" height="12" rx="1"/><rect x="9.5" y="2" width="3.5" height="12" rx="1"/></svg>
              {:else}<svg width="12" height="12" viewBox="0 0 16 16" fill="currentColor"><polygon points="3,2 13,8 3,14"/></svg>{/if}
            </button>
            {#if !isPlatform((t as any).project ?? t.namespace)}
              <button class="trg-act" onclick={() => openPublish({ kind: 'trigger', from: t.namespace, ref: t.id, label: t.name })} title="Publish a release others can use">↗</button>
            {/if}
            <button class="trg-act danger" onclick={() => deleteTarget = t} title="Delete">
              <svg width="12" height="12" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5"><path d="M12 4L4 12M4 4l8 8"/></svg>
            </button>
          </div>
        </div>
      {/each}
    </div>
    {/snippet}

    <FoldSection title="My Triggers" count={myTriggers.length} hint="Start this project's flows">
      {@render triggerRows(myTriggers)}
    </FoldSection>
    {#if myTriggers.length === 0}
      <div class="trg-empty">
        <p>No triggers in this project yet.</p>
        <button class="trg-create-btn" onclick={openCreate}>+ Create your first trigger</button>
      </div>
    {/if}
    <FoldSection title="Platform Triggers" count={sharedTriggers.length} hint="The platform's own (shown with “Show platform”)">
      {@render triggerRows(sharedTriggers)}
    </FoldSection>
    <ExposedSections kind="trigger" noun="Triggers" />
  {/if}
</div>

{/if}

<!-- ═══════════ EDITOR (SlidePanel pop-in) ═══════════ -->
<SlidePanel open={view === 'editor'} onclose={closeEditor} onsave={saveTrigger}>
  <div class="te" bind:this={editorContainer}>
    <!-- TOP: Form -->
    <div class="te-form">
      <div class="tf-row-top">
        <div class="tf-col">
          <div class="tf-section-first">IDENTITY</div>
          <div class="tf-row2">
            <label><span>Name</span><input type="text" bind:value={formName} placeholder="my-webhook" /></label>
            <label><span>Namespace</span><input type="text" bind:value={formNs} /></label>
          </div>
          <label class="tf-full"><span>Description</span><input type="text" bind:value={formDesc} placeholder="What this trigger does" /></label>
        </div>
        <div class="tf-col">
          <div class="tf-section-first">CHANNEL</div>
          <div class="tf-type-tabs">
            <button class="tf-type" class:active={formType === 'webhook'} onclick={() => formType = 'webhook'}>
              <span class="tf-ti" style="background:var(--runtime-bg);color:var(--runtime);">W</span> Webhook
            </button>
            <button class="tf-type" class:active={formType === 'cron'} onclick={() => formType = 'cron'}>
              <span class="tf-ti" style="background:var(--control-bg);color:var(--control);">C</span> Cron
            </button>
            <button class="tf-type" class:active={formType === 'event'} onclick={() => formType = 'event'}>
              <span class="tf-ti" style="background:var(--signal-bg);color:var(--signal);">E</span> Event
            </button>
          </div>
          {#if formType === 'webhook'}
            <div class="tf-row2">
              <label><span>Path</span><input type="text" bind:value={formPath} placeholder="/agent" /></label>
              <label><span>Method</span><select bind:value={formMethod}><option>POST</option><option>GET</option><option>PUT</option></select></label>
            </div>
            {#if formPath}
              <div class="tf-ep"><span class="tf-ep-l">Endpoint</span><code>{formMethod} /webhook{formPath.startsWith('/') ? formPath : '/' + formPath}</code></div>
            {/if}
          {:else if formType === 'cron'}
            <div class="tf-row2">
              <label><span>Expression</span><input type="text" bind:value={formCron} placeholder="*/5 * * * *" /></label>
              <label><span>Timezone</span><input type="text" bind:value={formTimezone} placeholder="UTC" /></label>
            </div>
          {:else if formType === 'event'}
            <label class="tf-full"><span>Source</span><input type="text" bind:value={formEventSource} placeholder="system.workflow.completed" /></label>
          {/if}
          <label class="tf-check"><input type="checkbox" bind:checked={formEnabled} /> Enabled</label>
        </div>
      </div>
    </div>

    <!-- Connected targets bar -->
    {#if formTargets.length > 0}
      <div class="te-targets-bar">
        <span class="te-tb-label">Connected ({formTargets.length})</span>
        {#each formTargets as tgt, i}
          <span class="te-tb-chip">
            {tgt.type === 'workflow' ? `${tgt.namespace}/${tgt.name}` : tgt.agent_id ?? tgt.url}
            <button class="te-tb-rm" onclick={() => removeTarget(i)} title="Disconnect">
              <svg width="10" height="10" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="2"><path d="M12 4L4 12M4 4l8 8"/></svg>
            </button>
          </span>
        {/each}
      </div>
    {/if}

    <!-- BOTTOM: Canvas (left) + divider + Flow/Agent list (right) -->
    <div class="te-bottom" class:dragging-side={draggingSide} bind:this={bottomContainer} style="grid-template-columns: 1fr 5px {flowListWidth}px;">
      <!-- Canvas -->
      <div class="te-canvas" ondrop={onDropFlow} ondragover={onDragOver}>
        <SvelteFlow nodes={graphNodes} edges={graphEdges} {nodeTypes} fitView
          defaultEdgeOptions={{ type: 'smoothstep', animated: true, style: 'stroke: var(--runtime, #1a7158); stroke-width: 2;' }}
        >
          <Background />
          <Controls />
        </SvelteFlow>
        {#if formTargets.length === 0}
          <div class="te-canvas-hint">Drag flows from the right and drop here</div>
        {/if}
        <!-- Test button -->
        {#if formType === 'webhook' && formTargets.length > 0}
          <button class="te-test-btn" class:running={testRunning} onclick={runTest} disabled={testRunning}>
            {#if testRunning}
              <span class="te-test-spin"></span> Running...
            {:else}
              <svg width="12" height="12" viewBox="0 0 16 16" fill="currentColor"><polygon points="3,2 13,8 3,14"/></svg>
              Test
            {/if}
          </button>
        {/if}
      </div>

      <!-- Side divider -->
      <div class="te-side-divider" onmousedown={onSideDragStart} role="separator"></div>

      <!-- Flow/Agent list (drag source) -->
      <div class="te-flowlist">
        <div class="te-fl-header">
          <div class="te-fl-tabs">
            <button class="te-fl-tab" class:active={sidebarFilter === 'all'} onclick={() => sidebarFilter = 'all'}>All</button>
            <button class="te-fl-tab" class:active={sidebarFilter === 'flows'} onclick={() => sidebarFilter = 'flows'}>Flows</button>
            <button class="te-fl-tab" class:active={sidebarFilter === 'agents'} onclick={() => sidebarFilter = 'agents'}>Agents</button>
          </div>
          <div class="te-fl-search">
            <svg width="12" height="12" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5"><circle cx="7" cy="7" r="4"/><path d="M10 10l3.5 3.5"/></svg>
            <input type="text" bind:value={sidebarSearch} placeholder="Filter..." />
          </div>
        </div>
        <div class="te-fl-list">
          {#each filteredSidebarItems as f}
            {@const connected = connectedKeys.has(`${f.namespace}/${f.name}`)}
            <div class="te-fl-item" class:connected
              draggable={connected ? 'false' : 'true'}
              ondragstart={(e) => { if (!connected) onDragFlowStart(e, f); else e.preventDefault(); }}
              role="listitem">
              <span class="te-fl-drag">
                {#if !connected}<svg width="10" height="10" viewBox="0 0 16 16" fill="currentColor"><circle cx="5" cy="4" r="1.3"/><circle cx="11" cy="4" r="1.3"/><circle cx="5" cy="8" r="1.3"/><circle cx="11" cy="8" r="1.3"/><circle cx="5" cy="12" r="1.3"/><circle cx="11" cy="12" r="1.3"/></svg>{/if}
              </span>
              <span class="te-fl-icon" class:agent={f.type === 'agent'}>{f.type === 'agent' ? 'A' : 'F'}</span>
              <div class="te-fl-info">
                <span class="te-fl-name">{f.name}</span>
                <span class="te-fl-ns">{f.namespace}</span>
              </div>
              {#if connected}<span class="te-fl-linked">linked</span>{/if}
            </div>
          {/each}
          {#if filteredSidebarItems.length === 0}
            <div class="te-fl-empty">No items</div>
          {/if}
        </div>
      </div>
    </div>

    <!-- Output panel -->
    {#if showOutput}
      <div class="te-bottom-divider" class:dragging-bottom={draggingBottom} onmousedown={onBottomDragStart} role="separator"></div>
      <div class="te-output" style="height: {outputHeight}px;">
        <div class="te-out-header">
          <span class="te-out-title">Test</span>
          {#if testResult?.phase === 'completed'}<span class="te-out-badge ok">completed</span>{/if}
          {#if testResult?.phase === 'failed'}<span class="te-out-badge err">failed</span>{/if}
          {#if testResult?.phase === 'running'}<span class="te-out-badge run">running ({testResult.poll ?? 0}s)</span>{/if}
          {#if testResult?.instanceId}<span class="te-out-id">{testResult.instanceId}</span>{/if}
          <span style="flex:1"></span>
          <button class="te-out-close" onclick={() => showOutput = false}>
            <svg width="12" height="12" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="2"><path d="M12 4L4 12M4 4l8 8"/></svg>
          </button>
        </div>
        <div class="te-out-split">
          <!-- INPUT -->
          <div class="te-out-pane">
            <div class="te-out-pane-label">INPUT</div>
            <textarea class="te-out-editor" bind:value={testPayload} placeholder='&#123;"message": "Hello"&#125;' spellcheck="false"></textarea>
          </div>
          <!-- OUTPUT -->
          <div class="te-out-pane">
            <div class="te-out-pane-label">OUTPUT</div>
            <div class="te-out-pane-body">
              {#if testError}
                <div class="te-out-err">{testError}</div>
              {/if}
              {#if testResult?.output}
                <pre class="te-out-json">{JSON.stringify(testResult.output, null, 2)}</pre>
              {:else if testResult?.trigger}
                <pre class="te-out-json">{JSON.stringify(testResult.trigger, null, 2)}</pre>
              {:else if testRunning}
                <div class="te-out-empty">Waiting for result...</div>
              {:else}
                <div class="te-out-empty">Click Test to fire the trigger</div>
              {/if}
            </div>
          </div>
        </div>
      </div>
    {/if}
  </div>
</SlidePanel>

<!-- Was passed `name`/`type` and no `open`, so the dialog never appeared and a
     trigger could not be deleted from here. -->
<DeleteConfirm open={!!deleteTarget} entityType="Trigger" entityName={deleteTarget?.name ?? ''}
  onconfirm={() => deleteTrigger(deleteTarget!)} oncancel={() => deleteTarget = null} />

<style>
  /* ═══ LIST VIEW ═══ */
  .screen-header { padding: 18px 32px 12px; border-bottom: 1px solid var(--border); background: var(--bg-page); }
  .breadcrumb { font-size: 12px; color: var(--text-3); margin-bottom: 8px; font-family: var(--font-mono); }
  .breadcrumb .sep { color: var(--text-5); }
  .screen-title-row { display: flex; align-items: center; gap: 18px; }
  .title-block { flex: 1; display: flex; align-items: center; gap: 10px; }
  .screen-title { font-family: var(--font-serif); font-size: 24px; font-weight: 500; letter-spacing: -0.02em; }
  .screen-actions { display: flex; gap: 8px; }
  .trg-page { flex: 1; min-height: 0; overflow-y: auto; padding: 20px 32px 40px; }
  .trg-create-btn { display: flex; align-items: center; gap: 5px; padding: 6px 14px; border: 1px solid var(--runtime); border-radius: 6px; background: var(--runtime-bg); color: var(--runtime); font-size: 12px; font-weight: 550; cursor: pointer; }
  .trg-create-btn:hover { background: var(--runtime); color: white; }
  .trg-empty { padding: 60px 20px; text-align: center; color: var(--text-4); font-size: 14px; display: flex; flex-direction: column; align-items: center; gap: 12px; }
  .trg-list { display: flex; flex-direction: column; gap: 4px; }
  .trg-card { display: flex; align-items: center; gap: 10px; padding: 10px 14px; border: 1px solid var(--border); border-radius: var(--r-sm); background: var(--bg-surface); cursor: pointer; }
  .trg-card:hover { border-color: var(--spec); }
  .trg-icon { width: 32px; height: 32px; border-radius: 6px; display: grid; place-items: center; font-size: 13px; font-weight: 700; font-family: var(--font-mono); flex-shrink: 0; }
  .trg-body { flex: 1; min-width: 0; }
  .trg-row1 { display: flex; align-items: baseline; gap: 6px; }
  .trg-name { font-weight: 600; font-size: 13px; color: var(--text-1); }
  .trg-badge { font-family: var(--font-mono); font-size: 10px; color: var(--text-3); background: var(--bg-subtle); padding: 1px 5px; border-radius: 3px; }
  .trg-detail { font-family: var(--font-mono); font-size: 11px; color: var(--text-3); }
  .trg-targets { display: flex; gap: 4px; margin-top: 4px; flex-wrap: wrap; }
  .trg-target { font-family: var(--font-mono); font-size: 10px; color: var(--spec-ink); background: var(--spec-bg); padding: 1px 6px; border-radius: 3px; }
  .trg-status { font-family: var(--font-mono); font-size: 10px; text-transform: uppercase; color: var(--text-4); }
  .trg-status.active { color: var(--runtime); }
  .trg-actions { display: flex; gap: 3px; }
  .trg-act { width: 28px; height: 28px; border: 1px solid var(--border); border-radius: 5px; background: var(--bg-surface); color: var(--text-3); cursor: pointer; display: grid; place-items: center; }
  .trg-act:hover { background: var(--bg-subtle); color: var(--text-1); }
  .trg-act.danger:hover { background: var(--err-bg); color: var(--err); border-color: var(--err); }

  /* ═══ EDITOR (inside SlidePanel) ═══ */
  .te { display: flex; flex-direction: column; height: 100%; }

  /* TOP: form — two columns side by side */
  .te-form { padding: 12px 16px; border-bottom: 1px solid var(--border); flex-shrink: 0; }
  .tf-row-top { display: grid; grid-template-columns: 1fr 1fr; gap: 20px; }
  .tf-col { display: flex; flex-direction: column; gap: 4px; }

  .tf-section-first { font-size: 9px; font-weight: 700; color: var(--text-3); letter-spacing: 0.06em; margin-bottom: 6px; }

  .te-form label { display: flex; flex-direction: column; gap: 2px; }
  .te-form label span { font-size: 10px; font-weight: 600; color: var(--text-2); text-transform: uppercase; letter-spacing: 0.04em; }
  .te-form input, .te-form select { padding: 5px 7px; border: 1px solid var(--border); border-radius: 4px; background: var(--bg-surface); color: var(--text-1); font-size: 11px; font-family: var(--font-mono); outline: none; }
  .te-form input:focus, .te-form select:focus { border-color: var(--spec); }

  .tf-row2 { display: grid; grid-template-columns: 1fr 1fr; gap: 6px; margin-bottom: 4px; }
  .tf-full { margin-bottom: 4px; }

  .tf-type-tabs { display: flex; gap: 3px; margin-bottom: 6px; }
  .tf-type { display: flex; align-items: center; gap: 4px; padding: 4px 8px; border: 1px solid var(--border); border-radius: 5px; background: var(--bg-surface); color: var(--text-3); font-size: 10px; font-weight: 500; cursor: pointer; }
  .tf-type.active { border-color: var(--spec); background: var(--spec-bg); color: var(--spec-ink); }
  .tf-ti { width: 16px; height: 16px; border-radius: 3px; display: grid; place-items: center; font-size: 8px; font-weight: 700; font-family: var(--font-mono); }

  .tf-ep { background: var(--bg-subtle); border: 1px solid var(--border); border-radius: 4px; padding: 4px 8px; margin-top: 2px; }
  .tf-ep-l { font-size: 9px; font-weight: 600; color: var(--text-4); text-transform: uppercase; display: block; margin-bottom: 1px; }
  .tf-ep code { font-family: var(--font-mono); font-size: 11px; color: var(--runtime); }

  .tf-check { flex-direction: row !important; align-items: center; gap: 6px; font-size: 11px; color: var(--text-2); margin-top: 6px; }
  .tf-check input { accent-color: var(--spec); }

  /* Connected targets bar */
  .te-targets-bar { display: flex; align-items: center; gap: 6px; padding: 6px 12px; border-bottom: 1px solid var(--border); background: var(--bg-subtle); flex-wrap: wrap; flex-shrink: 0; }
  .te-tb-label { font-size: 10px; font-weight: 600; color: var(--text-3); text-transform: uppercase; letter-spacing: 0.04em; }
  .te-tb-chip { display: inline-flex; align-items: center; gap: 4px; padding: 3px 6px 3px 8px; background: var(--spec-bg); border: 1px solid var(--spec-bg); border-radius: 4px; font-family: var(--font-mono); font-size: 11px; color: var(--spec-ink); font-weight: 550; }
  .te-tb-rm { width: 16px; height: 16px; border: none; border-radius: 3px; background: none; color: var(--text-4); cursor: pointer; display: grid; place-items: center; flex-shrink: 0; }
  .te-tb-rm:hover { color: var(--err); background: var(--err-bg); }

  /* BOTTOM: canvas + divider + flow list */
  .te-bottom { flex: 1; display: grid; overflow: hidden; min-height: 0; }
  .te-bottom.dragging-side { user-select: none; cursor: col-resize; }

  /* Side divider */
  .te-side-divider { width: 5px; cursor: col-resize; background: var(--border); transition: background 0.15s; }
  .te-side-divider:hover, .te-bottom.dragging-side .te-side-divider { background: var(--spec, #5530a8); }

  /* Bottom divider */
  .te-bottom-divider { height: 5px; cursor: row-resize; background: var(--border); transition: background 0.15s; flex-shrink: 0; }
  .te-bottom-divider:hover, .te-bottom-divider.dragging-bottom { background: var(--spec, #5530a8); }

  /* Canvas */
  .te-canvas { position: relative; overflow: hidden; }
  .te-canvas-hint { position: absolute; bottom: 16px; left: 50%; transform: translateX(-50%); font-size: 12px; color: var(--text-4); background: var(--bg-surface); padding: 6px 14px; border-radius: 6px; border: 1px dashed var(--border); pointer-events: none; z-index: 5; }

  /* Flow/Agent list */
  .te-flowlist { display: flex; flex-direction: column; overflow: hidden; background: var(--bg-surface); }
  .te-fl-header { padding: 6px 8px; border-bottom: 1px solid var(--border); }
  .te-fl-tabs { display: flex; gap: 2px; margin-bottom: 4px; }
  .te-fl-tab { padding: 3px 8px; border: none; border-radius: 4px; background: none; color: var(--text-4); font-size: 10px; font-weight: 550; cursor: pointer; }
  .te-fl-tab.active { background: var(--bg-subtle); color: var(--text-1); }
  .te-fl-search { display: flex; align-items: center; gap: 4px; padding: 3px 6px; border: 1px solid var(--border); border-radius: 4px; background: var(--bg-page); }
  .te-fl-search input { border: none; background: none; flex: 1; font-size: 11px; outline: none; color: var(--text-1); padding: 0; }
  .te-fl-list { flex: 1; overflow-y: auto; }
  .te-fl-item { display: flex; align-items: center; gap: 6px; padding: 7px 8px; border-bottom: 1px solid var(--border); cursor: grab; transition: background 0.1s; }
  .te-fl-item:hover { background: var(--bg-subtle); }
  .te-fl-item.connected { background: var(--runtime-bg); cursor: default; }
  .te-fl-item:active:not(.connected) { cursor: grabbing; }
  .te-fl-drag { width: 14px; flex-shrink: 0; color: var(--text-5); display: grid; place-items: center; }
  .te-fl-icon { width: 20px; height: 20px; border-radius: 4px; display: grid; place-items: center; font-size: 9px; font-weight: 700; font-family: var(--font-mono); flex-shrink: 0; background: var(--spec-bg); color: var(--spec-ink); }
  .te-fl-icon.agent { background: var(--signal-bg); color: var(--signal); }
  .te-fl-info { flex: 1; min-width: 0; }
  .te-fl-name { font-family: var(--font-mono); font-size: 11px; font-weight: 550; color: var(--text-1); display: block; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .te-fl-ns { font-size: 9px; color: var(--text-4); }
  .te-fl-linked { font-size: 8px; color: var(--runtime); font-weight: 600; text-transform: uppercase; flex-shrink: 0; }
  .te-fl-empty { padding: 20px; text-align: center; color: var(--text-4); font-size: 11px; }

  /* Test button (floating on canvas) */
  .te-test-btn {
    position: absolute; top: 12px; right: 12px; z-index: 5;
    display: flex; align-items: center; gap: 5px;
    padding: 7px 16px; border: 1px solid var(--runtime); border-radius: 6px;
    background: var(--runtime); color: white; font-size: 12px; font-weight: 600;
    cursor: pointer; box-shadow: 0 2px 8px rgba(26,113,88,0.25); transition: all 0.12s;
  }
  .te-test-btn:hover:not(:disabled) { opacity: 0.9; transform: translateY(-1px); }
  .te-test-btn:disabled { opacity: 0.6; cursor: default; }
  .te-test-btn.running { background: var(--control); border-color: var(--control); }
  .te-test-spin { width: 12px; height: 12px; border: 2px solid rgba(255,255,255,0.3); border-top-color: white; border-radius: 50%; animation: spin 0.8s linear infinite; }
  @keyframes spin { to { transform: rotate(360deg); } }

  /* Output panel (bottom) */
  .te-output { flex-shrink: 0; background: var(--bg-surface); display: flex; flex-direction: column; overflow: hidden; }

  .te-out-header { display: flex; align-items: center; gap: 6px; padding: 6px 12px; border-bottom: 1px solid var(--border); flex-shrink: 0; }
  .te-out-title { font-size: 11px; font-weight: 600; color: var(--text-2); text-transform: uppercase; letter-spacing: 0.04em; }
  .te-out-badge { font-size: 9px; font-family: var(--font-mono); padding: 1px 6px; border-radius: 3px; font-weight: 600; text-transform: uppercase; }
  .te-out-badge.ok { background: var(--runtime-bg); color: var(--runtime); }
  .te-out-badge.err { background: var(--err-bg, #fde8e8); color: var(--err, #dc2626); }
  .te-out-badge.run { background: var(--control-bg); color: var(--control); }
  .te-out-id { font-family: var(--font-mono); font-size: 9px; color: var(--text-4); }
  .te-out-close { width: 22px; height: 22px; border: 1px solid var(--border); border-radius: 4px; background: var(--bg-surface); color: var(--text-4); cursor: pointer; display: grid; place-items: center; flex-shrink: 0; }
  .te-out-close:hover { background: var(--bg-subtle); color: var(--text-1); }

  /* Split: input | output */
  .te-out-split { flex: 1; display: grid; grid-template-columns: 1fr 1fr; overflow: hidden; }
  .te-out-pane { display: flex; flex-direction: column; overflow: hidden; }
  .te-out-pane:first-child { border-right: 1px solid var(--border); }
  .te-out-pane-label { font-size: 9px; font-weight: 700; color: var(--text-4); text-transform: uppercase; letter-spacing: 0.05em; padding: 5px 10px; background: var(--bg-subtle); border-bottom: 1px solid var(--border); flex-shrink: 0; }
  .te-out-editor {
    flex: 1; padding: 8px 10px; border: none; background: var(--bg-page); color: var(--text-1);
    font-family: var(--font-mono); font-size: 11px; resize: none; outline: none;
  }
  .te-out-pane-body { flex: 1; overflow-y: auto; padding: 8px 10px; }
  .te-out-err { color: var(--err); font-size: 11px; font-family: var(--font-mono); padding: 6px 8px; background: var(--err-bg, #fde8e8); border-radius: 4px; margin-bottom: 6px; }
  .te-out-json { font-family: var(--font-mono); font-size: 11px; color: var(--text-1); background: var(--bg-page); border: 1px solid var(--border); border-radius: 4px; padding: 8px; margin: 0; white-space: pre-wrap; word-break: break-word; }
  .te-out-empty { font-size: 12px; color: var(--text-4); text-align: center; padding: 20px; }
</style>
