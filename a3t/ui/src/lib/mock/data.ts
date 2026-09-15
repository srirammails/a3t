// Mock data for A3T UI development
// Replace with real API calls in services/a3t.ts

export const mockTemplates = [
  // My Templates
  {
    name: 'rtb-bidder', version: 'v2.1', activeVersion: 'v2.1',
    versions: ['v1.0', 'v1.5', 'v2.0', 'v2.1'],
    description: 'Sub-80ms bid decisioning with URL classification, strategy matching, and post-fill learning.',
    icon: 'Rt', color: '#e67e22',
    tags: ['ephemeral', '5 memory types', 'PIPELINE'],
    vertical: 'Real-time decisioning',
    usedBy: 4, available: true, source: 'my' as const,
  },
  {
    name: 'trading-strategy', version: 'v1.3', activeVersion: 'v1.3',
    versions: ['v1.0', 'v1.3'],
    description: 'Signal ingestion, regime detection, order sizing with backtest/replay mode built in.',
    icon: 'Tr', color: '#27ae60',
    tags: ['ephemeral', 'replay', 'parameter sweep'],
    vertical: 'Real-time decisioning',
    usedBy: 1, available: true, source: 'my' as const,
  },
  {
    name: 'alert-triage', version: 'v1.0', activeVersion: 'v1.0',
    versions: ['v1.0'],
    description: 'Windowed alert correlation, noise filtering via learned patterns, runbook routing.',
    icon: 'Al', color: '#8e44ad',
    tags: ['checkpoint', 'WINDOW', 'FOLLOW LINKS'],
    vertical: 'Real-time decisioning',
    usedBy: 0, available: true, source: 'my' as const,
  },
  {
    name: 'support-chat', version: 'v3.0', activeVersion: 'v3.0',
    versions: ['v1.0', 'v2.0', 'v3.0'],
    description: 'Conversational support agent with KB search, escalation routing, and session memory.',
    icon: 'Sc', color: '#2980b9',
    tags: ['session', 'TTL 30m', 'RECALL'],
    vertical: 'Customer engagement',
    usedBy: 2, available: true, source: 'my' as const,
  },
  // Training Templates
  {
    name: 'hello-agent', version: 'v1.0', activeVersion: 'v1.0',
    versions: ['v1.0'],
    description: 'Minimal agent — single LLM call, no memory, no tools. The "hello world" of A3T.',
    icon: 'Hi', color: '#3498db',
    tags: ['ephemeral', 'beginner'],
    vertical: 'Training',
    usedBy: 0, available: true, source: 'training' as const,
  },
  {
    name: 'tool-caller', version: 'v1.0', activeVersion: 'v1.0',
    versions: ['v1.0'],
    description: 'Demonstrates MCP tool discovery and invocation. Calls a single MCP server, returns tool output.',
    icon: 'Tc', color: '#16a085',
    tags: ['ephemeral', 'MCP', 'tools'],
    vertical: 'Training',
    usedBy: 0, available: true, source: 'training' as const,
  },
  {
    name: 'memory-demo', version: 'v1.0', activeVersion: 'v1.0',
    versions: ['v1.0'],
    description: 'Shows all 5 memory types — working, episodic, procedural, semantic, tools. STORE and RECALL via AQL.',
    icon: 'Md', color: '#8e44ad',
    tags: ['persistent', 'AQL', '5 memory types'],
    vertical: 'Training',
    usedBy: 0, available: true, source: 'training' as const,
  },
  {
    name: 'checkpoint-flow', version: 'v1.0', activeVersion: 'v1.0',
    versions: ['v1.0'],
    description: 'Multi-step flow with checkpoints. Kill and restart — resumes from last checkpoint automatically.',
    icon: 'Cf', color: '#c97c2e',
    tags: ['checkpoint', 'durability', 'resume'],
    vertical: 'Training',
    usedBy: 0, available: true, source: 'training' as const,
  },
  // Community Templates
  {
    name: 'rag-pipeline', version: 'v2.0', activeVersion: 'v2.0',
    versions: ['v1.0', 'v2.0'],
    description: 'Retrieval-augmented generation with Qdrant vector search, re-ranking, and citation tracking.',
    icon: 'Rg', color: '#2c3e50',
    tags: ['persistent', 'RAG', 'vectors'],
    vertical: 'Knowledge',
    usedBy: 0, available: true, source: 'community' as const,
  },
  {
    name: 'code-reviewer', version: 'v1.2', activeVersion: 'v1.2',
    versions: ['v1.0', 'v1.2'],
    description: 'GitHub PR review agent. Reads diffs, runs static analysis tools via MCP, posts inline comments.',
    icon: 'Cr', color: '#34495e',
    tags: ['reactive', 'webhook', 'GitHub'],
    vertical: 'DevOps',
    usedBy: 0, available: true, source: 'community' as const,
  },
  {
    name: 'data-enricher', version: 'v1.0', activeVersion: 'v1.0',
    versions: ['v1.0'],
    description: 'Batch enrichment pipeline. Reads from Kafka, enriches via LLM + external APIs, writes to ADB.',
    icon: 'De', color: '#1abc9c',
    tags: ['reactive', 'batch', 'Kafka'],
    vertical: 'Data',
    usedBy: 0, available: true, source: 'community' as const,
  },
];

