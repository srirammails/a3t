<script lang="ts">
  import * as YAML from 'yaml';
  import YamlEditor from '$lib/components/editor/YamlEditor.svelte';
  import { mcpCall } from '$lib/services/api';

  let mode = $state<'yaml' | 'jq' | 'shell'>('yaml');
  let source = $state(`# CNCF Serverless Workflow — edit and execute
namespace: default
name: console-test
version: "1.0.0"
steps:
  - name: hello
    type: set
    config:
      message: "Hello from console"
`);
  let output = $state('');
  let running = $state(false);
  let history = $state<{ input: string; output: string; ts: number; ok: boolean }[]>([]);

  const EXAMPLES: Record<string, { label: string; code: string }[]> = {
    yaml: [
      { label: 'Set variable', code: `namespace: default\nname: quick-set\nversion: "1.0.0"\nsteps:\n  - name: greet\n    type: set\n    config:\n      greeting: "Hello, world"` },
      { label: 'HTTP call', code: `namespace: default\nname: quick-http\nversion: "1.0.0"\nsteps:\n  - name: fetch\n    type: call:http\n    config:\n      method: GET\n      endpoint:\n        uri: https://httpbin.org/get\n      timeout: 10` },
      { label: 'Switch', code: `namespace: default\nname: quick-switch\nversion: "1.0.0"\ninput_schema:\n  type: object\n  properties:\n    status:\n      type: string\nsteps:\n  - name: route\n    type: switch\n    config:\n      - when: '$\{ .status == "ok" }'\n        then:\n          - name: ok-path\n            type: set\n            config:\n              result: "All good"\n      - default:\n          - name: fallback\n            type: set\n            config:\n              result: "Unknown status"` },
      { label: 'For loop', code: `namespace: default\nname: quick-for\nversion: "1.0.0"\nsteps:\n  - name: init\n    type: set\n    config:\n      total: 0\n    export.as: "."\n  - name: sum-items\n    type: for\n    config:\n      each: item\n      in: '$\{ [1, 2, 3, 4, 5] }'\n      do:\n        - name: add\n          type: set\n          config:\n            total: '$\{ .total + $item }'\n    export.as: "."` },
    ],
    jq: [
      { label: 'Identity', code: '.' },
      { label: 'Select fields', code: '.items[] | {name, value}' },
      { label: 'Map + filter', code: '[.items[] | select(.value > 10) | .name]' },
      { label: 'Group by', code: 'group_by(.category) | map({key: .[0].category, count: length})' },
    ],
    shell: [
      { label: 'List flows', code: 'curl -s http://localhost:8000/api/v1/flows/default | jq .' },
      { label: 'Health check', code: 'curl -s http://localhost:8000/health' },
      { label: 'Flow status', code: 'curl -s http://localhost:8000/api/v1/flows/default/console-test' },
    ],
  };

  async function execute() {
    running = true;
    output = '';
    try {
      if (mode === 'yaml') {
        const parsed = YAML.parse(source);
        if (!parsed?.name) { output = 'Error: workflow must have a name'; return; }
        const res = await mcpCall('create_flow', { yaml: source });
        if (res.ok) {
          output = `Flow deployed: ${parsed.namespace ?? 'default'}/${parsed.name}\n\nExecuting...`;
          const execRes = await mcpCall('execute_flow', {
            namespace: parsed.namespace ?? 'default',
            name: parsed.name,
            input: {},
          });
          output = JSON.stringify(execRes.data ?? execRes, null, 2);
          history = [{ input: source.slice(0, 80) + '…', output: output.slice(0, 200), ts: Date.now(), ok: true }, ...history.slice(0, 19)];
        } else {
          output = `Deploy failed: ${JSON.stringify(res)}`;
          history = [{ input: source.slice(0, 80), output: output.slice(0, 200), ts: Date.now(), ok: false }, ...history.slice(0, 19)];
        }
      } else if (mode === 'jq') {
        output = `jq expression: ${source}\n\n(jq evaluation requires a running flowr backend with expression endpoint)`;
        history = [{ input: source, output: output.slice(0, 200), ts: Date.now(), ok: true }, ...history.slice(0, 19)];
      } else {
        output = `Shell: ${source}\n\n(Shell execution requires a running flowr backend with shell endpoint)`;
        history = [{ input: source, output: output.slice(0, 200), ts: Date.now(), ok: true }, ...history.slice(0, 19)];
      }
    } catch (e: any) {
      output = `Error: ${e.message ?? e}`;
      history = [{ input: source.slice(0, 80), output: output.slice(0, 200), ts: Date.now(), ok: false }, ...history.slice(0, 19)];
    } finally {
      running = false;
    }
  }

  function loadExample(code: string) {
    source = code;
    output = '';
  }
