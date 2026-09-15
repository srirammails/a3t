<script lang="ts">
  interface MemEntry { key: string; value: string; ts: string; }
  interface AgentTool { name: string; desc: string; }
  interface AgentMemory {
    id: string; mode: string; status: string;
    tools: AgentTool[];
    memory: { working: MemEntry[]; episodic: MemEntry[]; semantic: MemEntry[]; procedural: MemEntry[]; };
  }

  const agents: AgentMemory[] = [
    {
      id: 'ops-agent', mode: 'persistent', status: 'running',
      tools: [
        { name: 'execute_flow', desc: 'Run a workflow' },
        { name: 'get_execution_status', desc: 'Check workflow status' },
        { name: 'create_resource', desc: 'Register a resource' },
        { name: 'aql_query', desc: 'Query agent memory' },
        { name: 'list_flows', desc: 'List all workflows' },
        { name: 'shell_exec', desc: 'Execute shell command' },
      ],
      memory: {
        working: [
          { key: 'current_task', value: 'Monitoring deployment rollout', ts: '2026-04-28T10:32:00Z' },
          { key: 'active_alerts', value: '0 critical, 2 warning', ts: '2026-04-28T10:30:00Z' },
          { key: 'last_check', value: 'k8s pod health: all green', ts: '2026-04-28T10:31:45Z' },
        ],
        episodic: [
          { key: 'deploy-v0.6.3', value: 'Successfully deployed flowr v0.6.3 to k3d. All 17 tests passed.', ts: '2026-04-27T15:20:00Z' },
          { key: 'incident-db-slow', value: 'LanceDB query latency spiked to 200ms. Root cause: missing index. Resolved.', ts: '2026-04-26T09:15:00Z' },
          { key: 'config-change', value: 'Updated CORS origins to include console.a3t.io', ts: '2026-04-25T14:00:00Z' },
        ],
        semantic: [
          { key: 'k3d-architecture', value: 'k3d cluster "a3t" runs on 37.27.195.4. Services: flowr (3002), clawdb (3000).', ts: '2026-04-22T00:00:00Z' },
          { key: 'flowr-extensions', value: 'CNCF SWF extensions are before/after interceptors in use.extensions. Fire-and-forget.', ts: '2026-04-27T00:00:00Z' },
        ],
        procedural: [
          { key: 'deploy-to-k3d', value: 'rsync -> cargo build --release -> docker build -> k3d image import -> kubectl set image', ts: '2026-04-27T00:00:00Z' },
          { key: 'run-tests', value: 'SERVER=http://host:port bash test_new_features.sh -> verify 17/17 pass', ts: '2026-04-27T00:00:00Z' },
        ],
      }
    },
    {
      id: 'chat-agent', mode: 'session', status: 'idle',
      tools: [
        { name: 'aql_recall', desc: 'Recall from memory' },
        { name: 'aql_store', desc: 'Store to memory' },
        { name: 'execute_flow', desc: 'Run a workflow' },
        { name: 'web_search', desc: 'Search the web' },
      ],
      memory: {
        working: [
          { key: 'session_id', value: 'sess-abc123', ts: '2026-04-28T09:00:00Z' },
          { key: 'user_context', value: 'Enterprise customer, billing tier: pro', ts: '2026-04-28T09:00:00Z' },
        ],
        episodic: [
          { key: 'turn-1', value: 'User asked about pricing. Provided tier comparison.', ts: '2026-04-28T09:01:00Z' },
          { key: 'turn-2', value: 'User requested Q1 invoice. Triggered billing workflow.', ts: '2026-04-28T09:03:00Z' },
          { key: 'turn-3', value: 'Workflow completed. Invoice sent to user@company.com.', ts: '2026-04-28T09:04:00Z' },
        ],
        semantic: [
          { key: 'pricing-model', value: 'Free (5 agents), Pro ($49/mo, 50 agents), Enterprise (custom).', ts: '2026-04-01T00:00:00Z' },
          { key: 'refund-policy', value: 'Refunds within 30 days. Pro-rated for annual plans.', ts: '2026-04-01T00:00:00Z' },
        ],
        procedural: [
          { key: 'issue-refund', value: 'verify_purchase -> calculate_prorated -> stripe_refund -> notify_user', ts: '2026-04-01T00:00:00Z' },
          { key: 'escalate', value: 'collect_context -> create_ticket(jira) -> notify_slack(#support)', ts: '2026-04-01T00:00:00Z' },
        ],
      }
    },
    {
      id: 'bidder-agent', mode: 'reactive', status: 'waiting',
      tools: [
        { name: 'compute_bid', desc: 'Calculate bid price (Lua)' },
        { name: 'get_campaign', desc: 'Fetch campaign data' },
        { name: 'log_impression', desc: 'Log bid impression' },
      ],
      memory: {
        working: [
          { key: 'last_bid', value: '{"campaign":"camp-42","bid":0.85,"won":true}', ts: '2026-04-28T10:30:00Z' },
        ],
        episodic: [
          { key: 'hour-10', value: '1,247 bids, 312 wins (25%), avg CPM $0.92', ts: '2026-04-28T10:00:00Z' },
          { key: 'hour-09', value: '1,089 bids, 287 wins (26%), avg CPM $0.88', ts: '2026-04-28T09:00:00Z' },
        ],
        semantic: [
          { key: 'bid-strategy', value: 'Second-price auction. Floor multiplier 1.2x for tier-1 inventory.', ts: '2026-04-15T00:00:00Z' },
        ],
        procedural: [
          { key: 'bid-flow', value: 'parse_ortb -> match_campaign -> compute_bid(lua) -> format_response', ts: '2026-04-20T00:00:00Z' },
        ],
      }
    },
    {
      id: 'analyst-agent', mode: 'ephemeral', status: 'idle',
      tools: [
        { name: 'sql_query', desc: 'Run SQL on ClickHouse' },
        { name: 'create_chart', desc: 'Generate chart data' },
        { name: 'aql_recall', desc: 'Recall analysis patterns' },
      ],
      memory: {
        working: [],
        episodic: [
          { key: 'report-weekly', value: 'Weekly KPI: Revenue up 12%, CTR stable at 2.1%.', ts: '2026-04-27T08:00:00Z' },
        ],
        semantic: [
          { key: 'kpi-definitions', value: 'CTR = clicks/impressions. CPC = spend/clicks. ROAS = revenue/spend.', ts: '2026-04-01T00:00:00Z' },
        ],
        procedural: [
          { key: 'weekly-report', value: 'query_clickhouse(weekly_kpis) -> compute_deltas -> format_md -> post_slack', ts: '2026-04-01T00:00:00Z' },
        ],
      }
    },
  ];

  let selectedId = $state('');
  let selected = $derived(agents.find(a => a.id === selectedId));

  const MODE_COLORS: Record<string, string> = {
    persistent: 'var(--persistent, #0077cc)',
    session: 'var(--session, #d4a017)',
    reactive: 'var(--reactive, #cc3344)',
    ephemeral: 'var(--ephemeral, #3a7fff)',
  };

  const TIER_META: Record<string, { label: string; color: string; icon: string }> = {
    working:    { label: 'Working Memory',    color: '#4caf50', icon: 'W' },
    episodic:   { label: 'Episodic Memory',   color: '#2196f3', icon: 'E' },
    semantic:   { label: 'Semantic Memory',   color: '#e91e63', icon: 'S' },
    procedural: { label: 'Procedural Memory', color: '#ff9800', icon: 'P' },
  };

  function fmtTime(ts: string) {
    const d = new Date(ts);
    const now = new Date();
    const diff = now.getTime() - d.getTime();
    if (diff < 3600000) return `${Math.floor(diff / 60000)}m ago`;
    if (diff < 86400000) return `${Math.floor(diff / 3600000)}h ago`;
    return d.toLocaleDateString();
  }
