<script lang="ts">
  import { onMount } from 'svelte';
  import KGPointCloud from '$lib/components/editor/KGPointCloud.svelte';
  import { knowledge, type Triple } from '$lib/services/a3t';

  // ── State ──
  let activeTab = $state<'graph' | 'vectors' | 'upload' | 'train'>('graph');
  let searchQuery = $state('');
  let selectedNode = $state<any>(null);
  let focusNodeId = $state('');

  // ── Entity-type colors (assigned as types are discovered from the backend) ──
  const PALETTE = ['#3b82f6', '#8b5cf6', '#10b981', '#f59e0b', '#ef4444', '#ec4899', '#06b6d4', '#84cc16', '#f97316', '#14b8a6'];
  let typeColors = $state<Record<string, string>>({});
  function colorFor(type: string): string {
    if (!type) return '#64748b';
    if (!typeColors[type]) {
      const idx = Object.keys(typeColors).length % PALETTE.length;
      typeColors = { ...typeColors, [type]: PALETTE[idx] };
    }
    return typeColors[type];
  }

  // ── Graph state (loaded from a3t-knowledge / call:rotate) ──
  let stats = $state<any>(null);
  let cloudPoints = $state<{ id: string; name: string; type: string; x: number; y: number; z: number }[]>([]);
  let cloudEdges = $state<{ source: number; target: number; opacity: number; relation: string }[]>([]);
  let relationList = $state<{ id: string; name: string; triples: number }[]>([]);
  let loading = $state(false);
  let errorMsg = $state('');

  let nodeCount = $derived(stats?.entities ?? 0);
  let edgeCount = $derived(stats?.triples ?? 0);
  let relCount = $derived(stats?.relations ?? 0);
  let dim = $derived(stats?.dim ?? 0);
  let typeKeys = $derived(stats?.types ? Object.keys(stats.types) : []);

  async function loadGraph() {
    loading = true;
    errorMsg = '';
    try {
      const s = await knowledge.stats({ sample: 5000 });
      stats = s;
      for (const t of Object.keys(s?.types ?? {})) colorFor(t);
      cloudPoints = (s?.points ?? []).map((p: any) => ({ id: p.id, name: p.name, type: p.type, x: p.x, y: p.y, z: p.z }));
      cloudEdges = (s?.edges ?? []).map((e: any) => ({ source: e.source, target: e.target, relation: e.relation ?? 'related_to', opacity: 0.18 }));
      const r = await knowledge.relations();
      relationList = r?.relations ?? [];
    } catch (e: any) {
      errorMsg = e?.message ?? String(e);
    } finally {
      loading = false;
    }
  }
  onMount(loadGraph);

  // Relation names for the Quick-Add dropdown (falls back to a starter set when empty)
  let RELATION_TYPES = $derived(
    relationList.length ? relationList.map(r => r.name) : ['IsA', 'PartOf', 'UsedFor', 'RelatedTo', 'HasA', 'Causes', 'AtLocation', 'Synonym']
  );

  // ── Search (over the loaded sample) ──
  let searchResults = $derived.by(() => {
    if (!searchQuery.trim()) return [];
    const q = searchQuery.toLowerCase();
    return cloudPoints.filter(p => p.name.toLowerCase().includes(q)).slice(0, 50);
  });

  // Inspect a node — fetch its real connections via lookup (both directions).
  async function handleSelect(node: any) {
    selectedNode = { ...node, connections: [], loadingConns: true };
    try {
      const [outRes, inRes] = await Promise.all([
        knowledge.lookup({ head: node.name, limit: 50 }),
        knowledge.lookup({ tail: node.name, limit: 50 }),
      ]);
      const out = (outRes?.triples ?? []).map((t: any) => ({ name: t.tail, type: t.tail_type, relation: t.relation, dir: 'out' }));
      const inc = (inRes?.triples ?? []).map((t: any) => ({ name: t.head, type: t.head_type, relation: t.relation, dir: 'in' }));
      const seen = new Set<string>();
      const connections = [...out, ...inc].filter(c => {
        const k = `${c.dir}:${c.relation}:${c.name}`;
        if (seen.has(k)) return false;
        seen.add(k);
        return true;
      }).slice(0, 40);
      selectedNode = { ...node, connections, loadingConns: false };
    } catch (e: any) {
      selectedNode = { ...node, connections: [], loadingConns: false, connError: e?.message ?? String(e) };
    }
  }

  function focusOn(id: string) {
    focusNodeId = '';
    requestAnimationFrame(() => { focusNodeId = id; });
    const p = cloudPoints.find(n => n.id === id);
    if (p) handleSelect(p);
  }

  // ── Vectors tab ──
  let vectorRows = $state<any[]>([]);
  let vectorsLoaded = $state(false);
  async function loadVectors() {
    try {
      const res = await knowledge.entities({ limit: 200 });
      vectorRows = (res?.entities ?? []).map((e: any) => ({
        id: e.id,
        name: e.name,
        type: e.type,
        dims: e.dims,
        norm: Number(e.norm ?? 0).toFixed(4),
        preview: (e.preview ?? []).map((v: number) => Number(v).toFixed(3)),
      }));
      vectorsLoaded = true;
    } catch (e: any) {
      errorMsg = e?.message ?? String(e);
    }
  }
  $effect(() => {
    if (activeTab === 'vectors' && !vectorsLoaded) loadVectors();
  });

  // Vector search → nearest neighbours of the matched entity (real RotatE space).
  let vecSearchQuery = $state('');
  let vecSearchResults = $state<any[]>([]);
  let vecSearching = $state(false);
  async function searchVectors() {
    const q = vecSearchQuery.trim();
    if (!q) { vecSearchResults = []; return; }
    vecSearching = true;
    try {
      // resolve the closest entity by name, then fetch its neighbours
      const match = vectorRows.find(v => v.name.toLowerCase() === q.toLowerCase())
        ?? vectorRows.find(v => v.name.toLowerCase().includes(q.toLowerCase()));
      const entity = match?.name ?? q;
      const res = await knowledge.neighbors(entity, 25);
      vecSearchResults = (res?.results ?? []).map((r: any) => ({
        name: r.name,
        type: r.type,
        similarity: Number(r.similarity ?? 0).toFixed(4),
      }));
    } catch (e: any) {
      errorMsg = e?.message ?? String(e);
      vecSearchResults = [];
    } finally {
      vecSearching = false;
    }
  }

  // ── Upload tab ──
  let uploadFormat = $state<'csv' | 'json' | 'ttl'>('csv');
  let uploadText = $state('');
  let uploadStatus = $state('');
  let uploading = $state(false);

  // Quick-add form
  let qaHead = $state('');
  let qaRelation = $state('');
  let qaTail = $state('');
  let qaConfidence = $state(0.9);
  let qaStatus = $state('');

  function parseTriples(text: string, fmt: 'csv' | 'json' | 'ttl'): Triple[] {
    const out: Triple[] = [];
    if (fmt === 'csv') {
      for (const line of text.split('\n')) {
        const row = line.trim();
        if (!row || row.startsWith('#')) continue;
        const parts = row.split(',').map(s => s.trim());
        if (parts.length < 3) continue;
        out.push({ head: parts[0], relation: parts[1], tail: parts[2], weight: parts[3] ? Number(parts[3]) : 1.0 });
      }
    } else if (fmt === 'json') {
      const data = JSON.parse(text);
      const items = Array.isArray(data) ? data : [data];
      for (const it of items) {
        // Either an explicit triple, or JSON-LD-ish { "@id", "@type", <relation>: [tails] }
        if (it.head && it.relation && it.tail) {
          out.push({ head: it.head, relation: it.relation, tail: it.tail, weight: it.weight ?? 1.0, head_type: it.head_type, tail_type: it.tail_type });
          continue;
        }
        const head = it['@id'] ?? it.id;
        const htype = it['@type'] ?? it.type;
        if (!head) continue;
        for (const [k, v] of Object.entries(it)) {
          if (k.startsWith('@') || k === 'id' || k === 'type') continue;
          const tails = Array.isArray(v) ? v : [v];
          for (const tl of tails) out.push({ head, relation: k, tail: String(tl), weight: 1.0, head_type: htype });
        }
      }
    } else {
      // Minimal Turtle: ":Head :relation :Tail ." (one statement per line)
      for (const line of text.split('\n')) {
        const m = line.trim().match(/^:?(\S+)\s+:?(\S+)\s+:?(\S+)\s*\.?$/);
        if (m && m[2] !== 'a') out.push({ head: m[1], relation: m[2], tail: m[3], weight: 1.0 });
      }
    }
    return out;
  }

  async function importTriples() {
    const triples = parseTriples(uploadText, uploadFormat);
    if (!triples.length) { uploadStatus = 'No valid triples parsed.'; return; }
    uploading = true;
    uploadStatus = `Ingesting ${triples.length} triples…`;
    try {
      const res = await knowledge.ingest(triples, dim || 256);
      uploadStatus = `Ingested ${res?.ingested ?? 0} new triples (total: ${res?.triples ?? '?'}, entities: ${res?.entities ?? '?'}). Re-train to update embeddings.`;
      await loadGraph();
    } catch (e: any) {
      uploadStatus = `Error: ${e?.message ?? String(e)}`;
    } finally {
      uploading = false;
    }
  }

  async function quickAdd() {
    if (!qaHead.trim() || !qaRelation.trim() || !qaTail.trim()) { qaStatus = 'Head, relation and tail are required.'; return; }
    qaStatus = 'Adding…';
    try {
      const res = await knowledge.ingest([{ head: qaHead.trim(), relation: qaRelation.trim(), tail: qaTail.trim(), weight: qaConfidence }], dim || 256);
      qaStatus = `Added. Total triples: ${res?.triples ?? '?'}.`;
      qaHead = ''; qaTail = '';
      await loadGraph();
    } catch (e: any) {
      qaStatus = `Error: ${e?.message ?? String(e)}`;
    }
  }

  // ── Train tab ──
  let trainEpochs = $state(200);
  let trainLr = $state(0.001);
  let trainGamma = $state(12.0);
  let trainNeg = $state(8);
  let trainStatus = $state<'idle' | 'running' | 'done'>('idle');
  let trainLog = $state('');
  let trainResult = $state<any>(null);

  async function startTraining() {
    trainStatus = 'running';
    trainResult = null;
    trainLog = `[${new Date().toLocaleTimeString()}] Starting RotatE training…\n  epochs=${trainEpochs}, lr=${trainLr}, gamma=${trainGamma}, neg_samples=${trainNeg}\n  entities: ${nodeCount.toLocaleString()}, triples: ${edgeCount.toLocaleString()}, dim: ${dim}\n  (training runs server-side via call:rotate — this may take a moment)\n`;
    try {
      const res = await knowledge.train({ epochs: trainEpochs, lr: trainLr, gamma: trainGamma, neg_samples: trainNeg });
      trainResult = res;
      trainStatus = 'done';
      trainLog += `[${new Date().toLocaleTimeString()}] Training complete.\n`;
      trainLog += `  final loss: ${Number(res?.loss ?? 0).toFixed(4)}\n`;
      trainLog += `  MRR: ${Number(res?.mrr ?? 0).toFixed(4)} · Hits@10: ${Number(res?.hits_at_10 ?? 0).toFixed(4)}\n`;
      trainLog += `  Embeddings written to kg_entities / kg_relations (${res?.entities ?? 0} entities, ${res?.dim ?? dim}d)\n`;
      await loadGraph();
      vectorsLoaded = false;
    } catch (e: any) {
      trainStatus = 'idle';
      trainLog += `[${new Date().toLocaleTimeString()}] Training failed: ${e?.message ?? String(e)}\n`;
    }
  }
