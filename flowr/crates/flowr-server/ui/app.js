// FlowR UI — vanilla JS, no dependencies
const API = '';  // same origin

// Navigation
document.querySelectorAll('.nav-item').forEach(item => {
  item.addEventListener('click', (e) => {
    e.preventDefault();
    const page = item.dataset.page;
    document.querySelectorAll('.nav-item').forEach(n => n.classList.remove('active'));
    item.classList.add('active');
    document.querySelectorAll('.page').forEach(p => p.classList.remove('active'));
    document.getElementById('page-' + page).classList.add('active');
    loadPage(page);
  });
});

// Initial load
loadPage('dashboard');
setInterval(() => loadPage(currentPage()), 10000);

function currentPage() {
  const active = document.querySelector('.nav-item.active');
  return active ? active.dataset.page : 'dashboard';
}

async function api(path) {
  const r = await fetch(API + path);
  return r.json();
}

async function apiPost(path, body) {
  const r = await fetch(API + path, {
    method: 'POST', headers: {'Content-Type': 'application/json'}, body: JSON.stringify(body)
  });
  return r.json();
}

async function apiDelete(path) {
  const r = await fetch(API + path, {method: 'DELETE'});
  return r.json();
}

async function mcp(tool, args) {
  const r = await fetch(API + '/message', {
    method: 'POST', headers: {'Content-Type': 'application/json'},
    body: JSON.stringify({jsonrpc: '2.0', method: 'tools/call', id: 1, params: {name: tool, arguments: args}})
  });
  const d = await r.json();
  try { return JSON.parse(d.result.content[0].text); } catch { return d.result; }
}

// Page loaders
async function loadPage(page) {
  switch(page) {
    case 'dashboard': return loadDashboard();
    case 'agents': return loadAgents();
    case 'templates': return loadTemplates();
    case 'triggers': return loadTriggers();
    case 'workflows': return loadWorkflows();
    case 'resources': return loadResources();
    case 'knowledge': return loadKnowledge();
    case 'memory': return loadMemory();
  }
}

async function loadDashboard() {
  const [agents, triggers, templates] = await Promise.all([
    api('/agents'), api('/triggers'), api('/templates')
  ]);
  let resources;
  try { resources = await mcp('list_resources', {}); } catch { resources = {count: 0}; }

  set('stat-agents', agents.count);
  set('stat-triggers', triggers.count);
  set('stat-templates', templates.count);
  set('stat-resources', resources.count || 0);

  const el = document.getElementById('agents-list');
  if (agents.count === 0) { el.innerHTML = '<p style="color:#888">No live agents</p>'; return; }
  el.innerHTML = agents.agents.map(a => `
    <div class="card">
      <div class="card-title">${a.agent_id} <span class="badge badge-${a.mode}">${a.mode}</span></div>
      <div class="card-meta">Tools: ${a.tools_count} | Turns: ${a.history_len} | Status: ${a.status}</div>
    </div>
  `).join('');
}

async function loadAgents() {
  const d = await api('/agents');
  const el = document.getElementById('agents-full-list');
  if (d.count === 0) { el.innerHTML = '<p style="color:#888">No live agents. Spawn one via workflow.</p>'; return; }
  el.innerHTML = `<table><tr><th>Agent ID</th><th>Mode</th><th>Status</th><th>Tools</th><th>Turns</th><th>Actions</th></tr>
    ${d.agents.map(a => `<tr>
      <td>${a.agent_id}</td><td><span class="badge badge-${a.mode}">${a.mode}</span></td>
      <td>${a.status}</td><td>${a.tools_count}</td><td>${a.history_len}</td>
      <td><button class="btn btn-sm" onclick="showAgentDetail('${a.agent_id}')">Detail</button></td>
    </tr>`).join('')}</table>`;
}