</script>

<div class="screen-header">
  <div class="breadcrumb">Augment <span class="sep">/</span> Memory</div>
  <div class="screen-title-row">
    <h1 class="screen-title">Agent Memory</h1>
    <div class="screen-stats">
      <span class="kstat"><strong>{agents.length}</strong> agents</span>
      <span class="kstat"><strong>5</strong> memory tiers</span>
    </div>
  </div>
</div>

<div class="memory-body">
  <!-- Agent list panel -->
  <div class="agent-panel">
    <div class="panel-head">Agents</div>
    {#each agents as a}
      <button
        class="agent-row"
        class:selected={selectedId === a.id}
        onclick={() => selectedId = a.id}
      >
        <div class="ar-top">
          <span class="ar-name">{a.id}</span>
          <span class="ar-status" class:running={a.status === 'running'}>{a.status}</span>
        </div>
        <div class="ar-bottom">
          <span class="ar-mode" style="color: {MODE_COLORS[a.mode] ?? 'var(--text-4)'}">{a.mode}</span>
          <span class="ar-tools">{a.tools.length} tools</span>
        </div>
      </button>
    {/each}
  </div>

  <!-- Detail panel -->
  <div class="detail-panel">
    {#if selected}
      <div class="detail-head">
        <h2>{selected.id}</h2>
        <span class="mode-badge" style="background: {MODE_COLORS[selected.mode] ?? '#888'}">{selected.mode}</span>
        <span class="status-dot" class:running={selected.status === 'running'}></span>
        <span class="detail-status">{selected.status}</span>
      </div>

      <!-- 4 memory cards in 2x2 grid -->
      <div class="memory-grid">
        {#each Object.entries(TIER_META) as [tier, meta]}
          {@const entries = selected.memory[tier as keyof typeof selected.memory] ?? []}
          <div class="mem-card">
            <div class="mc-header" style="border-color: {meta.color}">
              <span class="mc-icon" style="background: {meta.color}">{meta.icon}</span>
              <span class="mc-title">{meta.label}</span>
              <span class="mc-count">{entries.length}</span>
            </div>
            <div class="mc-body">
              {#if entries.length === 0}
                <div class="mc-empty">No entries</div>
              {:else}
                {#each entries as entry}
                  <div class="mc-entry">
                    <div class="mc-key">{entry.key}</div>
                    <div class="mc-val">{entry.value}</div>
                    <div class="mc-ts">{fmtTime(entry.ts)}</div>
                  </div>
                {/each}
              {/if}
            </div>
          </div>
        {/each}
      </div>

      <!-- Tools section -->
      <div class="tools-section">
        <h3>Tools <span class="tool-count">{selected.tools.length}</span></h3>
        <div class="tool-grid">
          {#each selected.tools as tool}
            <div class="tool-chip">
              <span class="tc-name">{tool.name}</span>
              <span class="tc-desc">{tool.desc}</span>
            </div>
          {/each}
        </div>
      </div>
    {:else}
      <div class="empty-detail">
        <svg width="48" height="48" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1" opacity="0.2"><path d="M4 2h8a2 2 0 012 2v8a2 2 0 01-2 2H4a2 2 0 01-2-2V4a2 2 0 012-2z"/><path d="M6 2v12M10 2v12M2 6h12M2 10h12"/></svg>
        <p>Select an agent to view memory stores</p>
      </div>
    {/if}
  </div>
</div>

<style>
  .screen-header { padding: 18px 32px 12px; border-bottom: 1px solid var(--border); background: var(--bg-page); }
  .breadcrumb { font-size: 12px; color: var(--text-3); margin-bottom: 8px; font-family: var(--font-mono); }
  .breadcrumb .sep { color: var(--text-5); }
  .screen-title-row { display: flex; align-items: center; justify-content: space-between; }
  .screen-title { font-family: var(--font-serif); font-size: 24px; font-weight: 500; letter-spacing: -0.02em; }
  .screen-stats { display: flex; gap: 18px; }
  .kstat { font-size: 13px; color: var(--text-3); }
  .kstat strong { color: var(--ephemeral, #3a7fff); font-weight: 600; }

  .memory-body { flex: 1; display: flex; overflow: hidden; }

  /* Agent list */
  .agent-panel { width: 250px; border-right: 1px solid var(--border); overflow-y: auto; background: var(--bg-page); }
  .panel-head { padding: 14px 16px 10px; font-size: 11px; text-transform: uppercase; letter-spacing: 0.06em; color: var(--text-4); font-weight: 600; }
  .agent-row {
    display: block; width: 100%; padding: 10px 16px; border: none; background: none;
    text-align: left; cursor: pointer; border-left: 3px solid transparent;
    transition: all 0.12s;
  }
  .agent-row:hover { background: var(--bg-hover); }
  .agent-row.selected { background: var(--bg-surface); border-left-color: var(--ephemeral, #3a7fff); }
  .ar-top { display: flex; justify-content: space-between; align-items: center; }
  .ar-name { font-weight: 600; font-size: 13px; color: var(--text-1); }
  .ar-status { font-size: 10px; color: var(--text-4); font-family: var(--font-mono); }
  .ar-status.running { color: var(--ok, #22c55e); }
  .ar-bottom { display: flex; justify-content: space-between; margin-top: 3px; font-size: 11px; }
  .ar-mode { font-weight: 500; }
  .ar-tools { color: var(--text-4); }

  /* Detail */
  .detail-panel { flex: 1; overflow-y: auto; padding: 24px 32px 40px; }
  .detail-head { display: flex; align-items: center; gap: 12px; margin-bottom: 20px; }
  .detail-head h2 { font-family: var(--font-serif); font-size: 22px; font-weight: 500; }
  .mode-badge { padding: 2px 10px; border-radius: 4px; font-size: 11px; font-weight: 600; color: #fff; }
  .status-dot { width: 8px; height: 8px; border-radius: 50%; background: var(--text-5); margin-left: 8px; }
  .status-dot.running { background: var(--ok, #22c55e); animation: pulse 2s infinite; }
  .detail-status { font-size: 12px; color: var(--text-3); }

  @keyframes pulse { 0%, 100% { opacity: 1; } 50% { opacity: 0.4; } }

  /* Memory grid */
  .memory-grid { display: grid; grid-template-columns: 1fr 1fr; gap: 16px; }
  .mem-card { background: var(--bg-surface); border: 1px solid var(--border); border-radius: var(--r); overflow: hidden; }
  .mc-header {
    display: flex; align-items: center; gap: 8px; padding: 10px 14px;
    border-bottom: 2px solid; font-size: 12px; font-weight: 600; text-transform: uppercase; letter-spacing: 0.04em; color: var(--text-2);
  }
  .mc-icon { width: 22px; height: 22px; border-radius: 4px; display: grid; place-items: center; color: #fff; font-size: 11px; font-weight: 700; font-family: var(--font-mono); }
  .mc-count { margin-left: auto; font-family: var(--font-mono); font-size: 11px; color: var(--text-4); background: var(--bg-subtle); padding: 1px 6px; border-radius: 3px; }
  .mc-body { padding: 8px 14px; max-height: 260px; overflow-y: auto; }
  .mc-empty { padding: 16px 0; color: var(--text-5); font-size: 12px; text-align: center; }
  .mc-entry { padding: 8px 0; border-bottom: 1px solid var(--divider); }
  .mc-entry:last-child { border-bottom: none; }
  .mc-key { font-weight: 600; font-size: 12px; color: var(--text-2); font-family: var(--font-mono); }
  .mc-val { font-size: 12px; color: var(--text-3); margin-top: 2px; line-height: 1.4; }
  .mc-ts { font-size: 10px; color: var(--text-5); margin-top: 3px; }

  /* Tools */
  .tools-section { margin-top: 24px; }
  .tools-section h3 { font-family: var(--font-serif); font-size: 16px; font-weight: 500; margin-bottom: 12px; color: var(--text-2); }
  .tool-count { font-family: var(--font-mono); font-size: 11px; color: var(--text-4); background: var(--bg-subtle); padding: 1px 6px; border-radius: 3px; font-weight: 400; }
  .tool-grid { display: grid; grid-template-columns: repeat(auto-fill, minmax(200px, 1fr)); gap: 8px; }
  .tool-chip { background: var(--bg-surface); border: 1px solid var(--border); border-radius: var(--r-sm); padding: 8px 12px; }
  .tc-name { display: block; font-weight: 600; font-size: 12px; color: var(--text-1); font-family: var(--font-mono); }
  .tc-desc { display: block; font-size: 11px; color: var(--text-4); margin-top: 2px; }

  .empty-detail { display: flex; flex-direction: column; align-items: center; justify-content: center; height: 100%; gap: 12px; color: var(--text-4); font-size: 14px; }
</style>
