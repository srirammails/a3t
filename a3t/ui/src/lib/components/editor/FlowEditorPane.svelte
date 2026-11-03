<script lang="ts">
  import * as YAML from 'yaml';
  import { SvelteFlow, Background, Controls, MiniMap, type Node, type Edge } from '@xyflow/svelte';
  import '@xyflow/svelte/dist/style.css';
  import StepNode from './StepNode.svelte';
  import WorkflowNode from './WorkflowNode.svelte';
  import AgentBlockNode from './AgentBlockNode.svelte';
  import AgentPortNode from './AgentPortNode.svelte';
  import AgentSectionNode from './AgentSectionNode.svelte';
  import PortCardNode from './PortCardNode.svelte';
  import ResourceNode from './ResourceNode.svelte';
  import PipelineStepNode from './PipelineStepNode.svelte';
  import SubFlowNode from './SubFlowNode.svelte';
  import ControlNode from './ControlNode.svelte';
  import JoinNode from './JoinNode.svelte';
  import FlowMarkerNode from './FlowMarkerNode.svelte';
  import YamlEditor from './YamlEditor.svelte';
  import SchemaOutline from './SchemaOutline.svelte';
  import GraphPalette from './GraphPalette.svelte';
  import { addTask, addCase, paletteStep, setThen, STEP_MIME } from '$lib/studio-core/flow/compose';
  import { ITEM_MIME } from '$lib/studio-core/items';
  import { outline, completeAt } from '$lib/studio-core/flow/schema';
  import GraphFollow from './GraphFollow.svelte';
  import SchemaForm from './SchemaForm.svelte';
  import AgentTemplateEditor from './AgentTemplateEditor.svelte';
  import { parseWorkflowToGraph, isAgentTemplateYaml } from './workflowParser';
  import { runsApi, type Breakpoint, type DebugCommand } from '$lib/services/api';
  import { lineOfStep, stepLines } from '$lib/studio-core/flow/model';
  import type { FlowDebug } from '$lib/components/run/debugState.svelte.ts';
  import type { Snippet } from 'svelte';

  let {
    initialYaml = '',
    onchange,
    onopensub,
    readOnly = false,
    configSeed = null,
    singleColumn = false,
    showAi = true,
    onsection,
    aiKind = 'flow',
    aiName = '',
    part = null,
    debug = null,
    onrun,
    onshowrun,
    toolbar,
    showCode,
    focusStep = null,
    gotoLine = null,
    palette = false,
    onaddstep,
    ondropat,
  }: {
    initialYaml: string;
    onchange?: (val: string) => void;
    onopensub?: (type: string, data?: any) => void;
    readOnly?: boolean;
    // Seed for the agent-template form when the flow's config is a runtime
    // expression (see AgentTemplateEditor.configSeed).
    configSeed?: Record<string, any> | null;
    /// One unified tab bar (Graph | Code | Form), one view at a time, no split.
    /// For narrow hosts such as the in-chat artifact pane.
    singleColumn?: boolean;
    /// The AI tab is redundant where the host *is* a chat.
    showAi?: boolean;
    /// Reports the visible section (`graph` | `code` | form sub-tab) so a chat
    /// host can tell the agent what the user is looking at.
    onsection?: (section: string) => void;
    /// What the AI tab tells the agent it is editing, e.g. "template" and its name.
    aiKind?: string;
    aiName?: string;
    /// Render one piece only, for hosts that lay the pieces out themselves (the
    /// studio puts Form and AI in its own panels): `main` = Graph | Code,
    /// `form`, or `ai`. Instances stay in step through `initialYaml`/`onchange`.
    part?: 'main' | 'form' | 'ai' | null;
    /// The flow's breakpoints and shown run (studio): marked on the graph and
    /// driven from its context menu.
    debug?: FlowDebug | null;
    /// Context menu → Run / Debug, and → show a step in the Runs panel.
    onrun?: (debug: boolean) => void;
    onshowrun?: () => void;
    /// Bump to switch the main view to Code (the Studio's Edit / double-click).
    showCode?: number;
    /// A step for the form to open (its name, and a stamp per request).
    focusStep?: { name: string; n: number } | null;
    /// A line to show in the Code view (a problem picked in the Problems panel).
    gotoLine?: { line: number; n: number } | null;
    /// Show the step palette on the graph (flows): drag a step onto the flow, or
    /// click to add it. `onaddstep` gets the palette step's id (the host adds it
    /// and opens it in its form); without it the pane adds the step itself.
    palette?: boolean;
    onaddstep?: (stepId: string) => void;
    /// Something dropped on a step or a connector of the graph: where in the
    /// flow it goes (the list, the index, in words). Return true when taken;
    /// a drop elsewhere on the graph bubbles to the host (appends).
    ondropat?: (e: DragEvent, at: InsertAt) => boolean;
    /// Host content for the Graph | Code toolbar (`main` only), after the tabs —
    /// the studio puts the flow's name and Run/Deploy here instead of a bar of its own.
    toolbar?: Snippet;
  } = $props();

  const nodeTypes = {
    step: StepNode, workflow: WorkflowNode, agent: AgentBlockNode, agentPort: AgentPortNode,
    agentSection: AgentSectionNode, portCard: PortCardNode,
    resource: ResourceNode, pipeline: PipelineStepNode, subflow: SubFlowNode,
    control: ControlNode, join: JoinNode, flowMarker: FlowMarkerNode,
  };

  import { onMount, onDestroy, untrack } from 'svelte';
  type Path = (string | number)[];
  export type InsertAt = { list: Path; index: number; label: string };
  import DesignerChat from '$lib/components/editor/DesignerChat.svelte';

  // ── Internal state ──
  let leftMode = $state<'graph' | 'code'>(untrack(() => (showCode ? 'code' : 'graph')));
  $effect(() => { if (showCode) leftMode = 'code'; });
  let rightMode = $state<'form' | 'ai'>('form');
  // Single-column hosts drive one selection across all three views.
  let soloMode = $state<'graph' | 'code' | 'form'>('graph');
  // Graph/Code report themselves; the form delegates to its own sub-tabs.
  $effect(() => { if (soloMode !== 'form') onsection?.(soloMode); });
  // With the AI tab hidden, never leave the right pane on a tab that is gone.
  $effect(() => { if (!showAi && rightMode === 'ai') rightMode = 'form'; });
  let yamlSource = $state(untrack(() => initialYaml));
  // Follow the text when it changes outside (a step dropped on the flow, the
  // other pane of the same document); our own edits come back unchanged.
  $effect(() => {
    const next = initialYaml;
    if (next !== untrack(() => yamlSource)) yamlSource = next;
  });
  let agentScrollTarget = $state('');
  let splitPercent = $state(50);
  let dragging = $state(false);
  let splitContainer: HTMLDivElement;

  // Port expansion now handled inside AgentNode component (inline panels)

  function onDragStart(e: MouseEvent) {
    dragging = true;
    e.preventDefault();
    const onMove = (ev: MouseEvent) => {
      if (!splitContainer) return;
      const rect = splitContainer.getBoundingClientRect();
      const pct = ((ev.clientX - rect.left) / rect.width) * 100;
      splitPercent = Math.max(25, Math.min(75, pct));
    };
    const onUp = () => {
      dragging = false;
      window.removeEventListener('mousemove', onMove);
      window.removeEventListener('mouseup', onUp);
    };
    window.addEventListener('mousemove', onMove);
    window.addEventListener('mouseup', onUp);
  }

  // Listen for clicks on agent node ports
  function handleAgentScroll(e: Event) {
    const section = (e as CustomEvent).detail;
    agentScrollTarget = '';
    // Tick then set to trigger the effect
    requestAnimationFrame(() => { agentScrollTarget = section; });
    rightMode = 'form';
  }

  // Listen for "+ Add" clicks from agent node panels
  function handleAgentAdd(e: Event) {
    const section = (e as CustomEvent).detail;
    // Map node section IDs to form section IDs
    const sectionMap: Record<string, string> = {
      config: 'configuration',
      ext: 'extensions',
      events: 'extensions',
      input: 'identity',
      output: 'identity',
      error: 'extensions',
    };
    agentScrollTarget = '';
    requestAnimationFrame(() => { agentScrollTarget = sectionMap[section] ?? section; });
    rightMode = 'form';
  }

  onMount(() => {
    window.addEventListener('agent-scroll', handleAgentScroll);
    window.addEventListener('agent-add', handleAgentAdd);
  });
  onDestroy(() => {
    if (typeof window !== 'undefined') {
      window.removeEventListener('agent-scroll', handleAgentScroll);
      window.removeEventListener('agent-add', handleAgentAdd);
    }
  });

  // Re-init when a different template is selected (guard against cascade)
  let lastInitYaml = '';
  $effect(() => {
    if (initialYaml !== lastInitYaml) {
      lastInitYaml = initialYaml;
      yamlSource = initialYaml;
    }
  });

  let parsed = $derived.by(() => {
    if (!yamlSource) return null;
    try {
      return YAML.parse(yamlSource) ?? {};
    } catch (e) {
      console.warn('[FlowEditorPane] YAML parse error:', (e as Error).message?.slice(0, 100));
      return null;
    }
  });

  // Detect agent template
  let isAgentTemplate = $derived(isAgentTemplateYaml(parsed));

  // For agent templates, pass parsed CNCF YAML directly to the form editor
  // No CRD conversion — the form editor reads CNCF YAML natively now

  // ── Two-way sync ──
  let syncLock = false;

  function onCodeChange(val: string) {
    if (syncLock) return;
    syncLock = true;
    try {
      yamlSource = val;
      onchange?.(val);
    } finally {
      // Without the finally, a throw from the host's onchange wedged syncLock
      // true and silently killed every later edit.
      syncLock = false;
    }
  }

  function onFormChange(val: string) {
    if (syncLock) return;
    syncLock = true;
    try {
      yamlSource = val;
      onchange?.(val);
    } finally {
      // Without the finally, a throw from the host's onchange wedged syncLock
      // true and silently killed every later edit.
      syncLock = false;
    }
  }

  // Agent form emits CNCF YAML (source of truth)
  function onAgentFormChange(yaml: string) {
    if (syncLock) return;
    syncLock = true;
    try {
      yamlSource = yaml;
      onchange?.(yaml);
    } finally {
      // Without the finally, a throw from the host's onchange wedged syncLock
      // true and silently killed every later edit.
      syncLock = false;
    }
  }



  // ── Graph: unified CNCF workflow parser (handles both agents + regular flows) ──
  let baseGraph = $derived.by(() => {
    if (!parsed) return null;
    const hasWorkflow = parsed.steps || parsed.do || isAgentTemplate;
    if (!hasWorkflow) return null;
    return parseWorkflowToGraph(parsed);
  });

  // Reset state handled inside node components

  // Nodes and edges directly from parser (expansion handled inside node components)
  let nodes = $derived.by<Node[]>(() => baseGraph?.nodes ?? []);
  /// Edges flow only while the shown run is live (running, or a debug run paused);
  /// at rest the graph is still.
  let edges = $derived.by<Edge[]>(() => {
    const live = !!debug?.live;
    return (baseGraph?.edges ?? []).map((e) => (e.animated && !live ? { ...e, animated: false } : e));
  });

  // ── Breakpoints and the run's position on the graph ──
  let shownNodes = $derived.by<Node[]>(() => {
    if (!debug) return nodes;
    const marks = debug.marks, current = debug.current, bps = debug.breakpoints;
    return nodes.map((n) => {
      const k = n.data?.stepKey as string | undefined;
      if (!k) return n;
      const bp = bps.find((b) => b.step_key === k);
      const cls = [bp && `bp bp-${bp.when ?? 'before'}`, marks[k] && `st-${marks[k]}`, current === k && 'here'].filter(Boolean).join(' ');
      return cls ? { ...n, class: cls } : n;
    });
  });

  /// The node the shown run stands at, kept in view.
  const currentNode = $derived(debug?.current ? nodes.find((n) => n.data?.stepKey === debug!.current)?.id ?? null : null);

  // ── The same in the code: step lines, their breakpoints and states ──
  const lines = $derived(debug ? stepLines(yamlSource, nodes.map((n) => n.data?.stepKey as string).filter(Boolean)) : new Map<string, number>());
  const keyAtLine = $derived(new Map([...lines].map(([k, l]) => [l, k])));
  const codeMarks = $derived.by(() => {
    if (!debug) return null;
    const bps = debug.breakpoints, marks = debug.marks, current = debug.current;
    return [...lines].map(([k, line]) => ({
      line, bp: bps.find((b) => b.step_key === k)?.when ?? (bps.some((b) => b.step_key === k) ? 'before' : undefined),
      state: marks[k], current: current === k,
    }));
  });
  function onGutter(line: number) {
    const k = keyAtLine.get(line);
    if (!k) return;
    note = '';
    // A click sets a breakpoint before the step, or clears whichever is set.
    void debug!.toggle(k, bpOf(k)?.when ?? 'before').then((err) => { if (err) note = err; });
  }
  function onCodeContext(e: MouseEvent, line: number): boolean {
    const k = keyAtLine.get(line);
    if (!k) return false;
    openMenu(e, { data: { stepKey: k, label: k.split('/').pop() } }, 'code');
    return true;
  }
  /// A node to bring into view on the graph ("Show on graph" from the code).
  let graphFocus = $state<{ id: string; n: number } | null>(null);
  function showOnGraph(key?: string) {
    leftMode = 'graph';
    soloMode = 'graph';
    const id = key ? nodes.find((n) => n.data?.stepKey === key)?.id : null;
    if (id) graphFocus = { id, n: ++revealN };
  }

  // ── Dropping between steps ──
  /// What a drop at this element of the graph means: on a step → after it; on a
  /// connector → before the step it leads to (or after the one it leaves, when
  /// it leads to a join or the end); on start → first. Null: anywhere else.
  let dropAt = $state<(InsertAt & { node?: string; edge?: string }) | null>(null);
  const nodeById = (id: string | null | undefined) => (id ? nodes.find((n) => n.id === id) : undefined);
  const pathOf = (id: string | null | undefined) => nodeById(id)?.data?.path as Path | undefined;
  const after = (p: Path) => ({ list: p.slice(0, -2), index: (p[p.length - 2] as number) + 1, label: `after ${p[p.length - 1]}` });
  const before = (p: Path) => ({ list: p.slice(0, -2), index: p[p.length - 2] as number, label: `before ${p[p.length - 1]}` });
  function targetAt(el: Element | null): (InsertAt & { node?: string; edge?: string }) | null {
    const nodeEl = el?.closest('.svelte-flow__node');
    if (nodeEl) {
      const id = nodeEl.getAttribute('data-id') ?? '';
      const n = nodeById(id);
      const p = pathOf(id);
      if (p) return { ...after(p), node: id };
      if (n?.data?.marker === 'start') return { list: ['do'], index: 0, label: 'at the start', node: id };
      return null;
    }
    const edgeEl = el?.closest('.svelte-flow__edge');
    if (edgeEl) {
      const id = edgeEl.getAttribute('data-id') ?? '';
      const e = edges.find((x) => x.id === id);
      if (!e) return null;
      const t = pathOf(e.target), src = pathOf(e.source);
      if (t) return { ...before(t), edge: id };
      if (src) return { ...after(src), edge: id };
      if (nodeById(e.source)?.data?.marker === 'start') return { list: ['do'], index: 0, label: 'at the start', edge: id };
    }
    return null;
  }
  const droppable = (e: DragEvent) => { const t = e.dataTransfer?.types ?? []; return t.includes(STEP_MIME) || t.includes(ITEM_MIME); };
  function onGraphDragOver(e: DragEvent) {
    if (!ondropat || readOnly || !droppable(e)) return;
    // Accept here: over a connector the event may not bubble out of the graph.
    e.preventDefault();
    if (e.dataTransfer) e.dataTransfer.dropEffect = 'copy';
    const at = targetAt(e.target as Element);
    if (at?.node !== dropAt?.node || at?.edge !== dropAt?.edge) mark(at);
    dropAt = at;
  }
  /// The step or connector a drop would go to, marked on the element itself:
  /// changing the graph's nodes or edges mid-drag would redraw them under the
  /// pointer and lose the target.
  let marked: Element | null = null;
  function mark(at: { node?: string; edge?: string } | null) {
    marked?.classList.remove('drop-target');
    marked = null;
    const sel = at?.node ? `.svelte-flow__node[data-id="${at.node}"]` : at?.edge ? `.svelte-flow__edge[data-id="${at.edge}"]` : '';
    if (sel) { marked = document.querySelector(sel); marked?.classList.add('drop-target'); }
  }
  function onGraphDrop(e: DragEvent) {
    const at = dropAt;
    dropAt = null;
    mark(null);
    if (!at || !ondropat) return;
    if (ondropat(e, { list: at.list, index: at.index, label: at.label })) { e.preventDefault(); e.stopPropagation(); }
  }

  // ── Connecting steps: a jump (`then:`) ──
  /// A connection drawn from one step to another sets the first one's `then:`
  /// (both in the same list); from a switch, it adds a case going there; to the
  /// end marker, `then: end`.
  function onConnect(c: { source: string; target: string }) {
    const sp = pathOf(c.source);
    const tgt = nodeById(c.target);
    const tp = pathOf(c.target);
    if (!sp) { note = 'Connect from a step.'; return; }
    const to = tgt?.data?.marker === 'end' ? 'end' : tp ? String(tp[tp.length - 1]) : '';
    if (!to) { note = 'Connect to a step, or to the end.'; return; }
    if (tp && JSON.stringify(tp.slice(0, -2)) !== JSON.stringify(sp.slice(0, -2))) {
      note = `${sp[sp.length - 1]} can only jump to a step in its own list — ${to} is in another.`;
      return;
    }
    if (tp && JSON.stringify(tp) === JSON.stringify(sp)) return;
    note = '';
    const isSwitch = nodeById(c.source)?.data?.taskType === 'switch';
    onCodeChange(isSwitch ? addCase(yamlSource, sp, to).yaml : setThen(yamlSource, sp, to));
  }

  /// A palette block clicked: the host adds it (and opens it in its form), or we do.
  function addFromPalette(id: string) {
    if (onaddstep) { onaddstep(id); return; }
    const st = paletteStep(id);
    if (st) onCodeChange(addTask(yamlSource, st.base, st.task()).yaml);
  }

  // ── The code view's schema outline ──
  let showOutline = $state((() => { try { return localStorage.getItem('a3t.code.outline') !== 'off'; } catch { return true; } })());
  $effect(() => { try { localStorage.setItem('a3t.code.outline', showOutline ? 'on' : 'off'); } catch { /* private window */ } });
  let outlinePct = $state(42);
  let outlineDrag = $state(false);
  let codeSplit: HTMLDivElement | undefined = $state();
  function onOutlineDrag(e: MouseEvent) {
    e.preventDefault(); outlineDrag = true;
    const move = (ev: MouseEvent) => { if (!codeSplit) return; const r = codeSplit.getBoundingClientRect(); outlinePct = Math.max(22, Math.min(70, ((ev.clientX - r.left) / r.width) * 100)); };
    const up = () => { outlineDrag = false; window.removeEventListener('mousemove', move); window.removeEventListener('mouseup', up); };
    window.addEventListener('mousemove', move); window.addEventListener('mouseup', up);
  }
  /// The schema's problems, marked on their lines in the text.
  const schemaProblems = $derived(leftMode === 'code' || soloMode === 'code' ? outline(yamlSource).problems : []);
  const codeIssues = $derived(schemaProblems.filter((p) => p.line).map((p) => ({ line: p.line!, level: p.level, message: p.message })));
  const issueCount = $derived(schemaProblems.length);
  // A problem picked elsewhere (the Problems panel): show its line.
  $effect(() => {
    const g = gotoLine;
    if (!g) return;
    untrack(() => { leftMode = 'code'; soloMode = 'code'; reveal = { line: g.line, n: ++revealN }; });
  });

  // ── Context menu ──
  type Menu = { x: number; y: number; key?: string; label?: string; from: 'graph' | 'code' };
  let menu = $state<Menu | null>(null);
  let note = $state('');
  let reveal = $state<{ line: number; n: number } | null>(null);
  let revealN = 0;

  function openMenu(event: MouseEvent, node?: any, from: Menu['from'] = 'graph') {
    event.preventDefault();
    menu = { x: event.clientX, y: event.clientY, key: node?.data?.stepKey, label: node?.data?.label, from };
  }
  /// Close the menu and run its action on the step it was opened on (taken
  /// before closing: the menu's own fields go with it); a returned string or a
  /// throw is shown as a note.
  function act(f: (key: string) => unknown) {
    const key = menu?.key ?? '';
    menu = null;
    void (async () => {
      note = '';
      try { const err = await f(key); if (typeof err === 'string' && err) note = err; } catch (e) { note = (e as Error).message; }
    })();
  }
  const bpOf = (k: string) => debug?.breakpoints.find((b) => b.step_key === k);
  const bpToggle = (when: Breakpoint['when']) => act((k) => debug!.toggle(k, when));
  async function command(c: DebugCommand, key?: string) {
    const r = await runsApi.debug(debug!.runId!, c, key);
    return r.ok ? '' : r.error ?? 'Command failed';
  }
  function gotoCode(key?: string) {
    leftMode = 'code';
    soloMode = 'code';
    const line = key ? lineOfStep(yamlSource, key) : null;
    if (line) reveal = { line, n: ++revealN };
  }
  function showInRun(key: string) {
    debug!.focus = key;
    onshowrun?.();
  }
  function onMenuKey(e: KeyboardEvent) { if (e.key === 'Escape') menu = null; }

  // Handle node clicks — toggle port expansion
  function handleNodeClick({ event, node }: { event: MouseEvent | TouchEvent; node: any }) {
    // Clicking a sub-node spawned by port expansion — scroll form
    if (node.data?._portChild) {
      if (typeof window !== 'undefined') {
        window.dispatchEvent(new CustomEvent('agent-scroll', { detail: node.data._portSection }));
      }
      rightMode = 'form';
      return;
    }

    // Clicking agent or workflow node — no-op (use port clicks instead)
    // Port clicks are handled via custom events from the node component
    if (node.type === 'agent' || node.type === 'workflow') {
      // Scroll the form editor
      if (typeof window !== 'undefined') {
        window.dispatchEvent(new CustomEvent('agent-scroll', { detail: 'configuration' }));
      }
      rightMode = 'form';
    }
  }