export const mockAgents = [
  {
    agent_id: 'bidder-emea-premium', mode: 'reactive', status: 'running',
    template: 'rtb-bidder:v2.1', pod: 'a3t-bidder-emea-7f8b9', node: 'eu-west-1-pool-b-3',
    image: 'flowr:0.5.2-rt', replicas: '3/3', restarts: 2,
    uptime: '6d 4h 22m', ip: '10.42.3.128', pid: 1,
    cpu: '0.42 / 1.00', memory: '184Mi / 512Mi',
    tools_count: 16, has_memory: true, history_len: 612000,
    canvas: 'rtb-production',
    metrics: {
      inflight: 28, completed_1h: 612000, p99_latency_ms: 64, errors_1h: 142,
      total_turns: 1842000, successful_turns: 1841858, avg_duration_ms: 58,
    },
  },
  {
    agent_id: 'bidder-us-standard', mode: 'reactive', status: 'running',
    template: 'rtb-bidder:v2.1', pod: 'a3t-bidder-us-4a2c1', node: 'us-east-1-pool-a-1',
    image: 'flowr:0.5.2-rt', replicas: '2/2', restarts: 0,
    uptime: '3d 12h', ip: '10.42.1.55', pid: 1,
    cpu: '0.31 / 1.00', memory: '142Mi / 512Mi',
    tools_count: 16, has_memory: true, history_len: 305000,
    canvas: 'rtb-production',
    metrics: {
      inflight: 12, completed_1h: 305000, p99_latency_ms: 72, errors_1h: 89,
      total_turns: 920000, successful_turns: 919911, avg_duration_ms: 65,
    },
  },
  {
    agent_id: 'support-chat-prod', mode: 'session', status: 'running',
    template: 'support-chat:v3.0', pod: 'a3t-support-8c3d2', node: 'eu-west-1-pool-a-2',
    image: 'flowr:0.5.2-rt', replicas: '1/1', restarts: 0,
    uptime: '14d 2h', ip: '10.42.2.90', pid: 1,
    cpu: '0.08 / 0.50', memory: '96Mi / 256Mi',
    tools_count: 8, has_memory: true, history_len: 4200,
    canvas: null,
    metrics: {
      inflight: 3, completed_1h: 127, p99_latency_ms: 1200, errors_1h: 2,
      total_turns: 4200, successful_turns: 4198, avg_duration_ms: 950,
    },
  },
];

export const mockResources = {
  mcp_servers: [
    { name: 'ctx-scorer-mcp', type: 'mcp', url: 'mcp://scorer.a3t.internal:8009', healthy: true, usedBy: 4, tools: ['d','s','p','●','●','●'] },
    { name: 'tools-registry-mcp', type: 'mcp', url: 'mcp://tools.a3t.internal:8008', healthy: true, usedBy: 6, tools: ['d','s','p','●','●','●','●'] },
    { name: 'brand-safety-mcp', type: 'mcp', url: 'mcp://safety.a3t.internal:8010', healthy: true, usedBy: 2, tools: ['d','s','p','●','●'] },
    { name: 'payments-gateway-mcp', type: 'mcp', url: 'mcp://pay.a3t.internal:8012', healthy: false, usedBy: 3, tools: ['d','s','p','●','●','●','●'] },
    { name: 'viewability-mcp', type: 'mcp', url: 'mcp://view.a3t.internal:8015', healthy: true, usedBy: 0, tools: ['d','s','●','●'] },
  ],
  databases: [
    { name: 'adb-primary', type: 'database', subtype: 'ADB · Arrow + DataFusion', healthy: true, usedBy: 7 },
    { name: 'publisher-kg', type: 'database', subtype: 'Neo4j · 48k nodes', healthy: true, usedBy: 3 },
    { name: 'bid-analytics-pg', type: 'database', subtype: 'PostgreSQL 16', healthy: true, usedBy: 2 },
    { name: 'qdrant-vectors', type: 'database', subtype: 'Qdrant · 1024-dim', healthy: true, usedBy: 4 },
  ],
  llms: [
    { name: 'claude-sonnet', type: 'llm', subtype: 'Anthropic', model: 'claude-sonnet-4-20250514', healthy: true, usedBy: 5 },
    { name: 'gpt-4o-mini', type: 'llm', subtype: 'OpenAI', model: 'gpt-4o-mini', healthy: true, usedBy: 3 },
    { name: 'ollama-local', type: 'llm', subtype: 'Ollama', model: 'llama3:8b', healthy: false, usedBy: 0 },
  ],
  variables: [
    { name: 'floor-multiplier', type: 'variable', value: '1.22', usedBy: 4 },
    { name: 'max-cpm', type: 'variable', value: '5.00', usedBy: 4 },
    { name: 'risk-tolerance', type: 'variable', value: 'moderate', usedBy: 1 },
  ],
};