async function showAgentDetail(id) {
  const [health, hist] = await Promise.all([api(`/agents/${id}/health`), api(`/agents/${id}/history?limit=20`)]);
  const el = document.getElementById('agent-detail');
  el.style.display = 'block';
  el.innerHTML = `<h3>${id}</h3>
    <p>Mode: ${health.mode} | Uptime: ${health.uptime_secs}s | Tools: ${health.tools_count}</p>
    <p>Turns: ${health.metrics.total_turns} | Avg: ${health.metrics.avg_duration_ms}ms | Success: ${health.metrics.successful_turns}/${health.metrics.total_turns}</p>
    <h3>Conversation History</h3>
    ${hist.turns.map(t => `<div class="turn">
      <div class="turn-input">[${t.turn}] ${esc(t.input.substring(0,100))}</div>
      <div class="turn-output">${esc(t.output.substring(0,200))}</div>
      <div class="turn-meta">${t.duration_ms}ms | ${t.success ? 'ok' : 'FAIL'} | ${t.timestamp.substring(0,19)}</div>
    </div>`).join('')}`;
}

async function loadTemplates() {
  const d = await api('/templates');
  const el = document.getElementById('templates-list');
  if (d.count === 0) { el.innerHTML = '<p style="color:#888">No templates</p>'; return; }
  el.innerHTML = `<table><tr><th>Name</th><th>Active</th><th>Versions</th><th>Actions</th></tr>
    ${d.templates.map(t => `<tr>
      <td>${t.name}</td><td>${t.active_version || '-'}</td>
      <td>${t.versions.join(', ')}</td>
      <td><button class="btn btn-sm btn-danger" onclick="deleteTemplate('${t.name}')">Delete</button></td>
    </tr>`).join('')}</table>`;
}

async function loadTriggers() {
  const d = await api('/triggers');
  const el = document.getElementById('triggers-list');
  if (d.count === 0) { el.innerHTML = '<p style="color:#888">No triggers</p>'; return; }
  el.innerHTML = `<table><tr><th>Name</th><th>Type</th><th>Targets</th><th>Status</th><th>Actions</th></tr>
    ${d.triggers.map(t => {
      const type = t.config.type || '?';
      const status = t.enabled ? 'active' : 'paused';
      return `<tr>
        <td>${t.name}</td><td><span class="badge badge-${type}">${type}</span></td>
        <td>${t.targets.length}</td><td><span class="badge badge-${status}">${status}</span></td>
        <td>
          <button class="btn btn-sm" onclick="toggleTrigger('${t.id}','${status}')">${status==='active'?'Pause':'Enable'}</button>
          <button class="btn btn-sm btn-danger" onclick="deleteTrigger('${t.id}')">Delete</button>
        </td>
      </tr>`;
    }).join('')}</table>`;
}

async function loadWorkflows() {
  const d = await mcp('list_flows', {});
  const el = document.getElementById('workflows-list');
  const flows = d.flows || [];
  if (flows.length === 0) { el.innerHTML = '<p style="color:#888">No workflows</p>'; return; }
  el.innerHTML = `<table><tr><th>Name</th><th>Namespace</th><th>Version</th><th>Actions</th></tr>
    ${flows.map(f => `<tr>
      <td>${f.name}</td><td>${f.namespace}</td><td>${f.version}</td>
      <td><button class="btn btn-sm" onclick="executeWorkflow('${f.namespace}','${f.name}')">Execute</button></td>
    </tr>`).join('')}</table>`;
}

async function loadResources() {
  const d = await mcp('list_resources', {});
  const el = document.getElementById('resources-list');
  const res = d.resources || [];
  if (res.length === 0) { el.innerHTML = '<p style="color:#888">No resources</p>'; return; }
  el.innerHTML = `<table><tr><th>Name</th><th>Type</th><th>Actions</th></tr>
    ${res.map(r => `<tr>
      <td>${r.name}</td><td>${r.type}</td>
      <td><button class="btn btn-sm btn-danger" onclick="deleteResource('${r.namespace}','${r.name}')">Delete</button></td>
    </tr>`).join('')}</table>`;
}

// Actions
function showCreateTemplate() { document.getElementById('template-editor').style.display = 'block'; }
function showCreateTrigger() { document.getElementById('trigger-form').style.display = 'block'; }
function showCreateWorkflow() { document.getElementById('workflow-editor').style.display = 'block'; }
function showCreateResource() { document.getElementById('resource-form').style.display = 'block'; }
function hideEditor(id) { document.getElementById(id).style.display = 'none'; }

