<script lang="ts">
  import { mcpCall, resources as resourcesApi } from '$lib/services/api';
  import SqlEditor from '$lib/components/editor/SqlEditor.svelte';
  import { onMount } from 'svelte';

  // ── Connection state ──
  interface DbResource { name: string; namespace: string; type: string; }

  let dbResources = $state<DbResource[]>([]);
  let activeResource = $state<DbResource | null>(null);
  let loadingResources = $state(false);

  // ── Query state ──
  let query = $state('');
  let running = $state(false);
  let resultRows = $state<Record<string, any>[]>([]);
  let resultColumns = $state<string[]>([]);
  let resultRaw = $state('');
  let resultError = $state('');
  let resultMs = $state(0);
  let resultMode = $state<'table' | 'raw'>('table');

  // ── Saved queries ──
  let savedQueries = $state<{ name: string; query: string; resource: string }[]>([]);

  // ── History ──
  let history = $state<{ resource: string; query: string; ts: number; ok: boolean; ms: number }[]>([]);

  // ── Layout state ──
  let panelOpen = $state(false);
  let panelTab = $state<'connections' | 'saved' | 'history'>('connections');
  let splitPercent = $state(65);
  let dragging = $state(false);
  let splitContainer: HTMLDivElement;

  // DB type metadata
  const DB_META: Record<string, { label: string; color: string; lang: string }> = {
    postgres:  { label: 'PostgreSQL', color: '#2563eb', lang: 'SQL' },
    mysql:     { label: 'MySQL',      color: '#00758f', lang: 'SQL' },
    lancedb:   { label: 'LanceDB',    color: '#e97319', lang: 'SQL' },
    redis:     { label: 'Dragonfly',  color: '#dc2626', lang: 'Redis' },
    clawdb:    { label: 'ClawDB',     color: '#7c3aed', lang: 'AQL' },
    qdrant:    { label: 'Qdrant',     color: '#c4243b', lang: 'AQL' },
  };

  const DB_TYPES = new Set(Object.keys(DB_META));

  function getMeta(type: string) {
    return DB_META[type] ?? { label: type, color: 'var(--text-3)', lang: 'Query' };
  }

  // ── Draggable split ──
  function onDragStart(e: MouseEvent) {
    dragging = true;
    e.preventDefault();
    const onMove = (ev: MouseEvent) => {
      if (!splitContainer) return;
      const rect = splitContainer.getBoundingClientRect();
      const pct = ((ev.clientX - rect.left) / rect.width) * 100;
      splitPercent = Math.max(30, Math.min(80, pct));
    };
    const onUp = () => {
      dragging = false;
      window.removeEventListener('mousemove', onMove);
      window.removeEventListener('mouseup', onUp);
    };
    window.addEventListener('mousemove', onMove);
    window.addEventListener('mouseup', onUp);
  }

  // ── Load DB resources ──
  onMount(async () => {
    loadingResources = true;
    try {
      for (const ns of ['a3t', 'default']) {
        const res = await resourcesApi.list(ns);
        const items = res?.data?.resources ?? res?.data ?? [];
        if (Array.isArray(items)) {
          for (const r of items) {
            const type = r.type_ref ?? r.type ?? '';
            if (DB_TYPES.has(type)) {
              dbResources.push({ name: r.name, namespace: r.namespace ?? ns, type });
            }
          }
        }
      }
      dbResources = [...dbResources];
      if (dbResources.length > 0) activeResource = dbResources[0];
    } finally {
      loadingResources = false;
    }

    // Load saved queries from localStorage
    try {
      const stored = localStorage.getItem('db-console-saved');
      if (stored) savedQueries = JSON.parse(stored);
    } catch {}
  });

  // ── Execute query ──
  async function execute() {
    if (!query.trim() || !activeResource) return;
    running = true;
    resultRows = [];
    resultColumns = [];
    resultRaw = '';
    resultError = '';
    const start = performance.now();
    try {
      let res: any;
      const r = activeResource;

      if (r.type === 'postgres' || r.type === 'mysql') {
        res = await mcpCall('call_sql', { query: query.trim(), resource: r.name, namespace: r.namespace });
      } else if (r.type === 'redis') {
        const parts = query.trim().split(/\s+/);
        const cmd = parts[0].toUpperCase();
        const args = parts.slice(1);
        res = await mcpCall('call_redis', { operation: cmd.toLowerCase(), key: args[0] ?? '', value: args[1] ?? '', args, resource: r.name, namespace: r.namespace });
      } else {
        res = await mcpCall('query_resource', { query: query.trim(), resource: r.name, namespace: r.namespace });
      }

      resultMs = Math.round(performance.now() - start);
      const data = res?.data ?? res;

      if (Array.isArray(data) && data.length > 0 && typeof data[0] === 'object') {
        resultRows = data;
        resultColumns = [...new Set(data.flatMap(r => Object.keys(r)))];
        resultRaw = JSON.stringify(data, null, 2);
        resultMode = 'table';
      } else if (typeof data === 'string') {
        try {
          const parsed = JSON.parse(data);
          if (Array.isArray(parsed) && parsed.length > 0 && typeof parsed[0] === 'object') {
            resultRows = parsed;
            resultColumns = [...new Set(parsed.flatMap((r: any) => Object.keys(r)))];
            resultRaw = data;
            resultMode = 'table';
          } else {
            resultRaw = typeof parsed === 'object' ? JSON.stringify(parsed, null, 2) : data;
            resultMode = 'raw';
          }
        } catch {
          resultRaw = data;
          resultMode = 'raw';
        }
      } else {
        resultRaw = JSON.stringify(data, null, 2);
        resultMode = 'raw';
      }

      history = [{ resource: `${r.namespace}/${r.name}`, query: query.trim(), ts: Date.now(), ok: true, ms: resultMs }, ...history.slice(0, 49)];
    } catch (e: any) {
      resultMs = Math.round(performance.now() - start);
      resultError = e.message ?? String(e);
      resultMode = 'raw';
      history = [{ resource: `${activeResource.namespace}/${activeResource.name}`, query: query.trim(), ts: Date.now(), ok: false, ms: resultMs }, ...history.slice(0, 49)];
    } finally {
      running = false;
    }
  }

  function saveQuery() {
    if (!query.trim()) return;
    const name = prompt('Save query as:');
    if (!name) return;
    const resource = activeResource ? `${activeResource.namespace}/${activeResource.name}` : '';
    savedQueries = [{ name, query: query.trim(), resource }, ...savedQueries];
    try { localStorage.setItem('db-console-saved', JSON.stringify(savedQueries)); } catch {}
  }

  function deleteSavedQuery(idx: number) {
    savedQueries = savedQueries.filter((_, i) => i !== idx);
    try { localStorage.setItem('db-console-saved', JSON.stringify(savedQueries)); } catch {}
  }

  function selectConnection(r: DbResource) {
    activeResource = r;
    panelOpen = false;
  }

  function formatTime(ts: number) {
    const d = new Date(ts);
    return d.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' });
  }

  function formatCell(val: any): string {
    if (val === null || val === undefined) return 'NULL';
    if (typeof val === 'object') return JSON.stringify(val);
    return String(val);
  }

  let hasResult = $derived(resultRows.length > 0 || resultRaw || resultError);
  let activeLang = $derived(activeResource ? getMeta(activeResource.type).lang : 'Query');
  let sqlDialect = $derived.by<'postgres' | 'mysql' | 'standard'>(() => {
    if (!activeResource) return 'standard';
    if (activeResource.type === 'postgres') return 'postgres';
    if (activeResource.type === 'mysql') return 'mysql';
    return 'standard';
  });