</script>

<div class="kg-page">
  <div class="screen-header">
    <div class="breadcrumb">workbench <span class="sep">/</span> knowledge</div>
    <div class="screen-title-row">
      <h1 class="screen-title">Knowledge Graph</h1>
      <div class="screen-stats">
        <span class="kstat"><strong>{nodeCount.toLocaleString()}</strong> entities</span>
        <span class="kstat"><strong>{edgeCount.toLocaleString()}</strong> triples</span>
        <span class="kstat"><strong>{relCount.toLocaleString()}</strong> relations</span>
        <span class="kstat"><strong>{typeKeys.length}</strong> types</span>
        <span class="kstat"><strong>{dim || '—'}</strong>d RotatE</span>
      </div>
    </div>
    <div class="tab-bar">
      <button class="tab" class:active={activeTab === 'graph'} onclick={() => activeTab = 'graph'}>
        <svg width="14" height="14" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5"><circle cx="4" cy="4" r="2"/><circle cx="12" cy="4" r="2"/><circle cx="8" cy="13" r="2"/><path d="M5.5 5.5L7 11M10.5 5.5L9 11M6 4h4"/></svg>
        3D Graph
      </button>
      <button class="tab" class:active={activeTab === 'vectors'} onclick={() => activeTab = 'vectors'}>
        <svg width="14" height="14" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5"><path d="M2 3h12M2 7h12M2 11h12M2 15h8"/><circle cx="13" cy="13" r="2"/></svg>
        Vectors
      </button>
      <button class="tab" class:active={activeTab === 'upload'} onclick={() => activeTab = 'upload'}>
        <svg width="14" height="14" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5"><path d="M8 10V2M8 2L5 5M8 2l3 3"/><path d="M2 10v3a1 1 0 001 1h10a1 1 0 001-1v-3"/></svg>
        Upload
      </button>
      <button class="tab" class:active={activeTab === 'train'} onclick={() => activeTab = 'train'}>
        <svg width="14" height="14" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5"><path d="M2 14l3-6 3 4 2-3 4 5"/><circle cx="12" cy="3" r="2"/></svg>
        Train
      </button>
    </div>
  </div>

  <div class="kg-body">
    {#if activeTab === 'graph'}
      <!-- ═══ 3D GRAPH TAB ═══ -->
      <div class="graph-layout">
        <div class="graph-canvas">
          <!-- Overlay toolbar -->
          <div class="graph-hud">
            <div class="hud-search">
              <svg width="14" height="14" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5"><circle cx="7" cy="7" r="4.5"/><path d="M10.5 10.5L14 14"/></svg>
              <input type="text" placeholder="Search nodes…" bind:value={searchQuery}
                onkeydown={(e) => { if (e.key === 'Enter' && searchResults.length > 0) focusOn(searchResults[0].id); }} />
              {#if searchResults.length > 0}
                <div class="hud-results">
                  {#each searchResults.slice(0, 12) as r}
                    <button class="hud-result" onclick={() => focusOn(r.id)}>
                      <span class="hr-dot" style="background: {colorFor(r.type)};"></span>
                      <span class="hr-name">{r.name}</span>
                      <span class="hr-type">{r.type}</span>
                    </button>
                  {/each}
                </div>
              {/if}
            </div>
            <div class="hud-legend">
              {#each typeKeys as type}
                <span class="legend-item"><span class="legend-dot" style="background: {colorFor(type)};"></span>{type}</span>
              {/each}
            </div>
            <div class="hud-scale">
              <button class="scale-select" onclick={loadGraph} disabled={loading} title="Reload from LanceDB">
                {loading ? 'Loading…' : `↻ ${stats?.sampled ?? 0}/${nodeCount} shown`}
              </button>
            </div>
          </div>

          <div class="hud-controls">
            <span>Drag: rotate · Right-drag: pan · Scroll: fly into depth · WASD: move · Q/E: up/down · Shift: fast · Click: inspect</span>
          </div>

          {#if errorMsg}
            <div class="graph-empty"><p>⚠ {errorMsg}</p><button class="btn ghost sm" onclick={loadGraph}>Retry</button></div>
          {:else if !loading && cloudPoints.length === 0}
            <div class="graph-empty">
              <p>No knowledge graph yet.</p>
              <p class="detail-hint">Add ConceptNet triples in the <strong>Upload</strong> tab, then <strong>Train</strong> RotatE embeddings.</p>
            </div>
          {/if}

          <KGPointCloud
            points={cloudPoints}
            edges={cloudEdges}
            typeColors={typeColors}
            onselect={handleSelect}
            focusId={focusNodeId}
          />
        </div>

        <!-- Detail panel -->
        <div class="detail-panel">
          {#if selectedNode}
            {@const sc = colorFor(selectedNode.type)}
            <div class="detail-header" style="border-left-color: {sc};">
              <span class="detail-icon" style="background: {sc};">{(selectedNode.type || '?').slice(0, 1).toUpperCase()}</span>
              <div>
                <div class="detail-name">{selectedNode.name}</div>
                <div class="detail-type">{selectedNode.type}</div>
              </div>
              <button class="close-btn" onclick={() => selectedNode = null}>&times;</button>
            </div>

            <div class="detail-section">
              <h4>Position (3D projection)</h4>
              <div class="pos-grid">
                <span class="pos-axis">x</span><span class="pos-val">{selectedNode.x?.toFixed(2)}</span>
                <span class="pos-axis">y</span><span class="pos-val">{selectedNode.y?.toFixed(2)}</span>
                <span class="pos-axis">z</span><span class="pos-val">{selectedNode.z?.toFixed(2)}</span>
              </div>
            </div>

            <div class="detail-section">
              <h4>Connections <span class="conn-count">{selectedNode.loadingConns ? '…' : (selectedNode.connections?.length ?? 0)}</span></h4>
              <div class="conn-list">
                {#each selectedNode.connections ?? [] as conn}
                  <button class="conn-row" onclick={() => {
                    const p = cloudPoints.find(x => x.name === conn.name);
                    if (p) focusOn(p.id);
                  }}>
                    <span class="conn-dir" class:out={conn.dir === 'out'}>{conn.dir === 'out' ? '→' : '←'}</span>
                    <span class="conn-rel">{conn.relation}</span>
                    <span class="conn-entity">{conn.name}</span>
                    <span class="conn-type-dot" style="background: {colorFor(conn.type)};"></span>
                  </button>
                {/each}
                {#if selectedNode.loadingConns}
                  <span class="no-conn">Loading connections…</span>
                {:else if selectedNode.connError}
                  <span class="no-conn">⚠ {selectedNode.connError}</span>
                {:else if !selectedNode.connections?.length}
                  <span class="no-conn">No direct connections (only in the sampled view, or none stored)</span>
                {/if}
              </div>
            </div>

            <div class="detail-section">
              <h4>Position (3D RotatE projection)</h4>
              <p class="plane-desc">Coordinates are a deterministic random projection of this entity's {dim || '?'}-d RotatE complex embedding. Nearby points share relational structure.</p>
            </div>
          {:else}
            <div class="detail-empty">
              <svg width="40" height="40" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="0.8" opacity="0.25"><circle cx="4" cy="4" r="2"/><circle cx="12" cy="4" r="2"/><circle cx="8" cy="13" r="2"/><path d="M5.5 5.5L7 11M10.5 5.5L9 11M6 4h4"/></svg>
              <p>Click a node or search to inspect</p>
              <p class="detail-hint">{nodeCount.toLocaleString()} entities · {dim || '—'}d RotatE</p>
            </div>
          {/if}
        </div>
      </div>

    {:else if activeTab === 'vectors'}
      <!-- ═══ VECTORS TAB ═══ -->
      <div class="vectors-layout">
        <div class="vec-toolbar">
          <span class="vec-title">kg_entities</span>
          <span class="vec-meta">{nodeCount.toLocaleString()} rows · {dim || '—'}d · RotatE (re‖im)</span>
          <div class="vec-search">
            <input type="text" placeholder="Nearest neighbors of…" bind:value={vecSearchQuery}
              onkeydown={(e) => { if (e.key === 'Enter') searchVectors(); }} />
          </div>
        </div>

        {#if vecSearching}
          <div class="vec-search-results"><h4>Searching…</h4></div>
        {:else if vecSearchResults.length > 0}
          <div class="vec-search-results">
            <h4>Nearest neighbors for "{vecSearchQuery}" (embedding distance)</h4>
            <div class="vec-table">
              <div class="vec-row header">
                <span class="vc-name">Entity</span>
                <span class="vc-type">Type</span>
                <span class="vc-sim">Similarity</span>
              </div>
              {#each vecSearchResults as r}
                <div class="vec-row">
                  <span class="vc-name">{r.name}</span>
                  <span class="vc-type"><span class="type-badge" style="background: {colorFor(r.type)};">{r.type}</span></span>
                  <span class="vc-sim"><span class="sim-bar" style="width: {parseFloat(r.similarity) * 100}%;"></span>{r.similarity}</span>
                </div>
              {/each}
            </div>
          </div>
        {/if}

        <div class="vec-table-wrap">
          <div class="vec-table">
            <div class="vec-row header">
              <span class="vc-id">ID</span>
              <span class="vc-name">Entity</span>
              <span class="vc-type">Type</span>
              <span class="vc-dims">Dims</span>
              <span class="vc-norm">‖v‖</span>
              <span class="vc-vec">Vector (first 8 of re)</span>
            </div>
            {#if vectorRows.length === 0}
              <div class="vec-row"><span class="vc-name">{vectorsLoaded ? 'No entities yet — ingest triples first.' : 'Loading…'}</span></div>
            {/if}
            {#each vectorRows as row}
              <div class="vec-row">
                <span class="vc-id">{row.id}</span>
                <span class="vc-name">{row.name}</span>
                <span class="vc-type"><span class="type-badge" style="background: {colorFor(row.type)};">{row.type}</span></span>
                <span class="vc-dims">{row.dims}</span>
                <span class="vc-norm">{row.norm}</span>
                <span class="vc-vec">[{row.preview.join(', ')}…]</span>
              </div>
            {/each}
          </div>
        </div>
      </div>

    {:else if activeTab === 'upload'}
      <!-- ═══ UPLOAD TAB ═══ -->
      <div class="upload-layout">
        <div class="upload-panel">
          <h3>Import Knowledge Graph</h3>
          <p class="upload-desc">Add entities and triples. Supports CSV triples, JSON-LD, or Turtle (TTL).</p>

          <div class="format-selector">
            <button class="fmt-btn" class:active={uploadFormat === 'csv'} onclick={() => uploadFormat = 'csv'}>CSV Triples</button>
            <button class="fmt-btn" class:active={uploadFormat === 'json'} onclick={() => uploadFormat = 'json'}>JSON-LD</button>
            <button class="fmt-btn" class:active={uploadFormat === 'ttl'} onclick={() => uploadFormat = 'ttl'}>Turtle</button>
          </div>

          <div class="upload-hint">
            {#if uploadFormat === 'csv'}
              <code>head,relation,tail[,confidence]</code>
              <pre>Rust,IsA,ProgrammingLanguage,1.0{'\n'}Dog,IsA,Animal,2.5{'\n'}Wheel,PartOf,Car,1.0</pre>
            {:else if uploadFormat === 'json'}
              <pre>{`[{"@id": "Rust", "@type": "Technology", "IsA": ["ProgrammingLanguage"]}]`}</pre>
            {:else}
              <pre>{`:Rust :IsA :ProgrammingLanguage .`}</pre>
            {/if}
          </div>

          <textarea class="upload-input" bind:value={uploadText} rows="10" placeholder="Paste ConceptNet triples…" spellcheck="false"></textarea>

          <div class="upload-actions">
            <label class="file-label">
              <input type="file" accept=".csv,.json,.jsonld,.ttl,.nt" style="display:none;"
                onchange={(e) => { const f = (e.currentTarget as HTMLInputElement).files?.[0]; if (f) f.text().then(t => uploadText = t); }} />
              Upload File
            </label>
            <span style="flex:1;"></span>
            <button class="btn ghost sm" onclick={() => { uploadText = ''; uploadStatus = ''; }}>Clear</button>
            <button class="btn spec sm" disabled={!uploadText.trim() || uploading} onclick={importTriples}>{uploading ? 'Ingesting…' : 'Import'}</button>
          </div>
          {#if uploadStatus}<div class="upload-result">{uploadStatus}</div>{/if}
        </div>

        <div class="upload-panel">
          <h3>Quick Add Triple</h3>
          <div class="triple-form">
            <label><span>Head Entity</span><input type="text" placeholder="e.g. Rust" bind:value={qaHead} /></label>
            <label><span>Relation</span>
              <select bind:value={qaRelation}><option value="">-- select --</option>{#each RELATION_TYPES as r}<option value={r}>{r}</option>{/each}</select>
            </label>
            <label><span>Tail Entity</span><input type="text" placeholder="e.g. ProgrammingLanguage" bind:value={qaTail} /></label>
            <label><span>Confidence</span><input type="number" bind:value={qaConfidence} min="0" max="10" step="0.1" /></label>
            <button class="btn spec sm" style="align-self: end;" onclick={quickAdd}>Add Triple</button>
          </div>
          {#if qaStatus}<div class="upload-result">{qaStatus}</div>{/if}
        </div>
      </div>

    {:else if activeTab === 'train'}
      <!-- ═══ TRAIN TAB ═══ -->
      <div class="train-layout">
        <div class="train-config">
          <h3>RotatE Training Configuration</h3>
          <div class="train-form">
            <label><span>Epochs</span><input type="number" bind:value={trainEpochs} min="10" max="5000" step="10" disabled={trainStatus === 'running'} /></label>
            <label><span>Learning Rate</span><input type="number" bind:value={trainLr} min="0.0001" max="0.1" step="0.0001" disabled={trainStatus === 'running'} /></label>
            <label><span>Gamma (margin)</span><input type="number" bind:value={trainGamma} min="1" max="30" step="0.5" disabled={trainStatus === 'running'} /></label>
            <label><span>Neg. Samples</span><input type="number" bind:value={trainNeg} min="1" max="64" step="1" disabled={trainStatus === 'running'} /></label>
          </div>

          <div class="train-summary">
            <div class="ts-item"><span class="ts-label">Entities</span><span class="ts-value">{nodeCount.toLocaleString()}</span></div>
            <div class="ts-item"><span class="ts-label">Triples</span><span class="ts-value">{edgeCount.toLocaleString()}</span></div>
            <div class="ts-item"><span class="ts-label">Relations</span><span class="ts-value">{relCount.toLocaleString()}</span></div>
            <div class="ts-item"><span class="ts-label">Dims</span><span class="ts-value">{dim || '—'}</span></div>
            {#if trainResult}
              <div class="ts-item"><span class="ts-label">Loss</span><span class="ts-value">{Number(trainResult.loss).toFixed(4)}</span></div>
              <div class="ts-item"><span class="ts-label">MRR</span><span class="ts-value">{Number(trainResult.mrr).toFixed(4)}</span></div>
              <div class="ts-item"><span class="ts-label">Hits@10</span><span class="ts-value">{Number(trainResult.hits_at_10).toFixed(4)}</span></div>
            {/if}
            <div class="ts-item"><span class="ts-label">Output</span><span class="ts-value">kg_entities</span></div>
          </div>

          <div class="train-actions">
            <button class="btn spec" onclick={startTraining} disabled={trainStatus === 'running' || edgeCount === 0}>
              {#if trainStatus === 'running'}
                Training…
              {:else}
                <svg width="14" height="14" viewBox="0 0 16 16" fill="currentColor"><polygon points="3,2 13,8 3,14"/></svg>
                {trainStatus === 'done' ? 'Retrain RotatE' : 'Start Training'}
              {/if}
            </button>
            {#if edgeCount === 0}<span class="progress-label" style="color: var(--text-4);">Ingest triples first</span>{/if}
          </div>
        </div>

        <div class="train-log-panel">
          <h4>Training Log</h4>
          <pre class="train-log">{trainLog || 'No training runs yet.'}</pre>
        </div>
      </div>
    {/if}
  </div>
</div>

<style>
  .kg-page { display: flex; flex-direction: column; height: 100%; overflow: hidden; }

  .screen-header { padding: 18px 32px 0; border-bottom: 1px solid var(--border); background: var(--bg-page); flex-shrink: 0; }
  .breadcrumb { font-size: 12px; color: var(--text-3); margin-bottom: 8px; font-family: var(--font-mono); }
  .breadcrumb .sep { color: var(--text-5); }
  .screen-title-row { display: flex; align-items: center; justify-content: space-between; margin-bottom: 12px; }
  .screen-title { font-family: var(--font-serif); font-size: 24px; font-weight: 500; letter-spacing: -0.02em; }
  .screen-stats { display: flex; gap: 18px; }
  .kstat { font-size: 12px; color: var(--text-3); font-family: var(--font-mono); }
  .kstat strong { color: var(--text-1); font-weight: 600; }

  .tab-bar { display: flex; gap: 2px; }
  .tab { display: flex; align-items: center; gap: 5px; padding: 8px 16px; border: none; border-bottom: 2px solid transparent; background: none; color: var(--text-3); font-size: 12px; font-weight: 500; cursor: pointer; transition: all 0.12s; }
  .tab:hover { color: var(--text-1); }
  .tab.active { color: var(--text-1); border-bottom-color: var(--spec, #5530a8); }

  .kg-body { flex: 1; overflow: hidden; }

  /* ═══ 3D GRAPH ═══ */
  .graph-layout { display: grid; grid-template-columns: 1fr 280px; height: 100%; overflow: hidden; }
  .graph-canvas { position: relative; overflow: hidden; }

  .graph-empty {
    position: absolute; top: 50%; left: 50%; transform: translate(-50%, -50%); z-index: 9;
    display: flex; flex-direction: column; align-items: center; gap: 8px; text-align: center;
    padding: 24px 32px; background: rgba(255,255,255,0.9); border: 1px solid var(--border);
    border-radius: 10px; backdrop-filter: blur(8px); color: var(--text-3); font-size: 13px; max-width: 360px;
  }
  .graph-empty .detail-hint { font-size: 11px; color: var(--text-4); }

  .graph-hud {
    position: absolute; top: 12px; left: 12px; right: 12px; z-index: 10;
    display: flex; align-items: flex-start; gap: 10px; pointer-events: none;
  }
  .graph-hud > * { pointer-events: auto; }

  .hud-search { position: relative; flex-shrink: 0; }
  .hud-search svg { position: absolute; left: 10px; top: 9px; color: var(--text-4); pointer-events: none; }
  .hud-search input {
    padding: 7px 10px 7px 30px; border: 1px solid var(--border); border-radius: 6px;
    font-size: 12px; background: rgba(255,255,255,0.9); color: var(--text-1); outline: none; width: 220px;
    backdrop-filter: blur(8px); box-shadow: 0 1px 4px rgba(0,0,0,0.06);
  }
  .hud-search input:focus { border-color: var(--spec); }
  .hud-search input::placeholder { color: var(--text-5); }

  .hud-results {
    position: absolute; top: 100%; left: 0; right: 0; margin-top: 4px;
    background: rgba(255,255,255,0.97); border: 1px solid var(--border); border-radius: 6px;
    overflow: hidden; backdrop-filter: blur(8px); max-height: 300px; overflow-y: auto;
    box-shadow: 0 4px 16px rgba(0,0,0,0.08);
  }
  .hud-result {
    display: flex; align-items: center; gap: 8px; width: 100%;
    padding: 6px 10px; border: none; background: none; color: var(--text-1); font-size: 11px;
    cursor: pointer; text-align: left; transition: background 0.1s;
  }
  .hud-result:hover { background: var(--bg-hover); }
  .hr-dot { width: 7px; height: 7px; border-radius: 50%; flex-shrink: 0; }
  .hr-name { flex: 1; font-weight: 550; }
  .hr-type { font-family: var(--font-mono, monospace); font-size: 9px; color: var(--text-4); }

  .hud-legend {
    display: flex; gap: 8px; flex-wrap: wrap; padding: 6px 10px;
    background: rgba(255,255,255,0.85); border: 1px solid var(--border); border-radius: 6px; backdrop-filter: blur(8px);
  }
  .legend-item { display: flex; align-items: center; gap: 4px; font-size: 9px; color: var(--text-3); font-family: var(--font-mono, monospace); }
  .legend-dot { width: 6px; height: 6px; border-radius: 50%; }

  .hud-scale {
    margin-left: auto; display: flex; align-items: center; gap: 6px;
    padding: 5px 10px; background: rgba(255,255,255,0.85); border: 1px solid var(--border); border-radius: 6px; backdrop-filter: blur(8px);
  }
  .scale-label { font-size: 9px; color: var(--text-4); font-family: var(--font-mono, monospace); text-transform: uppercase; letter-spacing: 0.05em; }
  .scale-select {
    padding: 3px 6px; border: 1px solid var(--border); border-radius: 4px;
    background: var(--bg-surface); color: var(--text-1); font-size: 11px; font-family: var(--font-mono, monospace); outline: none;
  }

  .hud-controls {
    position: absolute; bottom: 12px; left: 50%; transform: translateX(-50%); z-index: 10;
    padding: 4px 14px; background: rgba(255,255,255,0.7); border: 1px solid var(--border); border-radius: 12px; backdrop-filter: blur(4px);
    font-size: 10px; color: var(--text-4); pointer-events: none; white-space: nowrap;
  }

  /* Detail panel */
  .detail-panel { border-left: 1px solid var(--border); overflow-y: auto; background: var(--bg-page); }
  .detail-header {
    display: flex; align-items: center; gap: 10px;
    padding: 14px 14px; border-bottom: 1px solid var(--border); border-left: 3px solid; background: var(--bg-surface);
  }
  .detail-icon { width: 28px; height: 28px; border-radius: 6px; display: grid; place-items: center; color: white; font-size: 12px; font-weight: 700; flex-shrink: 0; }
  .detail-name { font-size: 14px; font-weight: 600; }
  .detail-type { font-size: 11px; color: var(--text-3); font-family: var(--font-mono); }
  .close-btn { margin-left: auto; border: none; background: none; font-size: 18px; color: var(--text-4); cursor: pointer; }

  .detail-section { padding: 12px 14px; border-bottom: 1px solid var(--divider); }
  .detail-section h4 { font-size: 10px; font-family: var(--font-mono); text-transform: uppercase; letter-spacing: 0.05em; color: var(--text-4); margin-bottom: 8px; display: flex; align-items: center; gap: 6px; }
  .conn-count { background: var(--bg-subtle); padding: 1px 5px; border-radius: 3px; font-size: 10px; }

  .pos-grid { display: grid; grid-template-columns: 20px 1fr; gap: 2px 8px; }
  .pos-axis { font-family: var(--font-mono); font-size: 10px; color: var(--text-4); font-weight: 600; }
  .pos-val { font-family: var(--font-mono); font-size: 11px; color: var(--text-1); }

  .conn-list { display: flex; flex-direction: column; gap: 2px; }
  .conn-row {
    display: flex; align-items: center; gap: 6px;
    padding: 4px 6px; border: none; border-radius: 4px;
    background: none; font-size: 11px; cursor: pointer; text-align: left; width: 100%; transition: background 0.1s;
  }
  .conn-row:hover { background: var(--bg-subtle); }
  .conn-dir { font-size: 12px; color: var(--text-4); width: 14px; text-align: center; flex-shrink: 0; }
  .conn-dir.out { color: var(--ok); }
  .conn-rel { font-family: var(--font-mono); color: var(--spec); font-weight: 500; font-size: 10px; min-width: 65px; }
  .conn-entity { color: var(--text-1); flex: 1; }
  .conn-type-dot { width: 6px; height: 6px; border-radius: 50%; flex-shrink: 0; }
  .no-conn { font-size: 11px; color: var(--text-4); }

  .plane-info { display: flex; gap: 8px; font-size: 11px; margin-bottom: 3px; }
  .plane-label { color: var(--text-4); font-family: var(--font-mono); min-width: 50px; }
  .plane-vec { color: var(--text-1); font-family: var(--font-mono); }
  .plane-desc { font-size: 10px; color: var(--text-4); margin-top: 6px; line-height: 1.4; }
  .plane-desc strong { color: var(--text-2); }

  .embed-preview { display: flex; flex-wrap: wrap; gap: 3px; align-items: center; }
  .embed-val { font-family: var(--font-mono); font-size: 9px; color: var(--text-2); background: var(--bg-subtle); padding: 1px 4px; border-radius: 2px; }
  .embed-ellipsis { font-size: 9px; color: var(--text-5); font-family: var(--font-mono); }

  .detail-empty { display: flex; flex-direction: column; align-items: center; justify-content: center; height: 100%; gap: 8px; color: var(--text-4); font-size: 13px; }
  .detail-hint { font-size: 11px; color: var(--text-5); font-family: var(--font-mono); }

  /* ═══ VECTORS ═══ */
  .vectors-layout { display: flex; flex-direction: column; height: 100%; overflow: hidden; }
  .vec-toolbar { display: flex; align-items: center; gap: 12px; padding: 10px 16px; border-bottom: 1px solid var(--border); background: var(--bg-surface); flex-shrink: 0; }
  .vec-title { font-family: var(--font-mono); font-size: 13px; font-weight: 600; color: var(--text-1); }
  .vec-meta { font-size: 11px; color: var(--text-4); font-family: var(--font-mono); }
  .vec-search { margin-left: auto; }
  .vec-search input { padding: 5px 10px; border: 1px solid var(--border); border-radius: var(--r-sm); font-size: 12px; background: var(--bg-page); color: var(--text-1); outline: none; width: 200px; }
  .vec-search input:focus { border-color: var(--spec); }

  .vec-search-results { padding: 12px 16px; border-bottom: 1px solid var(--border); background: var(--bg-subtle); }
  .vec-search-results h4 { font-size: 12px; color: var(--text-2); margin-bottom: 8px; }

  .vec-table-wrap { flex: 1; overflow: auto; }
  .vec-table { display: table; width: 100%; font-size: 12px; }
  .vec-row { display: table-row; }
  .vec-row.header { background: var(--bg-surface); font-weight: 600; color: var(--text-3); font-size: 10px; text-transform: uppercase; letter-spacing: 0.04em; font-family: var(--font-mono); position: sticky; top: 0; z-index: 1; }
  .vec-row:not(.header):hover { background: var(--bg-hover); }
  .vec-row > span { display: table-cell; padding: 8px 10px; border-bottom: 1px solid var(--divider); vertical-align: middle; }
  .vc-id { width: 50px; font-family: var(--font-mono); color: var(--text-4); }
  .vc-name { font-weight: 550; color: var(--text-1); }
  .vc-type { width: 90px; }
  .vc-dims { width: 50px; font-family: var(--font-mono); color: var(--text-3); text-align: center; }
  .vc-norm { width: 60px; font-family: var(--font-mono); color: var(--text-3); }
  .vc-vec { font-family: var(--font-mono); font-size: 10px; color: var(--text-3); max-width: 280px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .vc-status { width: 60px; font-family: var(--font-mono); font-size: 10px; color: var(--ok); font-weight: 600; }
  .vc-status.pending { color: var(--warn, #d97706); }
  .vc-sim { width: 80px; font-family: var(--font-mono); position: relative; }
  .sim-bar { position: absolute; left: 0; top: 0; bottom: 0; background: rgba(85, 48, 168, 0.08); border-radius: 2px; }
  .type-badge { display: inline-block; padding: 1px 6px; border-radius: 3px; color: white; font-size: 9px; font-weight: 600; font-family: var(--font-mono); }

  /* ═══ UPLOAD ═══ */
  .upload-layout { display: grid; grid-template-columns: 1fr 1fr; gap: 20px; padding: 20px 24px; overflow-y: auto; height: 100%; align-content: start; }
  .upload-panel { background: var(--bg-surface); border: 1px solid var(--border); border-radius: var(--r); padding: 20px; }
  .upload-panel h3 { font-family: var(--font-serif); font-size: 16px; font-weight: 500; margin-bottom: 6px; }
  .upload-desc { font-size: 12px; color: var(--text-3); margin-bottom: 14px; }
  .format-selector { display: flex; gap: 2px; background: var(--bg-subtle); border-radius: var(--r-sm); padding: 2px; margin-bottom: 12px; }
  .fmt-btn { padding: 5px 12px; border: none; border-radius: 4px; background: none; color: var(--text-3); font-size: 12px; cursor: pointer; }
  .fmt-btn.active { background: var(--bg-surface); color: var(--text-1); box-shadow: var(--shadow-sm); }
  .upload-hint { margin-bottom: 12px; }
  .upload-hint code { font-size: 11px; color: var(--text-3); background: var(--bg-subtle); padding: 2px 6px; border-radius: 3px; }
  .upload-hint pre { font-size: 11px; color: var(--text-2); background: var(--bg-subtle); padding: 8px 10px; border-radius: var(--r-sm); margin-top: 6px; overflow-x: auto; }
  .upload-input { width: 100%; resize: vertical; border: 1px solid var(--border); border-radius: var(--r-sm); padding: 10px 12px; font-family: var(--font-mono); font-size: 12px; color: var(--text-1); background: var(--bg-page); outline: none; }
  .upload-input:focus { border-color: var(--spec); }
  .upload-actions { display: flex; align-items: center; gap: 8px; margin-top: 10px; }
  .file-label { display: flex; align-items: center; gap: 5px; padding: 5px 12px; border: 1px solid var(--border); border-radius: var(--r-sm); font-size: 12px; color: var(--text-2); cursor: pointer; }
  .file-label:hover { border-color: var(--spec); color: var(--text-1); }
  .upload-result { margin-top: 10px; padding: 8px 12px; background: #f0fdf4; color: #15803d; border: 1px solid #bbf7d0; border-radius: var(--r-sm); font-size: 12px; font-family: var(--font-mono); }
  .triple-form { display: grid; grid-template-columns: 1fr 1fr; gap: 10px; }
  .triple-form label { display: flex; flex-direction: column; gap: 3px; font-size: 11px; }
  .triple-form label span { color: var(--text-2); font-size: 10px; font-weight: 600; text-transform: uppercase; letter-spacing: 0.05em; }

  /* ═══ TRAIN ═══ */
  .train-layout { display: grid; grid-template-columns: 380px 1fr; height: 100%; overflow: hidden; }
  .train-config { padding: 20px; border-right: 1px solid var(--border); overflow-y: auto; }
  .train-config h3 { font-family: var(--font-serif); font-size: 16px; font-weight: 500; margin-bottom: 14px; }
  .train-form { display: grid; grid-template-columns: 1fr 1fr; gap: 10px; margin-bottom: 16px; }
  .train-form label { display: flex; flex-direction: column; gap: 3px; font-size: 11px; }
  .train-form label span { color: var(--text-2); font-size: 10px; font-weight: 600; text-transform: uppercase; letter-spacing: 0.05em; }
  .train-summary { display: grid; grid-template-columns: 1fr 1fr; gap: 6px; margin-bottom: 16px; padding: 12px; background: var(--bg-subtle); border-radius: var(--r-sm); }
  .ts-item { display: flex; justify-content: space-between; font-size: 12px; }
  .ts-label { color: var(--text-4); }
  .ts-value { color: var(--text-1); font-family: var(--font-mono); font-weight: 550; }
  .train-actions { display: flex; align-items: center; gap: 12px; }
  .progress-label { font-family: var(--font-mono); font-size: 12px; font-weight: 600; color: var(--spec); }
  .train-log-panel { display: flex; flex-direction: column; overflow: hidden; }
  .train-log-panel h4 { padding: 10px 16px; border-bottom: 1px solid var(--border); font-size: 11px; font-family: var(--font-mono); text-transform: uppercase; letter-spacing: 0.05em; color: var(--text-4); background: var(--bg-surface); flex-shrink: 0; }
  .train-log { flex: 1; margin: 0; padding: 12px 16px; font-family: var(--font-mono); font-size: 11px; color: var(--text-2); line-height: 1.6; white-space: pre-wrap; overflow-y: auto; background: var(--bg-page); }
</style>
