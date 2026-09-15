<script lang="ts">
  // --- Types ---
  interface Scenario {
    id: string;
    title: string;
    given: string;
    when: string;
    then: string;
    status: 'passed' | 'failed' | 'skipped' | 'running' | 'pending';
    duration_ms: number;
    error?: string;
    tags: string[];
  }

  interface Suite {
    name: string;
    feature: string;
    scenarios: Scenario[];
  }

  interface CoverageItem {
    name: string;
    covered: number;
    total: number;
  }

  // --- Mock agents for dropdown ---
  const agents = [
    { id: 'ops-agent', mode: 'persistent', template: 'alert-triage:v1.0' },
    { id: 'chat-agent', mode: 'session', template: 'support-chat:v3.0' },
    { id: 'bidder-agent', mode: 'reactive', template: 'rtb-bidder:v2.1' },
    { id: 'analyst-agent', mode: 'ephemeral', template: 'rag-pipeline:v2.0' },
  ];

  // --- Mock test data per agent ---
  const AGENT_SUITES: Record<string, Suite[]> = {
    'ops-agent': [
      {
        name: 'Deployment', feature: 'Deploy workflows to k3d cluster',
        scenarios: [
          { id: 'deploy-1', title: 'Deploy new version', given: 'A built Docker image v0.6.3', when: 'I run the deploy-to-k3d workflow', then: 'Pod is running with new image and health check passes', status: 'passed', duration_ms: 12400, tags: ['critical', 'infra'] },
          { id: 'deploy-2', title: 'Rollback on failure', given: 'A deployment with CrashLoopBackOff', when: 'Agent detects pod failure for >60s', then: 'Previous image is restored and alert is sent', status: 'passed', duration_ms: 8200, tags: ['critical', 'recovery'] },
          { id: 'deploy-3', title: 'Zero-downtime rolling update', given: 'A running service with 2 replicas', when: 'I deploy a new version', then: 'At least 1 replica is ready throughout', status: 'failed', duration_ms: 15600, error: 'Pod readiness probe timed out during rollout (expected <5s, got 12s)', tags: ['p1', 'infra'] },
        ]
      },
      {
        name: 'Monitoring', feature: 'Health checks and alert triage',
        scenarios: [
          { id: 'mon-1', title: 'Detect high memory usage', given: 'A pod using >85% memory', when: 'Metrics are polled', then: 'Warning alert is created with pod details', status: 'passed', duration_ms: 340, tags: ['monitoring'] },
          { id: 'mon-2', title: 'Auto-resolve transient alerts', given: 'A warning alert older than 5m', when: 'Condition is no longer true', then: 'Alert is auto-resolved and logged', status: 'passed', duration_ms: 520, tags: ['monitoring'] },
          { id: 'mon-3', title: 'Escalate P1 incidents', given: 'A critical alert with no ack after 10m', when: 'Escalation timer fires', then: 'Slack message sent to #oncall with runbook link', status: 'skipped', duration_ms: 0, tags: ['escalation'] },
        ]
      },
      {
        name: 'Memory', feature: 'Procedural memory recall and storage',
        scenarios: [
          { id: 'mem-1', title: 'Recall deploy procedure', given: 'Procedural memory has deploy-to-k3d entry', when: 'Agent is asked "how to deploy"', then: 'Workflow is executed directly (no LLM call)', status: 'passed', duration_ms: 18, tags: ['procedural', 'hot-path'] },
          { id: 'mem-2', title: 'Store new procedure from turn', given: 'Agent completes a multi-step task successfully', when: 'on_complete hook fires', then: 'Steps are stored in procedural memory as a reusable pattern', status: 'passed', duration_ms: 45, tags: ['procedural', 'learning'] },
        ]
      },
    ],
    'chat-agent': [
      {
        name: 'Conversation', feature: 'Multi-turn chat with memory',
        scenarios: [
          { id: 'chat-1', title: 'Answer from knowledge base', given: 'Semantic memory has pricing info', when: 'User asks about pricing', then: 'Response includes all 3 tiers with correct prices', status: 'passed', duration_ms: 1200, tags: ['semantic', 'kb'] },
          { id: 'chat-2', title: 'Recall previous conversation', given: 'Episodic memory has prior turns', when: 'User references earlier message', then: 'Agent retrieves context and responds coherently', status: 'passed', duration_ms: 980, tags: ['episodic'] },
          { id: 'chat-3', title: 'Trigger refund workflow', given: 'User requests refund for order #1234', when: 'Agent identifies intent as refund', then: 'issue-refund workflow executes and confirms to user', status: 'passed', duration_ms: 3400, tags: ['workflow', 'critical'] },
          { id: 'chat-4', title: 'Escalate unknown issue', given: 'Agent cannot resolve after 3 turns', when: 'Confidence drops below threshold', then: 'Ticket created in Jira, user notified of handoff', status: 'failed', duration_ms: 4500, error: 'Jira API returned 403: insufficient permissions', tags: ['escalation'] },
        ]
      },
      {
        name: 'Guardrails', feature: 'Input/output safety',
        scenarios: [
          { id: 'guard-1', title: 'Block PII in response', given: 'LLM output contains SSN pattern', when: 'Output guardrail runs', then: 'SSN is redacted before delivery', status: 'passed', duration_ms: 12, tags: ['security'] },
          { id: 'guard-2', title: 'Rate limit per session', given: 'Session has 100 turns in 1 minute', when: 'Next turn arrives', then: 'Rate limit error returned, no LLM call', status: 'passed', duration_ms: 2, tags: ['cost'] },
        ]
      },
    ],
    'bidder-agent': [
      {
        name: 'Bidding', feature: 'Real-time bid decisioning',
        scenarios: [
          { id: 'bid-1', title: 'Compute bid for tier-1 inventory', given: 'OpenRTB request with premium publisher', when: 'Bid flow executes', then: 'Bid price includes 1.2x floor multiplier', status: 'passed', duration_ms: 8, tags: ['latency', 'critical'] },
          { id: 'bid-2', title: 'Skip low-value impression', given: 'OpenRTB request with floor > max bid', when: 'Strategy matcher runs', then: 'No-bid response in <5ms', status: 'passed', duration_ms: 3, tags: ['latency'] },
          { id: 'bid-3', title: 'Budget cap enforcement', given: 'Campaign daily budget is 95% spent', when: 'Bid request arrives', then: 'Bid price reduced by 50% to pace remaining budget', status: 'passed', duration_ms: 11, tags: ['budget'] },
          { id: 'bid-4', title: 'Latency SLA under load', given: '1000 concurrent bid requests', when: 'All processed in parallel', then: 'P99 latency < 80ms', status: 'running', duration_ms: 0, tags: ['latency', 'sla'] },
        ]
      },
      {
        name: 'Learning', feature: 'Post-fill feedback loop',
        scenarios: [
          { id: 'learn-1', title: 'Update win rate model', given: 'Win notification received for bid', when: 'Feedback processor runs', then: 'Win rate for campaign updated in semantic memory', status: 'passed', duration_ms: 25, tags: ['learning'] },
          { id: 'learn-2', title: 'Detect bid landscape shift', given: 'Win rate drops >10% in 1 hour', when: 'Anomaly detector fires', then: 'Alert raised and strategy adjustment triggered', status: 'pending', duration_ms: 0, tags: ['anomaly'] },
        ]
      },
    ],
    'analyst-agent': [
      {
        name: 'Reporting', feature: 'Automated KPI reports',
        scenarios: [
          { id: 'rpt-1', title: 'Generate weekly KPI report', given: 'ClickHouse has 7 days of data', when: 'Weekly report workflow runs', then: 'Markdown report with revenue, CTR, CPC, ROAS deltas', status: 'passed', duration_ms: 2800, tags: ['report'] },
          { id: 'rpt-2', title: 'Handle missing data gracefully', given: 'ClickHouse partition for Tuesday is empty', when: 'Query returns partial results', then: 'Report includes "data unavailable" note, no crash', status: 'passed', duration_ms: 1900, tags: ['resilience'] },
        ]
      },
      {
        name: 'SQL Safety', feature: 'Query guardrails',
        scenarios: [
          { id: 'sql-1', title: 'Block DROP statements', given: 'Agent generates DROP TABLE query', when: 'SQL guardrail evaluates', then: 'Query is rejected with "destructive operation blocked"', status: 'passed', duration_ms: 1, tags: ['security', 'critical'] },
          { id: 'sql-2', title: 'Limit result set size', given: 'Agent generates SELECT * without LIMIT', when: 'Query rewriter runs', then: 'LIMIT 10000 is appended automatically', status: 'passed', duration_ms: 2, tags: ['safety'] },
        ]
      },
    ],
  };

  // --- State ---
  let selectedAgent = $state('ops-agent');
  let runningAll = $state(false);
  let filterStatus = $state('all');

  let suites = $derived(AGENT_SUITES[selectedAgent] ?? []);
  let allScenarios = $derived(suites.flatMap(s => s.scenarios));
  let filteredSuites = $derived(filterStatus === 'all'
    ? suites
    : suites.map(s => ({ ...s, scenarios: s.scenarios.filter(sc => sc.status === filterStatus) })).filter(s => s.scenarios.length > 0)
  );

  // Coverage
  let coverage = $derived((): CoverageItem[] => {
    const agent = agents.find(a => a.id === selectedAgent);
    const all = allScenarios;
    const passed = all.filter(s => s.status === 'passed').length;
    return [
      { name: 'Scenarios', covered: passed, total: all.length },
      { name: 'Features', covered: suites.filter(s => s.scenarios.every(sc => sc.status === 'passed')).length, total: suites.length },
      { name: 'Critical', covered: all.filter(s => s.tags.includes('critical') && s.status === 'passed').length, total: all.filter(s => s.tags.includes('critical')).length },
      { name: 'Hot Path', covered: all.filter(s => (s.tags.includes('hot-path') || s.tags.includes('latency')) && s.status === 'passed').length, total: all.filter(s => s.tags.includes('hot-path') || s.tags.includes('latency')).length },
    ];
  });

  // Summary counts
  let counts = $derived(() => {
    const all = allScenarios;
    return {
      total: all.length,
      passed: all.filter(s => s.status === 'passed').length,
      failed: all.filter(s => s.status === 'failed').length,
      skipped: all.filter(s => s.status === 'skipped').length,
      running: all.filter(s => s.status === 'running').length,
      pending: all.filter(s => s.status === 'pending').length,
    };
  });

  let totalDuration = $derived(allScenarios.reduce((s, sc) => s + sc.duration_ms, 0));

  const STATUS_COLORS: Record<string, string> = {
    passed: 'var(--ok, #22c55e)',
    failed: 'var(--err, #ef4444)',
    skipped: 'var(--text-4, #999)',
    running: 'var(--ephemeral, #3a7fff)',
    pending: 'var(--text-5, #ccc)',
  };

  const STATUS_ICONS: Record<string, string> = {
    passed: '<circle cx="8" cy="8" r="6" fill="none" stroke-width="1.5"/><path d="M5.5 8l1.5 2 3.5-4" stroke-width="1.5" fill="none"/>',
    failed: '<circle cx="8" cy="8" r="6" fill="none" stroke-width="1.5"/><path d="M6 6l4 4M10 6l-4 4" stroke-width="1.5"/>',
    skipped: '<circle cx="8" cy="8" r="6" fill="none" stroke-width="1.5" stroke-dasharray="3 2"/><path d="M6.5 6v4M9.5 6v4" stroke-width="1.5"/>',
    running: '<circle cx="8" cy="8" r="6" fill="none" stroke-width="1.5"/><path d="M8 5v3l2 1.5" stroke-width="1.5" fill="none"/>',
    pending: '<circle cx="8" cy="8" r="6" fill="none" stroke-width="1.5" stroke-dasharray="3 2"/>',
  };

  function fmtDuration(ms: number): string {
    if (ms === 0) return '-';
    if (ms < 1) return '<1ms';
    if (ms < 1000) return ms + 'ms';
    return (ms / 1000).toFixed(1) + 's';
  }

  // Expand/collapse per suite
  let expanded: Record<string, boolean> = $state({});
  function toggle(name: string) { expanded[name] = !expanded[name]; }
  function isOpen(name: string) { return expanded[name] !== false; } // default open