async function createTemplate() {
  const yaml = document.getElementById('template-yaml').value;
  try {
    const body = JSON.parse(yaml);
    await apiPost('/templates', body);
    hideEditor('template-editor');
    loadTemplates();
  } catch(e) { alert('Invalid JSON: ' + e.message); }
}

async function createTrigger() {
  const name = document.getElementById('trigger-name').value;
  const type = document.getElementById('trigger-type').value;
  const path = document.getElementById('trigger-path').value;
  const config = type === 'webhook' ? {type, path} : type === 'cron' ? {type, expression: path, message: 'cron'} : {type, source: path};
  await apiPost('/triggers', {name, config});
  hideEditor('trigger-form');
  loadTriggers();
}

async function createWorkflow() {
  const yaml = document.getElementById('workflow-yaml').value;
  try {
    const body = JSON.parse(yaml);
    await mcp('create_flow', {json: body});
    hideEditor('workflow-editor');
    loadWorkflows();
  } catch(e) { alert('Invalid JSON: ' + e.message); }
}

async function createResource() {
  const name = document.getElementById('resource-name').value;
  const type_ref = document.getElementById('resource-type').value;
  const config = JSON.parse(document.getElementById('resource-config').value);
  await mcp('create_resource', {name, type_ref, config});
  hideEditor('resource-form');
  loadResources();
}

async function deleteTemplate(name) { if(confirm('Delete '+name+'?')) { await apiDelete('/templates/'+name); loadTemplates(); } }
async function deleteTrigger(id) { if(confirm('Delete trigger?')) { await apiDelete('/triggers/'+id); loadTriggers(); } }
async function deleteResource(ns, name) { if(confirm('Delete '+name+'?')) { await mcp('delete_resource', {namespace:ns,name}); loadResources(); } }
async function toggleTrigger(id, status) { await apiPost(`/triggers/${id}/${status==='active'?'pause':'enable'}`, {}); loadTriggers(); }
async function executeWorkflow(ns, name) {
  const input = prompt('Input JSON (or empty):', '{}');
  if (input === null) return;
  try {
    const r = await mcp('execute_flow', {namespace: ns, name, input: JSON.parse(input)});
    alert('Execution started');
  } catch(e) { alert('Error: ' + e.message); }
}

function set(id, val) { document.querySelector('#'+id+' .stat-value').textContent = val; }
function esc(s) { const d = document.createElement('div'); d.textContent = s; return d.innerHTML; }

// ==================== Knowledge Page ====================

const MOCK_CLUSTERS = [
  { id: 'billing', label: 'Billing & Payments', docs: 127, x: 0.25, y: 0.30, r: 52, color: '#e67e22' },
  { id: 'auth', label: 'Authentication', docs: 89, x: 0.55, y: 0.20, r: 42, color: '#3498db' },
  { id: 'deploy', label: 'Deployment', docs: 156, x: 0.70, y: 0.55, r: 58, color: '#2ecc71' },
  { id: 'monitoring', label: 'Monitoring', docs: 73, x: 0.40, y: 0.65, r: 38, color: '#9b59b6' },
  { id: 'incidents', label: 'Incidents', docs: 45, x: 0.15, y: 0.60, r: 32, color: '#e74c3c' },
  { id: 'onboarding', label: 'Onboarding', docs: 98, x: 0.82, y: 0.25, r: 44, color: '#1abc9c' },
  { id: 'api', label: 'API Design', docs: 112, x: 0.50, y: 0.42, r: 48, color: '#f39c12' },
  { id: 'testing', label: 'Testing', docs: 34, x: 0.30, y: 0.82, r: 28, color: '#95a5a6' },
  { id: 'security', label: 'Security', docs: 67, x: 0.75, y: 0.78, r: 36, color: '#c0392b' },
  { id: 'data', label: 'Data Pipeline', docs: 82, x: 0.12, y: 0.42, r: 40, color: '#8e44ad' },
  { id: 'infra', label: 'Infrastructure', docs: 41, x: 0.60, y: 0.85, r: 30, color: '#16a085' },
  { id: 'ml', label: 'ML Models', docs: 23, x: 0.88, y: 0.60, r: 24, color: '#d35400' },
];