</script>

{#snippet graphView(showMiniMap: boolean)}
  {#if palette && !readOnly && !isAgentTemplate}<GraphPalette yaml={yamlSource} onadd={addFromPalette} />{/if}
  {#if nodes.length > 0}
    {#if baseGraph?.meta}
      <div class="wf-meta-bar">
        <span class="wf-meta-format">{baseGraph.meta.format === 'cncf-dsl' ? 'CNCF DSL' : 'FlowR'}</span>
        {#if baseGraph.meta.dsl}<span class="wf-meta-tag">v{baseGraph.meta.dsl}</span>{/if}
        <span class="wf-meta-tag">{baseGraph.meta.taskCount} tasks</span>
        {#if baseGraph.meta.hasExtensions}<span class="wf-meta-tag ext">{baseGraph.meta.extensionCount} ext</span>{/if}
      </div>
    {/if}
    <div class="gdrop" role="presentation" ondragover={onGraphDragOver} ondragleave={(e) => { if (e.currentTarget === e.target) { dropAt = null; mark(null); } }} ondrop={onGraphDrop}>
    <SvelteFlow nodes={shownNodes} {edges} {nodeTypes} fitView onnodeclick={handleNodeClick}
      onnodecontextmenu={({ event, node }) => openMenu(event, node)} onpanecontextmenu={({ event }) => openMenu(event)}
      onconnect={(c) => onConnect(c)}
      nodesDraggable={!readOnly} nodesConnectable={!readOnly} elementsSelectable={!readOnly}>
      <Background />
      <Controls />
      {#if showMiniMap}<MiniMap />{/if}
      <GraphFollow nodeId={currentNode} />
      {#if graphFocus}{#key graphFocus.n}<GraphFollow nodeId={graphFocus.id} delay={300} />{/key}{/if}
    </SvelteFlow>
    {#if dropAt}<div class="drop-label">Insert {dropAt.label}</div>{/if}
    </div>
    {#if debug?.runId && Object.keys(debug.marks).length}
      <div class="run-legend" title="The run selected in the Runs panel">
        <span class="lg done">ran</span><span class="lg failed">failed</span><span class="lg running">running</span><span class="lg paused">paused</span>
      </div>
    {/if}
    {#if note}<button class="graph-note" onclick={() => (note = '')} title="Dismiss">{note}</button>{/if}
  {:else}
    <div class="fep-empty">No steps to visualize — add <code>steps:</code> or <code>do:</code> tasks</div>
  {/if}
{/snippet}

{#snippet codeView()}
  <!-- The text, with the CNCF DSL outline beside it: the whole schema at each
       place, what the flow sets in full, its problems marked in both. -->
  <div class="code-split" class:no-outline={!showOutline} class:dragging={outlineDrag} bind:this={codeSplit} style="grid-template-columns: {showOutline ? `${outlinePct}% 5px 1fr` : '1fr'}">
    {#if showOutline}
      <div class="code-outline">
        <SchemaOutline text={yamlSource} onchange={onCodeChange} {readOnly} onreveal={(line) => (reveal = { line, n: ++revealN })} />
      </div>
      <div class="code-div" role="separator" onmousedown={onOutlineDrag}></div>
    {/if}
    <div class="code-text">
      <button class="outline-tg" onclick={() => (showOutline = !showOutline)} title={showOutline ? 'Hide the CNCF DSL outline' : 'Show the CNCF DSL outline'}>{showOutline ? '◧' : '◨'} Schema{#if !showOutline && issueCount} <b>{issueCount}</b>{/if}</button>
      <YamlEditor value={yamlSource} onchange={onCodeChange} {readOnly} {reveal} issues={codeIssues} complete={completeAt}
        marks={codeMarks} ongutter={onGutter} oncontext={debug ? onCodeContext : undefined} />
    </div>
  </div>
{/snippet}

{#snippet contextMenu(m: Menu)}
  <div class="cm-overlay" role="presentation" onclick={() => (menu = null)} oncontextmenu={(e) => { e.preventDefault(); menu = null; }}></div>
  <div class="cm" role="menu" style="left: {Math.min(m.x, window.innerWidth - 250)}px; top: {Math.min(m.y, window.innerHeight - 320)}px">
    {#if m.key}
      {@const bp = bpOf(m.key)}
      <div class="cm-head"><span>{m.label}</span><code>{m.key}</code></div>
      {#if debug}
        <button role="menuitem" onclick={() => bpToggle('before')}><span class="cm-ic bp">●</span>{bp?.when === 'before' || (bp && !bp.when) ? 'Remove breakpoint' : 'Break before this step'}</button>
        <button role="menuitem" onclick={() => bpToggle('after')}><span class="cm-ic bp">◐</span>{bp?.when === 'after' ? 'Remove break after' : 'Break after this step'}</button>
        <button role="menuitem" onclick={() => bpToggle('on_error')}><span class="cm-ic bp">⚠</span>{bp?.when === 'on_error' ? 'Remove break on error' : 'Break if this step fails'}</button>
        {#if debug.paused}
          <hr />
          <button role="menuitem" onclick={() => act((k) => command('run_to', k))}><span class="cm-ic">⇥</span>Run to here</button>
          <button role="menuitem" onclick={() => act(() => command('continue'))}><span class="cm-ic">⏵</span>Continue</button>
          <button role="menuitem" onclick={() => act(() => command('step_over'))}><span class="cm-ic">↷</span>Step over</button>
        {/if}
        {#if debug.runId}
          <hr />
          <button role="menuitem" onclick={() => act(showInRun)}><span class="cm-ic">☰</span>Show input and output in run</button>
        {/if}
      {/if}
      <hr />
      {#if m.from === 'code'}
        <button role="menuitem" onclick={() => act(showOnGraph)}><span class="cm-ic">◇</span>Show on graph</button>
      {:else}
        <button role="menuitem" onclick={() => act(gotoCode)}><span class="cm-ic">{'</>'}</span>Go to code</button>
      {/if}
      <button role="menuitem" onclick={() => act((k) => navigator.clipboard?.writeText(k))}><span class="cm-ic">⧉</span>Copy step key</button>
    {:else}
      {#if onrun}
        <button role="menuitem" onclick={() => act(() => onrun!(false))}><span class="cm-ic">▶</span>Run…</button>
        <button role="menuitem" onclick={() => act(() => onrun!(true))}><span class="cm-ic">🐞</span>Debug…</button>
      {/if}
      {#if debug?.paused}
        <button role="menuitem" onclick={() => act(() => command('continue'))}><span class="cm-ic">⏵</span>Continue the paused run</button>
      {/if}
      {#if debug?.breakpoints.length}
        <button role="menuitem" onclick={() => act(() => debug!.clear())}><span class="cm-ic bp">○</span>Clear all breakpoints ({debug.breakpoints.length})</button>
      {/if}
      {#if onrun || debug?.paused || debug?.breakpoints.length}<hr />{/if}
      <button role="menuitem" onclick={() => act(() => gotoCode())}><span class="cm-ic">{'</>'}</span>Show code</button>
    {/if}
  </div>
{/snippet}

<svelte:window onkeydown={onMenuKey} />
{#if menu}{@render contextMenu(menu)}{/if}

{#snippet formView()}
  {#if parsed === null && yamlSource}
    <div style="padding:12px; color:#dc2626; font-size:11px; font-family:var(--font-mono);">YAML parse error — check syntax in Code view</div>
  {:else if isAgentTemplate && parsed}
    <AgentTemplateEditor template={parsed} scrollTo={agentScrollTarget} onchange={onAgentFormChange} onopensub={onopensub} {readOnly} {configSeed} {onsection} />
  {:else}
    <SchemaForm value={yamlSource} onchange={onFormChange} focus={focusStep} />
  {/if}
{/snippet}

<div class="fep">
  {#if part === 'main'}
    <div class="fep-solo">
      <div class="fep-toolbar">
        <div class="fep-tabs">
          <button class="fep-tab" class:active={leftMode === 'graph'} onclick={() => leftMode = 'graph'}>
            <svg width="13" height="13" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5"><rect x="1" y="1" width="5" height="5" rx="1"/><rect x="10" y="10" width="5" height="5" rx="1"/><path d="M6 3.5h4M8 3.5v9"/></svg>
            Graph
          </button>
          <button class="fep-tab" class:active={leftMode === 'code'} onclick={() => leftMode = 'code'}>
            <svg width="13" height="13" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5"><path d="M5 3L1 8l4 5M11 3l4 5-4 5"/></svg>
            Code
          </button>
        </div>
        {@render toolbar?.()}
      </div>
      <div class="fep-pane-body part-main">
        {#if leftMode === 'graph'}{@render graphView(true)}{:else}{@render codeView()}{/if}
      </div>
    </div>
  {:else if part === 'form'}
    <div class="fep-part">{@render formView()}</div>
  {:else if part === 'ai'}
    <div class="fep-part">
      <DesignerChat kind={aiKind} name={aiName} readonly={readOnly} gettext={() => yamlSource} onapply={(t) => onCodeChange(t)} />
    </div>
  {:else if singleColumn}
    <!-- Narrow host: one tab bar, one view at a time, full width. -->
    <div class="fep-solo">
      <div class="fep-toolbar">
        <div class="fep-tabs">
          <button class="fep-tab" class:active={soloMode === 'graph'} onclick={() => soloMode = 'graph'}>
            <svg width="13" height="13" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5"><rect x="1" y="1" width="5" height="5" rx="1"/><rect x="10" y="10" width="5" height="5" rx="1"/><path d="M6 3.5h4M8 3.5v9"/></svg>
            Graph
          </button>
          <button class="fep-tab" class:active={soloMode === 'code'} onclick={() => soloMode = 'code'}>
            <svg width="13" height="13" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5"><path d="M5 3L1 8l4 5M11 3l4 5-4 5"/></svg>
            Code
          </button>
          <button class="fep-tab" class:active={soloMode === 'form'} onclick={() => soloMode = 'form'}>
            <svg width="13" height="13" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5"><rect x="2" y="2" width="12" height="12" rx="1.5"/><path d="M5 6h6M5 9h4"/></svg>
            Form
          </button>
        </div>
      </div>
      <div class="fep-pane-body">
        {#if soloMode === 'graph'}
          {@render graphView(false)}
        {:else if soloMode === 'code'}
          {@render codeView()}
        {:else}
          {@render formView()}
        {/if}
      </div>
    </div>
  {:else}
  <!-- Split: left + right -->
  <div class="fep-split" class:dragging bind:this={splitContainer} style="grid-template-columns: {splitPercent}% 6px 1fr;">
    <!-- LEFT: Graph or Code -->
    <div class="fep-left">
      <div class="fep-toolbar">
        <div class="fep-tabs">
          <button class="fep-tab" class:active={leftMode === 'graph'} onclick={() => leftMode = 'graph'}>
            <svg width="13" height="13" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5"><rect x="1" y="1" width="5" height="5" rx="1"/><rect x="10" y="10" width="5" height="5" rx="1"/><path d="M6 3.5h4M8 3.5v9"/></svg>
            Graph
          </button>
          <button class="fep-tab" class:active={leftMode === 'code'} onclick={() => leftMode = 'code'}>
            <svg width="13" height="13" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5"><path d="M5 3L1 8l4 5M11 3l4 5-4 5"/></svg>
            Code
          </button>
        </div>
      </div>
      <div class="fep-pane-body">
        {#if leftMode === 'graph'}
          {@render graphView(true)}
        {:else}
          {@render codeView()}
        {/if}
      </div>
    </div>

    <!-- Drag handle -->
    <div class="fep-divider" onmousedown={onDragStart} role="separator"></div>

    <!-- RIGHT: Form or AI -->
    <div class="fep-right">
      <div class="fep-toolbar">
        <div class="fep-tabs">
          <button class="fep-tab" class:active={rightMode === 'form'} onclick={() => rightMode = 'form'}>
            <svg width="13" height="13" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5"><rect x="2" y="2" width="12" height="12" rx="1.5"/><path d="M5 6h6M5 9h4"/></svg>
            Form
          </button>
          {#if showAi}
          <button class="fep-tab" class:active={rightMode === 'ai'} onclick={() => rightMode = 'ai'}>
            <svg width="13" height="13" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5"><path d="M2 3h12a1 1 0 011 1v7a1 1 0 01-1 1H6l-3 2v-2H2a1 1 0 01-1-1V4a1 1 0 011-1z"/></svg>
            AI
          </button>
          {/if}
        </div>
      </div>
      <div class="fep-pane-body">
        {#if rightMode === 'form'}
          {@render formView()}
        {:else}
          <!-- The real AI tab: a chat agent working on this document, applying
               changes to the editor (unsaved) for the user to Save. -->
          <DesignerChat kind={aiKind} name={aiName} readonly={readOnly}
            gettext={() => yamlSource}
            onapply={(t) => onCodeChange(t)} />
        {/if}
      </div>
    </div>
  </div>
  {/if}
</div>

<style>
  .fep { height: 100%; overflow: hidden; }
  .fep-part { height: 100%; overflow: auto; }
  .part-main { flex: 1; min-height: 0; overflow: hidden; }
  .fep-solo { display: flex; flex-direction: column; height: 100%; overflow: hidden; }
  /* The form is the only view that scrolls vertically in solo mode. */
  .fep-solo .fep-pane-body { flex: 1; min-height: 0; overflow: auto; }

  .fep-split { display: grid; height: 100%; overflow: hidden; }
  .fep-split.dragging { user-select: none; cursor: col-resize; }
  .fep-left, .fep-right { display: flex; flex-direction: column; overflow: hidden; }

  .fep-divider {
    width: 6px; cursor: col-resize; background: var(--border);
    position: relative; transition: background 0.15s;
  }
  .fep-divider:hover, .fep-split.dragging .fep-divider { background: var(--spec, #5530a8); }
  .fep-divider::after {
    content: ''; position: absolute; top: 50%; left: 50%;
    transform: translate(-50%, -50%);
    width: 2px; height: 24px; border-radius: 1px;
    background: var(--text-5); transition: background 0.15s;
  }
  .fep-divider:hover::after, .fep-split.dragging .fep-divider::after { background: white; }

  .fep-toolbar {
    position: relative;   /* a host's toolbar may centre its title on the bar */
    display: flex; align-items: center; gap: 8px;
    padding: 5px 10px; border-bottom: 1px solid var(--border);
    background: var(--bg-surface); flex-shrink: 0;
  }
  .fep-tabs { display: flex; gap: 2px; background: var(--bg-subtle); border-radius: var(--r-sm); padding: 2px; }
  .fep-tab {
    display: flex; align-items: center; gap: 4px;
    padding: 4px 10px; border: none; border-radius: 4px;
    background: none; color: var(--text-3); font-size: 11px;
    cursor: pointer; transition: all 0.12s;
  }
  .fep-tab.active { background: var(--bg-surface); color: var(--text-1); box-shadow: var(--shadow-sm); }
  .fep-tab:hover:not(.active) { color: var(--text-2); }

  .fep-pane-body { flex: 1; overflow: hidden; position: relative; }

  .gdrop { position: absolute; inset: 0; }
  .drop-label { position: absolute; top: 40px; left: 50%; transform: translateX(-50%); z-index: 8; pointer-events: none;
                padding: 3px 10px; border-radius: 10px; background: var(--spec, #5530a8); color: #fff; font-size: 11.5px; }
  .fep :global(.svelte-flow__node.drop-target) { box-shadow: 0 6px 0 -2px var(--spec, #5530a8); border-radius: 10px; }
  .fep :global(.svelte-flow__edge.drop-target path.svelte-flow__edge-path) { stroke: var(--spec, #5530a8) !important; stroke-width: 3px !important; }
  .code-split { display: grid; height: 100%; min-height: 0; overflow: hidden; }
  .code-split.dragging { user-select: none; cursor: col-resize; }
  .code-outline { min-width: 0; min-height: 0; overflow: hidden; border-right: 1px solid var(--border); }
  .code-div { cursor: col-resize; background: var(--bg-subtle); }
  .code-div:hover, .code-split.dragging .code-div { background: var(--spec, #5530a8); }
  .code-text { position: relative; min-width: 0; min-height: 0; height: 100%; }
  .outline-tg { position: absolute; top: 6px; right: 12px; z-index: 5; display: flex; align-items: center; gap: 4px; padding: 2px 8px; font-size: 11px;
                border: 1px solid var(--border); border-radius: 10px; background: var(--bg-surface); color: var(--text-3); cursor: pointer; }
  .outline-tg:hover { color: var(--spec); border-color: var(--spec); }
  .outline-tg b { color: var(--err, #b42318); }

  .fep-empty { display: flex; align-items: center; justify-content: center; height: 100%; color: var(--text-4); font-size: 13px; flex-direction: column; gap: 4px; }
  .fep-empty code { font-family: var(--font-mono); background: var(--bg-subtle); padding: 1px 5px; border-radius: 3px; font-size: 12px; }

  /* Workflow metadata bar */
  .wf-meta-bar {
    display: flex; align-items: center; gap: 6px;
    padding: 4px 10px; background: var(--bg-subtle); border-bottom: 1px solid var(--border);
    font-size: 10px; font-family: var(--font-mono); flex-shrink: 0; z-index: 5;
  }
  .wf-meta-format { font-weight: 600; color: var(--spec, #5530a8); text-transform: uppercase; letter-spacing: 0.05em; }
  .wf-meta-tag { color: var(--text-3); background: var(--bg-surface); padding: 1px 5px; border-radius: 3px; border: 1px solid var(--border); }
  .wf-meta-tag.ext { color: var(--checkpoint-ink, #8a5112); background: var(--checkpoint-bg, #fbf0e0); border-color: #f5dbb8; }

  /* Breakpoints and the shown run's state, on the graph's nodes */
  .fep :global(.svelte-flow__node.bp::before) {
    content: ''; position: absolute; top: -6px; left: -6px; z-index: 3; width: 12px; height: 12px; border-radius: 50%;
    background: var(--err, #d92d20); border: 2px solid var(--bg-surface, #fff); box-shadow: 0 0 0 1px var(--err, #d92d20);
  }
  .fep :global(.svelte-flow__node.bp-after::before) { background: linear-gradient(90deg, var(--bg-surface, #fff) 50%, var(--err, #d92d20) 50%); }
  .fep :global(.svelte-flow__node.bp-on_error::before) { border-radius: 2px; transform: rotate(45deg); }
  .fep :global(.svelte-flow__node[class*='st-']) { border-radius: 10px; }
  .fep :global(.svelte-flow__node.st-done) { box-shadow: 0 0 0 2px var(--ok, #15803d); }
  .fep :global(.svelte-flow__node.st-failed) { box-shadow: 0 0 0 2px var(--err, #d92d20); }
  .fep :global(.svelte-flow__node.st-waiting) { box-shadow: 0 0 0 2px var(--warn, #b45309); }
  .fep :global(.svelte-flow__node.st-running) { box-shadow: 0 0 0 3px var(--spec, #5530a8); animation: fep-pulse 1.2s ease-in-out infinite; }
  .fep :global(.svelte-flow__node.st-paused) { box-shadow: 0 0 0 3px var(--warn, #b45309), 0 0 0 7px rgba(180, 83, 9, 0.18); }
  .fep :global(.svelte-flow__node.here::after) {
    content: ''; position: absolute; top: 50%; right: -10px; transform: translate(100%, -50%); z-index: 3;
    font: 600 10.5px var(--font-mono); white-space: nowrap; padding: 2px 6px; border-radius: 3px; color: #fff; background: var(--warn, #b45309);
  }
  .fep :global(.svelte-flow__node.here.st-running::after) { background: var(--spec, #5530a8); }
  .fep :global(.svelte-flow__node.here.st-paused::after) { content: '⏸ paused here'; }
  .fep :global(.svelte-flow__node.here.st-running::after) { content: '● running'; }
  .fep :global(.svelte-flow__node.here.st-waiting::after) { content: '⏳ waiting'; }
  @keyframes fep-pulse { 50% { box-shadow: 0 0 0 6px rgba(85, 48, 168, 0.25); } }

  .run-legend { position: absolute; top: 34px; right: 10px; z-index: 5; display: flex; gap: 8px; padding: 4px 8px; font-size: 10.5px;
                background: var(--bg-surface); border: 1px solid var(--border); border-radius: var(--r-sm); color: var(--text-3); }
  .lg::before { content: ''; display: inline-block; width: 8px; height: 8px; border-radius: 50%; margin-right: 4px; vertical-align: -1px; }
  .lg.done::before { background: var(--ok, #15803d); } .lg.failed::before { background: var(--err, #d92d20); }
  .lg.running::before { background: var(--spec, #5530a8); } .lg.paused::before { background: var(--warn, #b45309); }
  .graph-note { position: absolute; bottom: 12px; left: 50%; transform: translateX(-50%); z-index: 6; max-width: 70%; border: 0; cursor: pointer;
                padding: 6px 12px; border-radius: var(--r-sm); background: var(--err-bg, #fde8e8); color: var(--err, #b42318); font-size: 12px; }

  /* Context menu */
  .cm-overlay { position: fixed; inset: 0; z-index: 90; }
  .cm { position: fixed; z-index: 91; min-width: 230px; max-width: 300px; display: flex; flex-direction: column; padding: 4px;
        background: var(--bg-surface); border: 1px solid var(--border); border-radius: var(--r-sm); box-shadow: var(--shadow-lg); font-size: 12.5px; }
  .cm-head { display: flex; flex-direction: column; gap: 1px; padding: 5px 8px 6px; border-bottom: 1px solid var(--border); margin-bottom: 3px; }
  .cm-head span { font-weight: 600; color: var(--text-1); }
  .cm-head code { font-family: var(--font-mono); font-size: 10.5px; color: var(--text-4); overflow: hidden; text-overflow: ellipsis; }
  .cm button { display: flex; align-items: center; gap: 8px; border: 0; background: transparent; padding: 5px 8px; border-radius: 4px;
               cursor: pointer; text-align: left; color: var(--text-1); font-size: 12.5px; }
  .cm button:hover { background: var(--bg-subtle); }
  .cm-ic { width: 16px; text-align: center; color: var(--text-3); font-size: 11px; }
  .cm-ic.bp { color: var(--err, #d92d20); }
  .cm hr { border: 0; border-top: 1px solid var(--border); margin: 3px 0; }

  /* AI chat placeholder */
  .fep-ai { display: flex; flex-direction: column; height: 100%; }
  .fep-ai-messages { flex: 1; overflow-y: auto; padding: 16px; }
  .fep-ai-msg { display: flex; gap: 10px; margin-bottom: 12px; }
  .fep-ai-msg.assistant { }
  .fep-ai-mark {
    width: 28px; height: 28px; border-radius: 6px; background: var(--bg-subtle);
    display: grid; place-items: center; font-family: var(--font-serif);
    font-size: 10px; font-weight: 600; color: var(--text-3); flex-shrink: 0;
  }
  .fep-ai-msg p { font-size: 13px; color: var(--text-2); line-height: 1.5; margin: 0; padding-top: 4px; }
  .fep-ai-input {
    padding: 10px 12px; border-top: 1px solid var(--border); background: var(--bg-surface);
  }
  .fep-ai-input input {
    width: 100%; padding: 8px 12px; border: 1px solid var(--border); border-radius: 8px;
    background: var(--bg-page); font-size: 12px; color: var(--text-1); outline: none;
  }
  .fep-ai-input input:disabled { opacity: 0.5; }
</style>