</script>

<div class="console">
  <!-- Toolbar -->
  <div class="console-toolbar">
    <button class="file-menu-btn" onclick={() => panelOpen = !panelOpen}>
      <svg width="16" height="16" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"><path d="M2 4h12M2 8h12M2 12h12"/></svg>
      {#if activeResource}
        <span class="conn-dot-sm" style="background: {getMeta(activeResource.type).color};"></span>
        <span class="file-menu-label">{activeResource.name}</span>
      {:else}
        <span class="file-menu-label">Connections</span>
      {/if}
    </button>

    <span class="query-lang">{activeLang}</span>
    <span class="query-hint">&#8984;+Enter</span>

    <span style="flex: 1;"></span>

    <button class="btn ghost sm" onclick={saveQuery} disabled={!query.trim()}>
      <svg width="12" height="12" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5"><path d="M3 1h8l3 3v9a2 2 0 01-2 2H3a2 2 0 01-2-2V3a2 2 0 012-2z"/><path d="M5 1v4h6V1M4 9h8"/></svg>
      Save
    </button>
    <button class="btn ghost sm" onclick={() => { query = ''; resultRows = []; resultColumns = []; resultRaw = ''; resultError = ''; }}>Clear</button>
    <button class="btn spec sm" disabled={running || !query.trim() || !activeResource} onclick={execute}>
      {#if running}
        Running...
      {:else}
        <svg width="10" height="10" viewBox="0 0 16 16" fill="currentColor"><polygon points="3,2 13,8 3,14"/></svg>
        Execute
      {/if}
    </button>
  </div>

  <div class="console-content">
    <!-- Left slide panel: connections + saved + history -->
    {#if panelOpen}
      <!-- svelte-ignore a11y_no_static_element_interactions -->
      <div class="panel-overlay" onclick={() => panelOpen = false} onkeydown={() => {}}></div>
      <div class="side-panel">
        <div class="panel-header">
          <div class="panel-tabs">
            <button class="ptab" class:active={panelTab === 'connections'} onclick={() => panelTab = 'connections'}>Connections</button>
            <button class="ptab" class:active={panelTab === 'saved'} onclick={() => panelTab = 'saved'}>Saved</button>
            <button class="ptab" class:active={panelTab === 'history'} onclick={() => panelTab = 'history'}>History</button>
          </div>
          <button class="panel-close" onclick={() => panelOpen = false}>
            <svg width="14" height="14" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"><path d="M4 4l8 8M12 4l-8 8"/></svg>
          </button>
        </div>

        <div class="panel-body">
          {#if panelTab === 'connections'}
            {#if loadingResources}
              <div class="panel-empty">Loading resources...</div>
            {:else if dbResources.length === 0}
              <div class="panel-empty">No DB resources found</div>
            {:else}
              {#each dbResources as r}
                {@const meta = getMeta(r.type)}
                <button
                  class="panel-item"
                  class:active={activeResource?.name === r.name && activeResource?.namespace === r.namespace}
                  onclick={() => selectConnection(r)}
                >
                  <span class="conn-dot" style="background: {meta.color};"></span>
                  <div class="panel-item-info">
                    <span class="panel-item-name">{r.name}</span>
                    <span class="panel-item-meta">{meta.label} &middot; {r.namespace}</span>
                  </div>
                </button>
              {/each}
            {/if}

          {:else if panelTab === 'saved'}
            {#if savedQueries.length === 0}
              <div class="panel-empty">No saved queries</div>
            {:else}
              {#each savedQueries as sq, i}
                <div class="panel-item saved-item">
                  <button class="saved-item-main" onclick={() => { query = sq.query; panelOpen = false; }}>
                    <svg width="13" height="13" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.3"><path d="M3 2h6l4 4v8a1 1 0 01-1 1H3a1 1 0 01-1-1V3a1 1 0 011-1z"/><path d="M9 2v4h4"/></svg>
                    <div class="panel-item-info">
                      <span class="panel-item-name">{sq.name}</span>
                      <span class="panel-item-meta">{sq.query.slice(0, 50)}{sq.query.length > 50 ? '...' : ''}</span>
                    </div>
                  </button>
                  <button class="saved-delete" onclick={() => deleteSavedQuery(i)}>
                    <svg width="12" height="12" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5"><path d="M4 4l8 8M12 4l-8 8"/></svg>
                  </button>
                </div>
              {/each}
            {/if}

          {:else}
            {#if history.length === 0}
              <div class="panel-empty">No queries yet</div>
            {:else}
              {#each history.slice(0, 30) as h}
                <button class="panel-item hist-item" class:err={!h.ok} onclick={() => { query = h.query; panelOpen = false; }}>
                  <span class="hist-dot" class:ok={h.ok}></span>
                  <div class="panel-item-info">
                    <span class="panel-item-name">{h.query.slice(0, 45)}{h.query.length > 45 ? '...' : ''}</span>
                    <span class="panel-item-meta">{h.resource} &middot; {h.ms}ms &middot; {formatTime(h.ts)}</span>
                  </div>
                </button>
              {/each}
            {/if}
          {/if}
        </div>
      </div>
    {/if}

    <!-- Split: Editor+Results | AI -->
    <div class="split-root" class:dragging bind:this={splitContainer} style="grid-template-columns: {splitPercent}% 6px 1fr;">
      <!-- LEFT: SQL editor + results -->
      <div class="split-left">
        <!-- Query editor -->
        <div class="query-section">
          <SqlEditor
            value={query}
            dialect={sqlDialect}
            placeholder={activeResource ? `Enter ${activeLang} query...` : 'Select a connection first'}
            onchange={(val) => { query = val; }}
            onexecute={execute}
          />
        </div>

        <!-- Results -->
        <div class="result-section">
          <div class="result-toolbar">
            <span class="toolbar-label">Result</span>
            {#if hasResult && !resultError}
              <div class="result-tabs">
                <button class="rtab" class:active={resultMode === 'table'} disabled={resultColumns.length === 0} onclick={() => resultMode = 'table'}>Table</button>
                <button class="rtab" class:active={resultMode === 'raw'} onclick={() => resultMode = 'raw'}>Raw</button>
              </div>
              {#if resultRows.length > 0}
                <span class="result-meta">{resultRows.length} rows</span>
              {/if}
              <span class="result-meta">{resultMs}ms</span>
            {/if}
          </div>

          <div class="result-body">
            {#if resultError}
              <div class="result-error">
                <svg width="14" height="14" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5"><circle cx="8" cy="8" r="6"/><path d="M8 5v3M8 10.5v.5"/></svg>
                {resultError}
              </div>
            {:else if !hasResult}
              <div class="result-empty">Execute a query to see results</div>
            {:else if resultMode === 'table' && resultColumns.length > 0}
              <div class="table-wrap">
                <table class="result-table">
                  <thead>
                    <tr>
                      {#each resultColumns as col}
                        <th>{col}</th>
                      {/each}
                    </tr>
                  </thead>
                  <tbody>
                    {#each resultRows as row}
                      <tr>
                        {#each resultColumns as col}
                          <td class:null-val={row[col] === null || row[col] === undefined}>{formatCell(row[col])}</td>
                        {/each}
                      </tr>
                    {/each}
                  </tbody>
                </table>
              </div>
            {:else}
              <pre class="result-raw">{resultRaw}</pre>
            {/if}
          </div>
        </div>
      </div>

      <!-- Drag handle -->
      <div class="split-divider" onmousedown={onDragStart} role="separator"></div>

      <!-- RIGHT: AI chat -->
      <div class="split-right">
        <div class="ai-toolbar">
          <div class="ai-tabs">
            <button class="ai-tab active">
              <svg width="13" height="13" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5"><path d="M2 3h12a1 1 0 011 1v7a1 1 0 01-1 1H6l-3 2v-2H2a1 1 0 01-1-1V4a1 1 0 011-1z"/></svg>
              AI
            </button>
          </div>
        </div>
        <div class="ai-body">
          <div class="ai-messages">
            <div class="ai-msg assistant">
              <span class="ai-mark">A3</span>
              <p>I can help you write queries. Describe what data you need or paste an error to debug.</p>
            </div>
          </div>
          <div class="ai-input">
            <textarea placeholder="Ask about your data..." rows="2"></textarea>
          </div>
        </div>
      </div>
    </div>
  </div>
</div>

<style>
  .console { display: flex; flex-direction: column; height: 100%; overflow: hidden; }

  /* ── Toolbar ── */
  .console-toolbar {
    display: flex; align-items: center; gap: 8px;
    padding: 8px 16px; border-bottom: 1px solid var(--border);
    background: var(--bg-surface); flex-shrink: 0;
  }

  .file-menu-btn {
    display: flex; align-items: center; gap: 6px;
    padding: 5px 10px; border: 1px solid var(--border); border-radius: var(--r-sm);
    background: var(--bg-surface); color: var(--text-2); font-size: 12px;
    font-family: var(--font-mono); cursor: pointer; transition: all 0.12s;
  }
  .file-menu-btn:hover { background: var(--bg-subtle); color: var(--text-1); border-color: var(--border-strong); }
  .file-menu-label { max-width: 180px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .conn-dot-sm { width: 7px; height: 7px; border-radius: 50%; flex-shrink: 0; }

  .query-lang {
    font-family: var(--font-mono); font-size: 10px; font-weight: 600;
    text-transform: uppercase; letter-spacing: 0.05em; color: var(--spec);
    background: color-mix(in srgb, var(--spec) 8%, transparent);
    padding: 2px 6px; border-radius: 3px;
  }
  .query-hint { font-size: 10px; color: var(--text-5); font-family: var(--font-mono); }

  /* ── Content area ── */
  .console-content { flex: 1; overflow: hidden; position: relative; display: flex; }

  /* ── Left slide panel ── */
  .panel-overlay {
    position: absolute; inset: 0; z-index: 10;
    background: rgba(0, 0, 0, 0.08);
  }

  .side-panel {
    position: absolute; top: 0; left: 0; bottom: 0; z-index: 11;
    width: 300px; display: flex; flex-direction: column;
    background: var(--bg-surface); border-right: 1px solid var(--border);
    box-shadow: 4px 0 20px rgba(0, 0, 0, 0.06);
    animation: panelSlideIn 0.2s cubic-bezier(0.16, 1, 0.3, 1);
  }
  @keyframes panelSlideIn { from { transform: translateX(-100%); opacity: 0; } to { transform: translateX(0); opacity: 1; } }

  .panel-header {
    display: flex; align-items: center; justify-content: space-between;
    padding: 8px 10px 8px 10px; border-bottom: 1px solid var(--border); gap: 8px;
  }
  .panel-tabs { display: flex; gap: 2px; background: var(--bg-subtle); border-radius: var(--r-sm); padding: 2px; }
  .ptab {
    padding: 4px 10px; border: none; border-radius: 4px;
    background: none; color: var(--text-3); font-size: 11px;
    cursor: pointer; transition: all 0.12s;
  }
  .ptab.active { background: var(--bg-surface); color: var(--text-1); box-shadow: var(--shadow-sm); }
  .ptab:hover:not(.active) { color: var(--text-2); }

  .panel-close {
    width: 28px; height: 28px; display: flex; align-items: center; justify-content: center;
    border: 1px solid var(--border); border-radius: 50%; background: none;
    color: var(--text-3); cursor: pointer; transition: all 0.12s; flex-shrink: 0;
  }
  .panel-close:hover { background: var(--bg-subtle); color: var(--text-1); border-color: var(--border-strong); }

  .panel-body { flex: 1; overflow-y: auto; padding: 8px; }
  .panel-empty { padding: 20px 10px; text-align: center; color: var(--text-4); font-size: 12px; }

  .panel-item {
    display: flex; align-items: center; gap: 8px; width: 100%;
    padding: 7px 8px; margin-bottom: 2px; border: none; border-radius: var(--r-sm);
    background: none; color: var(--text-2); cursor: pointer;
    text-align: left; transition: all 0.1s;
  }
  .panel-item:hover { background: var(--bg-subtle); color: var(--text-1); }
  .panel-item.active { background: var(--bg-subtle); color: var(--text-1); }

  .conn-dot { width: 8px; height: 8px; border-radius: 50%; flex-shrink: 0; }
  .panel-item-info { display: flex; flex-direction: column; min-width: 0; flex: 1; }
  .panel-item-name { font-size: 12px; font-weight: 500; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .panel-item-meta { font-size: 10px; color: var(--text-4); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }

  .saved-item { padding: 0; cursor: default; }
  .saved-item-main {
    display: flex; align-items: center; gap: 8px; flex: 1; min-width: 0;
    padding: 7px 0 7px 8px; border: none; background: none;
    color: var(--text-2); cursor: pointer; text-align: left;
  }
  .saved-item-main svg { flex-shrink: 0; color: var(--text-4); }
  .saved-delete {
    width: 28px; height: 28px; display: flex; align-items: center; justify-content: center;
    border: none; background: none; color: var(--text-5); cursor: pointer;
    border-radius: 4px; flex-shrink: 0; transition: all 0.1s;
  }
  .saved-delete:hover { background: var(--bg-subtle); color: var(--err); }

  .hist-item.err .panel-item-name { color: var(--err); }
  .hist-dot { width: 5px; height: 5px; border-radius: 50%; background: var(--err); flex-shrink: 0; margin-top: 5px; }
  .hist-dot.ok { background: var(--ok); }

  /* ── Split layout (matches FlowEditorPane) ── */
  .split-root { display: grid; height: 100%; overflow: hidden; flex: 1; }
  .split-root.dragging { user-select: none; cursor: col-resize; }

  .split-left { display: flex; flex-direction: column; overflow: hidden; }
  .split-right { display: flex; flex-direction: column; overflow: hidden; }

  .split-divider {
    width: 6px; cursor: col-resize; background: var(--border);
    position: relative; transition: background 0.15s;
  }
  .split-divider:hover, .split-root.dragging .split-divider { background: var(--spec, #5530a8); }
  .split-divider::after {
    content: ''; position: absolute; top: 50%; left: 50%;
    transform: translate(-50%, -50%);
    width: 2px; height: 24px; border-radius: 1px;
    background: var(--text-5); transition: background 0.15s;
  }
  .split-divider:hover::after, .split-root.dragging .split-divider::after { background: white; }

  /* ── Query editor ── */
  .query-section {
    height: 180px; min-height: 100px; flex-shrink: 0;
    border-bottom: 1px solid var(--border);
    background: var(--bg-surface);
  }

  /* ── Results ── */
  .result-section { flex: 1; display: flex; flex-direction: column; overflow: hidden; }
  .result-toolbar {
    display: flex; align-items: center; gap: 8px;
    padding: 6px 12px; border-bottom: 1px solid var(--divider);
    background: var(--bg-surface); flex-shrink: 0;
  }
  .toolbar-label {
    font-family: var(--font-mono); font-size: 10px; font-weight: 600;
    text-transform: uppercase; letter-spacing: 0.05em; color: var(--text-4);
  }
  .result-tabs { display: flex; gap: 2px; background: var(--bg-subtle); border-radius: 4px; padding: 2px; }
  .rtab {
    padding: 2px 8px; border: none; border-radius: 3px;
    background: none; color: var(--text-3); font-size: 11px;
    cursor: pointer; transition: all 0.1s;
  }
  .rtab.active { background: var(--bg-surface); color: var(--text-1); box-shadow: var(--shadow-sm); }
  .rtab:disabled { opacity: 0.4; cursor: default; }
  .result-meta { font-family: var(--font-mono); font-size: 10px; color: var(--text-5); }

  .result-body { flex: 1; overflow: auto; }
  .result-empty { padding: 40px 16px; text-align: center; color: var(--text-4); font-size: 13px; }
  .result-error {
    display: flex; align-items: flex-start; gap: 8px;
    padding: 12px 16px; margin: 12px; border-radius: var(--r-sm);
    background: color-mix(in srgb, var(--err) 6%, transparent);
    color: var(--err); font-family: var(--font-mono); font-size: 12px;
    line-height: 1.5;
  }
  .result-error svg { flex-shrink: 0; margin-top: 2px; }
  .result-raw {
    margin: 0; padding: 12px 16px;
    font-family: var(--font-mono); font-size: 12px;
    color: var(--text-2); line-height: 1.5;
    white-space: pre-wrap; word-break: break-word;
  }

  /* Table */
  .table-wrap { overflow: auto; height: 100%; }
  .result-table {
    width: 100%; border-collapse: collapse;
    font-family: var(--font-mono); font-size: 12px;
  }
  .result-table th {
    position: sticky; top: 0; z-index: 1;
    padding: 6px 12px; text-align: left;
    background: var(--bg-surface); color: var(--text-3);
    font-weight: 600; font-size: 11px; text-transform: uppercase;
    letter-spacing: 0.03em; border-bottom: 1px solid var(--border);
    white-space: nowrap;
  }
  .result-table td {
    padding: 5px 12px; border-bottom: 1px solid var(--divider);
    color: var(--text-2); max-width: 300px; overflow: hidden;
    text-overflow: ellipsis; white-space: nowrap;
  }
  .result-table tr:hover td { background: var(--bg-subtle); }
  .null-val { color: var(--text-5); font-style: italic; }

  /* ── AI panel (matches FlowEditorPane) ── */
  .ai-toolbar {
    display: flex; align-items: center; gap: 8px;
    padding: 5px 10px; border-bottom: 1px solid var(--border);
    background: var(--bg-surface); flex-shrink: 0;
  }
  .ai-tabs { display: flex; gap: 2px; background: var(--bg-subtle); border-radius: var(--r-sm); padding: 2px; }
  .ai-tab {
    display: flex; align-items: center; gap: 4px;
    padding: 4px 10px; border: none; border-radius: 4px;
    background: none; color: var(--text-3); font-size: 11px;
    cursor: pointer; transition: all 0.12s;
  }
  .ai-tab.active { background: var(--bg-surface); color: var(--text-1); box-shadow: var(--shadow-sm); }

  .ai-body { flex: 1; display: flex; flex-direction: column; overflow: hidden; }
  .ai-messages { flex: 1; overflow-y: auto; padding: 16px; }
  .ai-msg { display: flex; gap: 10px; margin-bottom: 12px; }
  .ai-mark {
    width: 28px; height: 28px; border-radius: 6px; background: var(--bg-subtle);
    display: grid; place-items: center; font-family: var(--font-serif);
    font-size: 10px; font-weight: 600; color: var(--text-3); flex-shrink: 0;
  }
  .ai-msg p { font-size: 13px; color: var(--text-2); line-height: 1.5; margin: 0; padding-top: 4px; }
  .ai-input {
    padding: 10px 12px; border-top: 1px solid var(--border); background: var(--bg-surface);
  }
  .ai-input textarea {
    width: 100%; padding: 8px 12px; border: 1px solid var(--border); border-radius: 8px;
    background: var(--bg-page); font-size: 12px; color: var(--text-1); outline: none;
    font-family: var(--font-sans); line-height: 1.5; resize: vertical;
    min-height: 36px; max-height: 200px;
  }
  .ai-input textarea:focus { border-color: var(--spec); }
</style>
