<script lang="ts">
  import * as YAML from 'yaml';
  import { SvelteFlow, Background, Controls, MiniMap, type Node, type Edge } from '@xyflow/svelte';
  import '@xyflow/svelte/dist/style.css';
  import StepNode from './StepNode.svelte';
  import WorkflowNode from './WorkflowNode.svelte';
  import AgentNode from './AgentNode.svelte';
  import AgentSectionNode from './AgentSectionNode.svelte';
  import PortCardNode from './PortCardNode.svelte';
  import ResourceNode from './ResourceNode.svelte';
  import PipelineStepNode from './PipelineStepNode.svelte';
  import SubFlowNode from './SubFlowNode.svelte';
  import ControlNode from './ControlNode.svelte';
  import JoinNode from './JoinNode.svelte';
  import FlowMarkerNode from './FlowMarkerNode.svelte';
  import YamlEditor from './YamlEditor.svelte';
  import SchemaForm from './SchemaForm.svelte';
  import AgentTemplateEditor from './AgentTemplateEditor.svelte';
  import { parseWorkflowToGraph, isAgentTemplateYaml } from './workflowParser';

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
  } = $props();

  const nodeTypes = {
    step: StepNode, workflow: WorkflowNode, agent: AgentNode,
    agentSection: AgentSectionNode, portCard: PortCardNode,
    resource: ResourceNode, pipeline: PipelineStepNode, subflow: SubFlowNode,
    control: ControlNode, join: JoinNode, flowMarker: FlowMarkerNode,
  };

  import { onMount, onDestroy } from 'svelte';
  import DesignerChat from '$lib/components/editor/DesignerChat.svelte';

  // ── Internal state ──
  let leftMode = $state<'graph' | 'code'>('graph');
  let rightMode = $state<'form' | 'ai'>('form');
  // Single-column hosts drive one selection across all three views.
  let soloMode = $state<'graph' | 'code' | 'form'>('graph');
  // Graph/Code report themselves; the form delegates to its own sub-tabs.
  $effect(() => { if (soloMode !== 'form') onsection?.(soloMode); });
  // With the AI tab hidden, never leave the right pane on a tab that is gone.
  $effect(() => { if (!showAi && rightMode === 'ai') rightMode = 'form'; });
  let yamlSource = $state(initialYaml);
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
  let edges = $derived.by<Edge[]>(() => baseGraph?.edges ?? []);

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
  {#if nodes.length > 0}
    {#if baseGraph?.meta}
      <div class="wf-meta-bar">
        <span class="wf-meta-format">{baseGraph.meta.format === 'cncf-dsl' ? 'CNCF DSL' : 'FlowR'}</span>
        {#if baseGraph.meta.dsl}<span class="wf-meta-tag">v{baseGraph.meta.dsl}</span>{/if}
        <span class="wf-meta-tag">{baseGraph.meta.taskCount} tasks</span>
        {#if baseGraph.meta.hasExtensions}<span class="wf-meta-tag ext">{baseGraph.meta.extensionCount} ext</span>{/if}
      </div>
    {/if}
    <SvelteFlow {nodes} {edges} {nodeTypes} fitView onnodeclick={handleNodeClick}
      nodesDraggable={!readOnly} nodesConnectable={!readOnly} elementsSelectable={!readOnly}>
      <Background />
      <Controls />
      {#if showMiniMap}<MiniMap />{/if}
    </SvelteFlow>
  {:else}
    <div class="fep-empty">No steps to visualize — add <code>steps:</code> or <code>do:</code> tasks</div>
  {/if}
{/snippet}

{#snippet codeView()}
  <YamlEditor value={yamlSource} onchange={onCodeChange} {readOnly} />
{/snippet}

{#snippet formView()}
  {#if parsed === null && yamlSource}
    <div style="padding:12px; color:#dc2626; font-size:11px; font-family:var(--font-mono);">YAML parse error — check syntax in Code view</div>
  {:else if isAgentTemplate && parsed}
    <AgentTemplateEditor template={parsed} scrollTo={agentScrollTarget} onchange={onAgentFormChange} onopensub={onopensub} {readOnly} {configSeed} {onsection} />
  {:else}
    <SchemaForm value={yamlSource} onchange={onFormChange} />
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

  .fep-pane-body { flex: 1; overflow: hidden; }

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
