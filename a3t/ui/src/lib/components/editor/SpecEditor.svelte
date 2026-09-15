<script lang="ts">
  import { SvelteFlow, Background, Controls, MiniMap, type Node, type Edge } from '@xyflow/svelte';
  import '@xyflow/svelte/dist/style.css';
  import StepNode from './StepNode.svelte';

  let { template }: { template: any } = $props();

  let mode = $state<'code' | 'visual'>('visual');
  let fullscreen = $state(false);
  let selectedNode = $state<Node | null>(null);

  const nodeTypes = { step: StepNode };

  // Build nodes/edges from template steps
  const steps = [
    { name: 'classify_url', call: 'mcp', server: 'ctx-scorer-mcp' },
    { name: 'win_history', call: 'aql', checkpoint: true },
    { name: 'match_strategy', call: 'aql', server: 'pattern match' },
    { name: 'compute_bid', call: 'llm', checkpoint: true },
    { name: 'emit_bid', call: 'emit', server: 'output_topic' },
  ];

  let nodes = $state<Node[]>(steps.map((s, i) => ({
    id: `step-${i}`,
    type: 'step',
    position: { x: 60 + i * 200, y: 80 },
    data: {
      label: s.name,
      callType: `call: ${s.call}`,
      server: s.server || '',
      checkpoint: s.checkpoint || false,
      stepNum: i + 1,
      _selected: false,
    },
  })));

  let edges = $state<Edge[]>(steps.slice(0, -1).map((_, i) => ({
    id: `e-${i}`,
    source: `step-${i}`,
    target: `step-${i + 1}`,
    type: 'smoothstep',
    animated: false,
  })));

  const specYaml = $derived(`# AgentTemplate · ${template.name} · v${template.activeVersion}
apiVersion: workflow.a3t.io/v1alpha1
kind: AgentTemplate
metadata:
  name: ${template.name}
  version: v${template.activeVersion}
spec:
  mode: ephemeral
  llm:
    primary: anthropic/claude-sonnet
  memory:
    types: [working, episodic, procedural]
  tools:
    mcp_servers:
      - ctx-scorer-mcp
      - tools-registry-mcp
      - brand-safety-mcp
  budget: 80ms

  do:
    - classify_url:
        call: mcp
        with: \\{ server: ctx-scorer-mcp \\}
    - win_history:
        call: aql
        checkpoint: true
    - match_strategy:
        call: aql
        with:
          pattern: \\{\\{ strategy_pattern \\}\\}
    - compute_bid:
        call: llm
        checkpoint: true
    - emit_bid:
        emit:
          topic: \\{\\{ output_topic \\}\\}

  on:
    error:
      catch: \\{ retry: 2, fallback: use-last-strategy \\}`);

  function handleNodeClick(_event: any, node: Node) {
    selectedNode = node;
    nodes = nodes.map(n => ({
      ...n,
      data: { ...n.data, _selected: n.id === node.id },
    }));
  }

  function handlePaneClick() {
    selectedNode = null;
    nodes = nodes.map(n => ({
      ...n,
      data: { ...n.data, _selected: false },
    }));
  }
</script>