</script>

<div class="console-page">
  <div class="screen-header">
    <div class="breadcrumb">workbench / console</div>
    <div class="screen-title-row">
      <h1 class="screen-title">Console Editor</h1>
      <div class="mode-tabs">
        <button class="mode-tab" class:active={mode === 'yaml'} onclick={() => { mode = 'yaml'; source = EXAMPLES.yaml[0].code; output = ''; }}>
          <svg width="12" height="12" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5"><path d="M5 3L1 8l4 5M11 3l4 5-4 5"/></svg>
          Workflow
        </button>
        <button class="mode-tab" class:active={mode === 'jq'} onclick={() => { mode = 'jq'; source = '.'; output = ''; }}>
          <svg width="12" height="12" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5"><path d="M4 6l3 2-3 2"/><path d="M8 10h4"/></svg>
          jq
        </button>
        <button class="mode-tab" class:active={mode === 'shell'} onclick={() => { mode = 'shell'; source = ''; output = ''; }}>
          <svg width="12" height="12" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5"><rect x="1" y="2" width="14" height="12" rx="1.5"/><path d="M4 6l2 2-2 2M8 10h4"/></svg>
          Shell
        </button>
      </div>
    </div>
  </div>

  <div class="console-body">
    <!-- Examples sidebar -->
    <div class="examples-panel">
      <h3 class="panel-title">Examples</h3>
      {#each EXAMPLES[mode] ?? [] as ex}
        <button class="example-btn" onclick={() => loadExample(ex.code)}>
          {ex.label}
        </button>
      {/each}

      {#if history.length > 0}
        <h3 class="panel-title" style="margin-top: 16px;">History</h3>
        {#each history as h}
          <button class="history-btn" class:err={!h.ok} onclick={() => { source = h.input; output = h.output; }}>
            <span class="hist-dot" class:ok={h.ok}></span>
            <span class="hist-text">{h.input.slice(0, 40)}</span>
            <span class="hist-time">{new Date(h.ts).toLocaleTimeString()}</span>
          </button>
        {/each}
      {/if}
    </div>

    <!-- Editor + Output split -->
    <div class="editor-split">
      <div class="editor-pane">
        <div class="pane-toolbar">
          <span class="pane-label">Input</span>
          <span style="flex:1;"></span>
          <button class="btn spec sm" disabled={running} onclick={execute}>
            {#if running}
              Running…
            {:else}
              <svg width="12" height="12" viewBox="0 0 16 16" fill="currentColor"><polygon points="3,2 13,8 3,14"/></svg>
              Execute
            {/if}
          </button>
        </div>
        <div class="pane-body">
          {#if mode === 'yaml'}
            <YamlEditor value={source} onchange={(v) => source = v} />
          {:else}
            <textarea class="code-input" bind:value={source} spellcheck="false" placeholder={mode === 'jq' ? 'Enter jq expression…' : 'Enter shell command…'}></textarea>
          {/if}
        </div>
      </div>

      <div class="output-pane">
        <div class="pane-toolbar">
          <span class="pane-label">Output</span>
          {#if output}<button class="btn ghost sm" onclick={() => output = ''}>Clear</button>{/if}
        </div>
        <div class="pane-body">
          <pre class="output-pre">{output || 'No output yet. Click Execute to run.'}</pre>
        </div>
      </div>
    </div>
  </div>
</div>

<style>
  .console-page { display: flex; flex-direction: column; height: 100%; overflow: hidden; }

  .screen-header { padding: 22px 40px 14px; border-bottom: 1px solid var(--border); background: var(--bg-page); flex-shrink: 0; }
  .breadcrumb { font-size: 12px; color: var(--text-3); margin-bottom: 10px; font-family: var(--font-mono); }
  .screen-title-row { display: flex; align-items: flex-end; gap: 18px; }
  .screen-title { font-family: var(--font-serif); font-size: 28px; font-weight: 500; letter-spacing: -0.02em; }

  .mode-tabs { display: flex; gap: 2px; background: var(--bg-subtle); border-radius: var(--r-sm); padding: 2px; margin-left: auto; }
  .mode-tab {
    display: flex; align-items: center; gap: 4px;
    padding: 5px 12px; border: none; border-radius: 4px;
    background: none; color: var(--text-3); font-size: 12px;
    cursor: pointer; transition: all 0.12s;
  }
  .mode-tab.active { background: var(--bg-surface); color: var(--text-1); box-shadow: var(--shadow-sm); }
  .mode-tab:hover:not(.active) { color: var(--text-2); }

  .console-body { flex: 1; display: grid; grid-template-columns: 180px 1fr; overflow: hidden; }

  .examples-panel {
    padding: 12px; border-right: 1px solid var(--border);
    overflow-y: auto; background: var(--bg-surface);
  }
  .panel-title {
    font-family: var(--font-mono); font-size: 10px; font-weight: 600;
    text-transform: uppercase; letter-spacing: 0.06em;
    color: var(--text-4); margin-bottom: 8px;
  }
  .example-btn {
    display: block; width: 100%;
    padding: 6px 10px; margin-bottom: 3px;
    border: 1px solid var(--border); border-radius: var(--r-sm);
    background: var(--bg-page); color: var(--text-2);
    font-size: 12px; text-align: left; cursor: pointer;
    transition: all 0.1s;
  }
  .example-btn:hover { background: var(--bg-subtle); color: var(--text-1); border-color: var(--spec); }

  .history-btn {
    display: flex; align-items: center; gap: 6px;
    width: 100%; padding: 4px 8px; margin-bottom: 2px;
    border: none; border-radius: 3px;
    background: none; color: var(--text-3); font-size: 10px;
    cursor: pointer; text-align: left;
  }
  .history-btn:hover { background: var(--bg-subtle); }
  .hist-dot { width: 6px; height: 6px; border-radius: 50%; background: var(--err); flex-shrink: 0; }
  .hist-dot.ok { background: var(--ok); }
  .hist-text { flex: 1; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-family: var(--font-mono); }
  .hist-time { color: var(--text-5); font-family: var(--font-mono); flex-shrink: 0; }

  .editor-split { display: grid; grid-template-rows: 1fr 1fr; overflow: hidden; }

  .editor-pane, .output-pane { display: flex; flex-direction: column; overflow: hidden; }
  .output-pane { border-top: 1px solid var(--border); }

  .pane-toolbar {
    display: flex; align-items: center; gap: 8px;
    padding: 6px 12px; border-bottom: 1px solid var(--border);
    background: var(--bg-surface); flex-shrink: 0;
  }
  .pane-label {
    font-family: var(--font-mono); font-size: 10px; font-weight: 600;
    text-transform: uppercase; letter-spacing: 0.05em; color: var(--text-4);
  }

  .pane-body { flex: 1; overflow: hidden; }

  .code-input {
    width: 100%; height: 100%; resize: none; border: none;
    padding: 12px 16px; font-family: var(--font-mono); font-size: 13px;
    color: var(--text-1); background: var(--bg-page); outline: none;
    line-height: 1.6;
  }

  .output-pre {
    margin: 0; padding: 12px 16px;
    font-family: var(--font-mono); font-size: 12px;
    color: var(--text-2); line-height: 1.5;
    white-space: pre-wrap; word-break: break-word;
    overflow-y: auto; height: 100%;
  }
</style>