const MOCK_SEARCH_RESULTS = [
  { title: 'Kubernetes Pod Scheduling Guide', snippet: 'Pod scheduling uses node selectors and affinity rules to place workloads on appropriate nodes...', cluster: 'deploy', score: 0.94, source: 'wiki/k8s-scheduling.md' },
  { title: 'Incident Response Runbook', snippet: 'When a P1 incident is declared, the on-call engineer should immediately check the Grafana dashboard...', cluster: 'incidents', score: 0.89, source: 'runbooks/incident-p1.md' },
  { title: 'OAuth2 Token Refresh Flow', snippet: 'The refresh token grant type is used to obtain a new access token when the current token expires...', cluster: 'auth', score: 0.87, source: 'docs/auth/oauth2-refresh.md' },
  { title: 'Stripe Webhook Integration', snippet: 'Payment webhooks are processed asynchronously via the billing-events queue with at-least-once delivery...', cluster: 'billing', score: 0.82, source: 'docs/billing/stripe-webhooks.md' },
  { title: 'ClickHouse ETL Pipeline', snippet: 'Raw events are ingested from Redpanda, transformed via materialized views, and stored in partitioned tables...', cluster: 'data', score: 0.78, source: 'docs/data/clickhouse-etl.md' },
  { title: 'API Rate Limiting Strategy', snippet: 'We use a sliding window rate limiter backed by Redis with configurable limits per endpoint...', cluster: 'api', score: 0.75, source: 'docs/api/rate-limiting.md' },
];

let knowledgeLoaded = false;

function loadKnowledge() {
  if (knowledgeLoaded) return;
  knowledgeLoaded = true;
  renderClusters();
  renderSearchResults(MOCK_SEARCH_RESULTS);
}

function renderClusters() {
  const el = document.getElementById('cluster-viz');
  const tooltip = document.createElement('div');
  tooltip.className = 'cluster-tooltip';
  tooltip.id = 'cluster-tip';
  el.appendChild(tooltip);

  MOCK_CLUSTERS.forEach(c => {
    const node = document.createElement('div');
    node.className = 'cluster-node';
    node.style.width = c.r * 2 + 'px';
    node.style.height = c.r * 2 + 'px';
    node.style.left = `calc(${c.x * 100}% - ${c.r}px)`;
    node.style.top = `calc(${c.y * 100}% - ${c.r}px)`;
    node.style.background = `radial-gradient(circle at 35% 35%, ${c.color}cc, ${c.color}66)`;
    node.style.border = `2px solid ${c.color}`;
    node.textContent = c.docs;
    node.addEventListener('mouseenter', (e) => showClusterTip(e, c));
    node.addEventListener('mouseleave', hideClusterTip);
    node.addEventListener('click', () => filterByCluster(c.id));
    el.appendChild(node);

    const label = document.createElement('div');
    label.className = 'cluster-label';
    label.style.left = `calc(${c.x * 100}% - 40px)`;
    label.style.top = `calc(${c.y * 100}% + ${c.r + 4}px)`;
    label.style.width = '80px';
    label.textContent = c.label;
    el.appendChild(label);
  });
}

function showClusterTip(e, c) {
  const tip = document.getElementById('cluster-tip');
  tip.innerHTML = `<strong>${c.label}</strong><br>${c.docs} documents<br><span style="color:#888">Click to filter</span>`;
  tip.classList.add('show');
  const rect = e.target.getBoundingClientRect();
  const parent = document.getElementById('cluster-viz').getBoundingClientRect();
  tip.style.left = (rect.left - parent.left + rect.width / 2 - 90) + 'px';
  tip.style.top = (rect.top - parent.top - 60) + 'px';
}

function hideClusterTip() { document.getElementById('cluster-tip').classList.remove('show'); }

function filterByCluster(clusterId) {
  const filtered = MOCK_SEARCH_RESULTS.filter(r => r.cluster === clusterId);
  if (filtered.length) renderSearchResults(filtered);
  else renderSearchResults(MOCK_SEARCH_RESULTS);
  document.getElementById('k-search').value = MOCK_CLUSTERS.find(c => c.id === clusterId)?.label || '';
}