<div class="spec-editor" class:fullscreen>
  <!-- Toolbar -->
  <div class="se-toolbar">
    <div class="se-tabs">
      <button class="se-tab" class:active={mode === 'visual'} onclick={() => mode = 'visual'} title="Visual editor">
        <svg width="15" height="15" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5"><circle cx="3.5" cy="3.5" r="1.5"/><circle cx="12.5" cy="3.5" r="1.5"/><circle cx="8" cy="12.5" r="1.5"/><path d="M5 3.5h6M4.5 5L7 11M11.5 5L9 11"/></svg>
      </button>
      <button class="se-tab" class:active={mode === 'code'} onclick={() => mode = 'code'} title="YAML code">
        <svg width="15" height="15" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5"><path d="M5 4L2 8l3 4M11 4l3 4-3 4M9 2l-2 12"/></svg>
      </button>
    </div>
    <span class="se-filename mono dim">{template.name}.yaml</span>
    <span class="se-badge">CNCF SW 1.0</span>
    <span style="flex: 1;"></span>
    <button class="se-btn" onclick={() => fullscreen = !fullscreen} title={fullscreen ? 'Exit fullscreen' : 'Fullscreen'}>
      {#if fullscreen}
        <svg width="14" height="14" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5"><path d="M6 2v4H2M14 10h-4v4M10 2v4h4M2 10h4v4"/></svg>
      {:else}
        <svg width="14" height="14" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5"><path d="M2 6V2h4M10 2h4v4M14 10v4h-4M6 14H2v-4"/></svg>
      {/if}
    </button>
  </div>

  <!-- Content -->
  <div class="se-content">
    {#if mode === 'code'}
      <div class="se-code">
        <pre>{specYaml}</pre>
      </div>
    {:else}
      <div class="se-visual">
        <div class="se-canvas" class:has-selection={!!selectedNode}>
          <SvelteFlow
            bind:nodes
            bind:edges
            {nodeTypes}
            fitView
            snapToGrid={true}
            snapGrid={[15, 15]}
            onnodeclick={handleNodeClick}
            onpaneclick={handlePaneClick}
            defaultEdgeOptions={{ type: 'smoothstep', animated: false }}
            elementsSelectable={true}
          >
            <Background gap={20} size={1} />
            <Controls />
            <MiniMap />
          </SvelteFlow>
        </div>

        <!-- Properties panel (when node selected) -->
        {#if selectedNode}
          <div class="se-props">
            <div class="sp-header">
              <h4>Step properties</h4>
              <button class="sp-close" onclick={handlePaneClick}>
                <svg width="12" height="12" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="2"><path d="M12 4L4 12M4 4l8 8"/></svg>
              </button>
            </div>
            <div class="sp-body">
              <div class="sp-field">
                <label>Name</label>
                <div class="sp-value">{selectedNode.data.label}</div>
              </div>
              <div class="sp-field">
                <label>Type</label>
                <div class="sp-value mono">{selectedNode.data.callType}</div>
              </div>
              {#if selectedNode.data.server}
                <div class="sp-field">
                  <label>Server / Target</label>
                  <div class="sp-value mono">{selectedNode.data.server}</div>
                </div>
              {/if}
              <div class="sp-field">
                <label>Checkpoint</label>
                <div class="sp-value">{selectedNode.data.checkpoint ? 'Yes — durability marker' : 'No'}</div>
              </div>
              <div class="sp-field">
                <label>Step #</label>
                <div class="sp-value">{selectedNode.data.stepNum}</div>
              </div>

              <div class="sp-section">CNCF SW Fields</div>
              <div class="sp-field">
                <label>call</label>
                <input type="text" value={String(selectedNode.data.callType).replace('call: ', '')} disabled />
              </div>
              <div class="sp-field">
                <label>with</label>
                <input type="text" value={selectedNode.data.server || '—'} disabled />
              </div>
              <div class="sp-field">
                <label>checkpoint</label>
                <input type="text" value={selectedNode.data.checkpoint ? 'true' : 'false'} disabled />
              </div>
              <div class="sp-field">
                <label>timeout</label>
                <input type="text" value="80ms (budget)" disabled />
              </div>
              <div class="sp-field">
                <label>retry</label>
                <input type="text" value="2" disabled />
              </div>
            </div>
          </div>
        {/if}
      </div>
    {/if}
  </div>
</div>

<style>
  .spec-editor {
    background: var(--bg-surface);
    border: 1px solid var(--border);
    border-radius: var(--r);
    overflow: hidden;
    box-shadow: var(--shadow-sm);
    display: flex;
    flex-direction: column;
  }
  .spec-editor.fullscreen {
    position: fixed;
    inset: 6px;
    z-index: 300;
    border-radius: var(--r-lg);
    box-shadow: var(--shadow-lg);
  }

  /* Toolbar */
  .se-toolbar {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 6px 12px;
    border-bottom: 1px solid var(--border);
    background: var(--bg-surface);
    flex-shrink: 0;
  }
  .se-tabs { display: flex; background: var(--bg-subtle); border-radius: 5px; padding: 2px; gap: 1px; }
  .se-tab {
    width: 30px; height: 28px;
    display: grid; place-items: center;
    border: none; background: none;
    border-radius: 4px; cursor: pointer;
    color: var(--text-3);
    transition: all 0.12s;
  }
  .se-tab:hover { color: var(--text-1); }
  .se-tab.active { background: var(--bg-surface); color: var(--text-1); box-shadow: var(--shadow-sm); }
  .se-filename { font-size: 12px; }
  .se-badge {
    font-family: var(--font-mono); font-size: 9px; color: var(--text-4);
    background: var(--bg-subtle); padding: 2px 6px; border-radius: 3px;
  }
  .se-btn {
    width: 28px; height: 28px;
    border: 1px solid var(--border); border-radius: var(--r-sm);
    background: var(--bg-surface); color: var(--text-3);
    display: grid; place-items: center; cursor: pointer;
  }
  .se-btn:hover { background: var(--bg-subtle); color: var(--text-1); }

  /* Content */
  .se-content { flex: 1; min-height: 400px; display: flex; flex-direction: column; }

  /* Code view */
  .se-code {
    flex: 1;
    background: var(--bg-ivory);
    font-family: var(--font-mono);
    font-size: 12px;
    line-height: 1.75;
    white-space: pre;
    padding: 20px 24px;
    overflow: auto;
    color: var(--text-1);
  }
  .se-code pre { margin: 0; font-family: inherit; }

  /* Visual view */
  .se-visual { flex: 1; display: flex; position: relative; }
  .se-canvas { flex: 1; }
  .se-canvas.has-selection { flex: 1; }

  /* Global overrides for xyflow */
  .se-canvas :global(.svelte-flow) { background: var(--bg-page) !important; }
  .se-canvas :global(.svelte-flow__background) { opacity: 0.5; }
  .se-canvas :global(.svelte-flow__edge-path) { stroke: var(--border-strong); stroke-width: 1.5; }
  .se-canvas :global(.svelte-flow__edge.selected .svelte-flow__edge-path) { stroke: var(--spec); }
  .se-canvas :global(.svelte-flow__controls) { box-shadow: var(--shadow-sm); border: 1px solid var(--border); border-radius: var(--r-sm); }
  .se-canvas :global(.svelte-flow__controls button) { background: var(--bg-surface); border-color: var(--border); color: var(--text-2); }
  .se-canvas :global(.svelte-flow__controls button:hover) { background: var(--bg-subtle); }
  .se-canvas :global(.svelte-flow__minimap) { border: 1px solid var(--border); border-radius: var(--r-sm); box-shadow: var(--shadow-sm); }

  /* Properties panel */
  .se-props {
    width: 260px;
    flex-shrink: 0;
    border-left: 1px solid var(--border);
    background: var(--bg-surface);
    display: flex;
    flex-direction: column;
    overflow-y: auto;
  }
  .sp-header {
    display: flex; align-items: center; justify-content: space-between;
    padding: 12px 14px; border-bottom: 1px solid var(--border);
  }
  .sp-header h4 { font-size: 13px; font-weight: 550; }
  .sp-close {
    width: 24px; height: 24px; border: none; background: none;
    color: var(--text-4); display: grid; place-items: center; cursor: pointer;
    border-radius: var(--r-sm);
  }
  .sp-close:hover { background: var(--bg-subtle); color: var(--text-1); }

  .sp-body { padding: 12px 14px; }
  .sp-field { margin-bottom: 10px; }
  .sp-field label {
    font-size: 10px; text-transform: uppercase; letter-spacing: 0.05em;
    color: var(--text-3); font-weight: 600; display: block; margin-bottom: 3px;
  }
  .sp-value { font-size: 13px; color: var(--text-1); }
  .sp-field input {
    width: 100%; padding: 5px 8px;
    border: 1px solid var(--border); border-radius: var(--r-sm);
    font-family: var(--font-mono); font-size: 11px; color: var(--text-2);
    background: var(--bg-subtle);
  }

  .sp-section {
    font-size: 10px; text-transform: uppercase; letter-spacing: 0.06em;
    color: var(--text-4); font-weight: 600;
    margin: 14px 0 8px; padding-top: 10px;
    border-top: 1px solid var(--divider);
  }
</style>