export const mockTriggers = [
  { id: 'trg-001', name: 'bid-requests', config: { type: 'webhook', path: '/bid' }, enabled: true, targets: [{ type: 'agent', agent_id: 'bidder-emea-premium' }] },
  { id: 'trg-002', name: 'hourly-aggregate', config: { type: 'cron', expression: '0 * * * *' }, enabled: true, targets: [{ type: 'agent', agent_id: 'bidder-emea-premium' }] },
  { id: 'trg-003', name: 'market-alerts', config: { type: 'event', source: 'market.*' }, enabled: false, targets: [] },
];

export const mockInputs = [
  {
    id: 'inp-001', request_type: 'approval', channel: 'form',
    message: 'Approve deployment of rtb-bidder v2.2 to production?',
    title: 'Production Deployment', status: 'pending',
    options: [
      { label: 'Approve', value: 'approved', style: 'primary' },
      { label: 'Reject', value: 'rejected', style: 'danger' },
      { label: 'Defer', value: 'deferred', style: 'default' },
    ],
    context: { version: 'v2.2', environment: 'production', changes: '3 files, +142 -28 lines' },
    created_at: '2026-04-20T08:30:00Z', expires_at: '2026-04-20T09:30:00Z',
    agent_id: 'bidder-emea-premium',
  },
  {
    id: 'inp-002', request_type: 'form', channel: 'form',
    message: 'Configure budget allocation for Q2 campaigns',
    title: 'Budget Input Required', status: 'pending',
    fields: [
      { name: 'total_budget', type: 'number', label: 'Total Budget ($)', required: true },
      { name: 'split', type: 'select', label: 'Split Strategy', options: ['Even', 'Performance-weighted', 'Manual'] },
      { name: 'notes', type: 'text', label: 'Notes', required: false },
    ],
    context: { campaign: 'Q2-awareness', current_spend: 12500 },
    created_at: '2026-04-20T09:15:00Z', expires_at: '2026-04-21T09:15:00Z',
    agent_id: null,
  },
];

export const mockCanvas = [
  {
    name: 'rtb-production', namespace: 'rtb', kind: 'AgentCanvas',
    generation: 14, observedGeneration: 13, drift: true,
    agents: [
      { ref: 'bidder-primary', template: 'rtb-bidder:v2.1', replicas: 3, readyReplicas: 3, trafficShare: 0.99, p99: 68, status: 'running' },
      { ref: 'bidder-canary', template: 'rtb-bidder:v2.1', replicas: 2, readyReplicas: 1, trafficShare: 0.01, status: 'scaling', drift: true },
      { ref: 'bid-settlement', template: 'bid-settlement:v1.0', replicas: 2, readyReplicas: 2, status: 'running' },
    ],
    triggers: [
      { kind: 'HTTPTrigger', path: '/bid', port: 8000, rps: 12000 },
    ],
    outputs: [
      { ref: 'ssp.example', type: 'webhook' },
      { ref: 'bids.audit', type: 'kafka' },
    ],
    reconciliation: {
      status: 'reconciling',
      steps: ['spec parsed', 'diff computed', 'scaling agent-canary', 'triggers bound', 'outputs bound'],
      currentStep: 2,
    },
  },
];

export const mockLogs = [
  { ts: '14:22:03.241', level: 'INFO', msg: 'step=emit_bid instance=01HQK_3F7X bid=3.20 latency=67ms' },
  { ts: '14:22:03.198', level: 'INFO', msg: 'step=compute_bid checkpoint=ok offset=48712' },
  { ts: '14:22:03.183', level: 'DEBUG', msg: 'aql=RECALL FROM EPISODIC WHERE url=... rows=142' },
  { ts: '14:22:03.156', level: 'INFO', msg: 'step=match_strategy pattern=tech_news_premium conf=0.72' },
  { ts: '14:22:03.144', level: 'INFO', msg: 'step=win_history win_rate=0.58 sample=142' },
  { ts: '14:22:03.131', level: 'WARN', msg: 'mcp=brand-safety-mcp p99=18ms note=approaching budget' },
  { ts: '14:22:03.103', level: 'INFO', msg: 'step=classify_url iab=IAB19 conf=0.93' },
  { ts: '14:22:03.103', level: 'INFO', msg: 'instance=01HQK_3F7X started trigger=http:POST /bid' },
  { ts: '14:22:02.989', level: 'DEBUG', msg: 'heartbeat inflight=27 cpu=0.41' },
];

// Summary stats
export const mockStats = {
  templates: 14, agents: 7, canvas: 3,
  resources: { total: 42, healthy: 38, warn: 3, error: 1, types: 7, consumedBy: 7 },
  runtime: { events: 142 },
  knowledge: 8,
};