function searchKnowledge() {
  const q = document.getElementById('k-search').value.toLowerCase();
  if (!q) { renderSearchResults(MOCK_SEARCH_RESULTS); return; }
  const filtered = MOCK_SEARCH_RESULTS.filter(r =>
    r.title.toLowerCase().includes(q) || r.snippet.toLowerCase().includes(q) || r.cluster.includes(q)
  );
  renderSearchResults(filtered.length ? filtered : MOCK_SEARCH_RESULTS);
}

function renderSearchResults(results) {
  document.getElementById('search-results').innerHTML = results.map(r => `
    <div class="search-result" onclick="filterByCluster('${r.cluster}')">
      <div class="sr-title">${esc(r.title)}</div>
      <div class="sr-snippet">${esc(r.snippet)}</div>
      <div class="sr-meta"><span class="sr-score">${r.score.toFixed(2)}</span> ${esc(r.source)}</div>
    </div>
  `).join('');
}

document.getElementById('k-search')?.addEventListener('keydown', e => { if (e.key === 'Enter') searchKnowledge(); });

// ==================== Memory Page ====================

const MOCK_AGENTS_MEMORY = [
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
        { key: 'deploy-v0.6.3', value: 'Successfully deployed flowr v0.6.3 to k3d cluster. All 17 tests passed.', ts: '2026-04-27T15:20:00Z' },
        { key: 'incident-db-slow', value: 'LanceDB query latency spiked to 200ms. Root cause: missing index on executions table. Added index, resolved.', ts: '2026-04-26T09:15:00Z' },
        { key: 'config-change', value: 'Updated CORS origins to include console.a3t.io', ts: '2026-04-25T14:00:00Z' },
        { key: 'test-run-42', value: 'Ran full test suite. 30/30 passed. Duration: 45s.', ts: '2026-04-24T18:30:00Z' },
      ],
      semantic: [
        { key: 'k3d-architecture', value: 'k3d cluster "a3t" runs on 37.27.195.4 with 1 server + 1 agent node. Services: flowr (port 3002), clawdb (port 3000).', ts: '2026-04-22T00:00:00Z' },
        { key: 'flowr-extensions', value: 'CNCF SWF extensions are before/after interceptors declared in use.extensions. Resolved at build time, fire-and-forget execution.', ts: '2026-04-27T00:00:00Z' },
        { key: 'retry-backoff', value: 'RetryConfig supports constant, exponential, linear backoff with optional jitter (from/to Duration range).', ts: '2026-04-27T00:00:00Z' },
      ],
      procedural: [
        { key: 'deploy-to-k3d', value: 'WORKFLOW: rsync → cargo build --release → docker build → k3d image import (k8s.io namespace) → kubectl set image → verify', ts: '2026-04-27T00:00:00Z' },
        { key: 'run-tests', value: 'WORKFLOW: SERVER=http://host:port bash test_new_features.sh → verify 17/17 pass', ts: '2026-04-27T00:00:00Z' },
        { key: 'create-agent', value: 'WORKFLOW: create_resource(type:agent) → spawn:agent → verify /agents endpoint', ts: '2026-04-22T00:00:00Z' },
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
        { key: 'turn-1', value: 'User asked about pricing. Provided tier comparison table.', ts: '2026-04-28T09:01:00Z' },
        { key: 'turn-2', value: 'User requested invoice for Q1. Triggered billing workflow.', ts: '2026-04-28T09:03:00Z' },
        { key: 'turn-3', value: 'Workflow completed. Invoice sent to user@company.com.', ts: '2026-04-28T09:04:00Z' },
      ],
      semantic: [
        { key: 'pricing-model', value: 'Three tiers: Free (5 agents), Pro ($49/mo, 50 agents), Enterprise (custom).', ts: '2026-04-01T00:00:00Z' },
        { key: 'refund-policy', value: 'Refunds within 30 days of purchase. Pro-rated for annual plans.', ts: '2026-04-01T00:00:00Z' },
      ],
      procedural: [
        { key: 'issue-refund', value: 'WORKFLOW: verify_purchase → calculate_prorated → stripe_refund → notify_user', ts: '2026-04-01T00:00:00Z' },
        { key: 'escalate', value: 'WORKFLOW: collect_context → create_ticket(jira) → notify_slack(#support)', ts: '2026-04-01T00:00:00Z' },
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
        { key: 'hourly-summary', value: 'Hour 10: 1,247 bids, 312 wins (25% rate), avg CPM $0.92', ts: '2026-04-28T10:00:00Z' },
        { key: 'hourly-summary', value: 'Hour 09: 1,089 bids, 287 wins (26% rate), avg CPM $0.88', ts: '2026-04-28T09:00:00Z' },
      ],
      semantic: [
        { key: 'bid-strategy', value: 'Second-price auction. Floor multiplier 1.2x for tier-1 inventory.', ts: '2026-04-15T00:00:00Z' },
      ],
      procedural: [
        { key: 'bid-flow', value: 'WORKFLOW: parse_ortb → match_campaign → compute_bid(lua) → format_response', ts: '2026-04-20T00:00:00Z' },
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
        { key: 'report-weekly', value: 'Generated weekly KPI report. Revenue up 12%, CTR stable at 2.1%.', ts: '2026-04-27T08:00:00Z' },
      ],
      semantic: [
        { key: 'kpi-definitions', value: 'CTR = clicks/impressions. CPC = spend/clicks. ROAS = revenue/spend.', ts: '2026-04-01T00:00:00Z' },
      ],
      procedural: [
        { key: 'weekly-report', value: 'WORKFLOW: query_clickhouse(weekly_kpis) → compute_deltas → format_markdown → post_slack', ts: '2026-04-01T00:00:00Z' },
      ],
    }
  },
];