</script>

<div class="screen-header">
  <div class="breadcrumb">Workbench <span class="sep">/</span> Test Bench</div>
  <div class="screen-title-row">
    <div class="title-block">
      <h1 class="screen-title">Test Bench</h1>
    </div>
    <div class="header-actions">
      <select class="agent-select" bind:value={selectedAgent}>
        {#each agents as a}
          <option value={a.id}>{a.id} ({a.mode})</option>
        {/each}
      </select>
      <button class="btn spec" onclick={() => runningAll = !runningAll}>
        <svg width="14" height="14" viewBox="0 0 16 16" fill="currentColor" stroke="none"><polygon points="4,2 14,8 4,14"/></svg>
        Run All
      </button>
    </div>
  </div>
</div>

<div class="bench-body">
  <!-- Summary bar -->
  <div class="summary-bar">
    <div class="summary-counts">
      <button class="sc-pill" class:active={filterStatus === 'all'} onclick={() => filterStatus = 'all'}>
        All <strong>{counts().total}</strong>
      </button>
      <button class="sc-pill passed" class:active={filterStatus === 'passed'} onclick={() => filterStatus = 'passed'}>
        <svg width="12" height="12" viewBox="0 0 16 16" stroke="currentColor" fill="none">{@html STATUS_ICONS.passed}</svg>
        {counts().passed}
      </button>
      <button class="sc-pill failed" class:active={filterStatus === 'failed'} onclick={() => filterStatus = 'failed'}>
        <svg width="12" height="12" viewBox="0 0 16 16" stroke="currentColor" fill="none">{@html STATUS_ICONS.failed}</svg>
        {counts().failed}
      </button>
      <button class="sc-pill skipped" class:active={filterStatus === 'skipped'} onclick={() => filterStatus = 'skipped'}>
        {counts().skipped} skipped
      </button>
      {#if counts().running > 0}
        <span class="sc-pill running">{counts().running} running</span>
      {/if}
      {#if counts().pending > 0}
        <span class="sc-pill pending">{counts().pending} pending</span>
      {/if}
    </div>
    <div class="summary-duration">{fmtDuration(totalDuration)}</div>
  </div>

  <!-- Coverage cards -->
  <div class="coverage-row">
    {#each coverage() as cov}
      {@const pct = cov.total > 0 ? Math.round(cov.covered / cov.total * 100) : 0}
      <div class="cov-card">
        <div class="cov-bar-bg"><div class="cov-bar-fill" style="width:{pct}%; background: {pct === 100 ? 'var(--ok,#22c55e)' : pct >= 70 ? 'var(--ephemeral,#3a7fff)' : 'var(--err,#ef4444)'}"></div></div>
        <div class="cov-label">{cov.name}</div>
        <div class="cov-value">{cov.covered}/{cov.total} <span class="cov-pct">{pct}%</span></div>
      </div>
    {/each}
  </div>

  <!-- BDD Suites -->
  <div class="suites">
    {#each filteredSuites as suite}
      <div class="suite">
        <button class="suite-header" onclick={() => toggle(suite.name)}>
          <svg class="chevron" class:open={isOpen(suite.name)} width="12" height="12" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="2"><path d="M6 4l4 4-4 4"/></svg>
          <span class="suite-feature">Feature:</span>
          <span class="suite-name">{suite.feature}</span>
          <span class="suite-count">
            {suite.scenarios.filter(s => s.status === 'passed').length}/{suite.scenarios.length} passed
          </span>
        </button>

        {#if isOpen(suite.name)}
          <div class="scenario-list">
            {#each suite.scenarios as sc}
              <div class="scenario" class:scenario-failed={sc.status === 'failed'}>
                <div class="sc-status">
                  <svg width="16" height="16" viewBox="0 0 16 16" stroke={STATUS_COLORS[sc.status]} fill="none" stroke-width="1.5">{@html STATUS_ICONS[sc.status]}</svg>
                </div>
                <div class="sc-body">
                  <div class="sc-title">{sc.title}</div>
                  <div class="bdd-steps">
                    <div class="bdd-step"><span class="bdd-kw">Given</span> {sc.given}</div>
                    <div class="bdd-step"><span class="bdd-kw">When</span> {sc.when}</div>
                    <div class="bdd-step"><span class="bdd-kw">Then</span> {sc.then}</div>
                  </div>
                  {#if sc.error}
                    <div class="sc-error">{sc.error}</div>
                  {/if}
                  <div class="sc-meta">
                    <span class="sc-duration">{fmtDuration(sc.duration_ms)}</span>
                    {#each sc.tags as tag}
                      <span class="sc-tag">{tag}</span>
                    {/each}
                  </div>
                </div>
              </div>
            {/each}
          </div>
        {/if}
      </div>
    {/each}
  </div>
</div>

<style>
  .screen-header { padding: 18px 32px 12px; border-bottom: 1px solid var(--border); background: var(--bg-page); }
  .breadcrumb { font-size: 12px; color: var(--text-3); margin-bottom: 8px; font-family: var(--font-mono); }
  .breadcrumb .sep { color: var(--text-5); }
  .screen-title-row { display: flex; align-items: center; justify-content: space-between; }
  .title-block { display: flex; align-items: center; gap: 10px; }
  .screen-title { font-family: var(--font-serif); font-size: 24px; font-weight: 500; letter-spacing: -0.02em; }
  .header-actions { display: flex; gap: 10px; align-items: center; }

  .agent-select {
    padding: 7px 12px; border: 1px solid var(--border); border-radius: var(--r-sm);
    background: var(--bg-surface); font-size: 13px; font-family: var(--font-mono);
    color: var(--text-1); cursor: pointer; outline: none; min-width: 200px;
  }
  .agent-select:focus { border-color: var(--ephemeral, #3a7fff); }

  .bench-body { flex: 1; overflow-y: auto; padding: 20px 32px 40px; }

  /* Summary bar */
  .summary-bar { display: flex; align-items: center; justify-content: space-between; margin-bottom: 16px; }
  .summary-counts { display: flex; gap: 6px; align-items: center; }
  .sc-pill {
    display: inline-flex; align-items: center; gap: 4px; padding: 4px 10px;
    border: 1px solid var(--border); border-radius: 20px; font-size: 12px; font-weight: 500;
    background: var(--bg-surface); color: var(--text-3); cursor: pointer; transition: all 0.12s;
  }
  .sc-pill:hover { background: var(--bg-hover); }
  .sc-pill.active { border-color: var(--ephemeral, #3a7fff); background: var(--ephemeral-bg, #eaf1ff); color: var(--ephemeral, #3a7fff); }
  .sc-pill strong { font-weight: 700; }
  .sc-pill.passed svg { color: var(--ok, #22c55e); }
  .sc-pill.failed svg { color: var(--err, #ef4444); }
  .summary-duration { font-family: var(--font-mono); font-size: 12px; color: var(--text-4); }

  /* Coverage row */
  .coverage-row { display: grid; grid-template-columns: repeat(4, 1fr); gap: 12px; margin-bottom: 24px; }
  .cov-card { background: var(--bg-surface); border: 1px solid var(--border); border-radius: var(--r-sm); padding: 12px 14px; }
  .cov-bar-bg { height: 4px; background: var(--bg-subtle); border-radius: 2px; margin-bottom: 8px; overflow: hidden; }
  .cov-bar-fill { height: 100%; border-radius: 2px; transition: width 0.5s; }
  .cov-label { font-size: 11px; color: var(--text-4); text-transform: uppercase; letter-spacing: 0.04em; }
  .cov-value { font-size: 16px; font-weight: 600; color: var(--text-1); margin-top: 2px; font-family: var(--font-mono); }
  .cov-pct { font-size: 12px; color: var(--text-3); font-weight: 400; }

  /* Suites */
  .suites { display: flex; flex-direction: column; gap: 12px; }
  .suite { background: var(--bg-surface); border: 1px solid var(--border); border-radius: var(--r); overflow: hidden; }
  .suite-header {
    display: flex; align-items: center; gap: 8px; width: 100%; padding: 12px 16px;
    border: none; background: none; cursor: pointer; text-align: left; font-size: 13px;
  }
  .suite-header:hover { background: var(--bg-hover); }
  .suite-feature { color: var(--ephemeral, #3a7fff); font-weight: 600; font-family: var(--font-mono); font-size: 11px; text-transform: uppercase; }
  .suite-name { font-weight: 500; color: var(--text-1); flex: 1; }
  .suite-count { font-family: var(--font-mono); font-size: 11px; color: var(--text-4); background: var(--bg-subtle); padding: 2px 8px; border-radius: 3px; }
  .chevron { color: var(--text-4); transition: transform 0.2s; flex-shrink: 0; }
  .chevron.open { transform: rotate(90deg); }

  /* Scenarios */
  .scenario-list { border-top: 1px solid var(--divider); }
  .scenario { display: flex; gap: 12px; padding: 12px 16px 12px 20px; border-bottom: 1px solid var(--divider); }
  .scenario:last-child { border-bottom: none; }
  .scenario-failed { background: color-mix(in srgb, var(--err, #ef4444) 4%, transparent); }
  .sc-status { flex-shrink: 0; padding-top: 2px; }
  .sc-body { flex: 1; min-width: 0; }
  .sc-title { font-weight: 600; font-size: 13px; color: var(--text-1); margin-bottom: 6px; }

  .bdd-steps { display: flex; flex-direction: column; gap: 3px; margin-bottom: 6px; }
  .bdd-step { font-size: 12px; color: var(--text-3); line-height: 1.4; }
  .bdd-kw { font-weight: 700; color: var(--ephemeral, #3a7fff); font-family: var(--font-mono); font-size: 11px; display: inline-block; width: 50px; }

  .sc-error {
    background: color-mix(in srgb, var(--err, #ef4444) 8%, transparent);
    border: 1px solid color-mix(in srgb, var(--err) 20%, transparent);
    border-radius: var(--r-sm); padding: 6px 10px; font-size: 12px; color: var(--err, #ef4444);
    font-family: var(--font-mono); margin-bottom: 6px;
  }

  .sc-meta { display: flex; align-items: center; gap: 6px; flex-wrap: wrap; }
  .sc-duration { font-family: var(--font-mono); font-size: 11px; color: var(--text-4); }
  .sc-tag { font-size: 10px; padding: 1px 6px; border-radius: 3px; background: var(--bg-subtle); color: var(--text-3); font-family: var(--font-mono); }
</style>