let memoryLoaded = false;

function loadMemory() {
  if (memoryLoaded) return;
  memoryLoaded = true;
  const el = document.getElementById('memory-agent-list');
  el.innerHTML = MOCK_AGENTS_MEMORY.map(a => `
    <div class="mem-agent-item" data-agent="${a.id}" onclick="selectMemoryAgent('${a.id}')">
      <div class="agent-name">${a.id}</div>
      <div class="agent-mode"><span class="badge badge-${a.mode}">${a.mode}</span> ${a.status}</div>
    </div>
  `).join('');
}

function selectMemoryAgent(agentId) {
  document.querySelectorAll('.mem-agent-item').forEach(el => el.classList.remove('selected'));
  document.querySelector(`.mem-agent-item[data-agent="${agentId}"]`)?.classList.add('selected');

  const agent = MOCK_AGENTS_MEMORY.find(a => a.id === agentId);
  if (!agent) return;

  const m = agent.memory;
  const el = document.getElementById('memory-detail-content');
  el.innerHTML = `
    <h2 style="margin-bottom:16px">${agent.id} <span class="badge badge-${agent.mode}">${agent.mode}</span></h2>
    <div class="memory-grid">
      ${renderMemoryCard('Working Memory', 'working', m.working)}
      ${renderMemoryCard('Episodic Memory', 'episodic', m.episodic)}
      ${renderMemoryCard('Semantic Memory', 'semantic', m.semantic)}
      ${renderMemoryCard('Procedural Memory', 'procedural', m.procedural)}
    </div>
    <div class="tools-section">
      <h3>Tools (${agent.tools.length})</h3>
      <div class="tool-grid">
        ${agent.tools.map(t => `
          <div class="tool-chip">
            <div class="tool-name">${esc(t.name)}</div>
            <div class="tool-desc">${esc(t.desc)}</div>
          </div>
        `).join('')}
      </div>
    </div>
  `;
}

function renderMemoryCard(title, type, entries) {
  const empty = !entries || entries.length === 0;
  return `
    <div class="memory-card mc-${type}">
      <div class="memory-card-header">${title} (${entries?.length || 0})</div>
      <div class="memory-card-body">
        ${empty ? '<div style="color:#aaa;font-size:13px;padding:8px 0">No entries</div>' :
          entries.map(e => `
            <div class="mem-entry">
              <div class="mem-key">${esc(e.key)}</div>
              <div class="mem-val">${esc(e.value)}</div>
              <div class="mem-ts">${new Date(e.ts).toLocaleString()}</div>
            </div>
          `).join('')}
      </div>
    </div>
  `;
}
