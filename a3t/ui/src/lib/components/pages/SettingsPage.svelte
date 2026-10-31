<script lang="ts">
  import OrgsSettings from '$lib/components/settings/OrgsSettings.svelte';
  import PageBanner from '$lib/components/shared/PageBanner.svelte';
  import { onMount } from 'svelte';
  import { execFlow, getPlatformDb, switchPlatformDb, getAdbEmbedding, setAdbEmbedding, getAdbCache, setAdbCache, type PlatformDbStatus, type AdbEmbeddingStatus, type AdbEmbeddingSettings, type AdbCacheStatus, type AdbCacheRule, type AdbCacheKeep } from '$lib/services/api';
  import { currentUser } from '$lib/stores/auth';
  import { API_BASE, getDefaultApiBase, getApiOverride, setApiOverride, clearApiOverride } from '$lib/config';

  // ── Icons (16 viewBox, stroke) ──
  const IC: Record<string, string> = {
    all: '<rect x="2" y="2" width="5" height="5" rx="1"/><rect x="9" y="2" width="5" height="5" rx="1"/><rect x="2" y="9" width="5" height="5" rx="1"/><rect x="9" y="9" width="5" height="5" rx="1"/>',
    general: '<circle cx="8" cy="8" r="2.1"/><path d="M8 1.5v1.6M8 12.9v1.6M14.5 8h-1.6M3.1 8H1.5M12.6 3.4l-1.1 1.1M4.5 11.5l-1.1 1.1M12.6 12.6l-1.1-1.1M4.5 4.5L3.4 3.4"/>',
    box: '<path d="M8 1.5l6 3.2v6.6L8 14.5l-6-3.2V4.7z"/><path d="M2 4.7L8 8l6-3.3M8 8v6.5"/>',
    swap: '<path d="M3 6h9l-2.2-2.2M13 10H4l2.2 2.2"/>',
    palette: '<path d="M8 1.5a6.5 6.5 0 100 13c.9 0 1.4-.8 1.4-1.5 0-.4-.2-.7-.4-1s-.4-.6-.4-1c0-.8.7-1.4 1.5-1.4H12A2.5 2.5 0 0014.5 6 6.5 6.5 0 008 1.5z"/><circle cx="5.5" cy="6" r=".7" fill="currentColor"/><circle cx="8" cy="4.6" r=".7" fill="currentColor"/><circle cx="10.4" cy="6" r=".7" fill="currentColor"/>',
    shield: '<path d="M8 1.5l5.5 2v4c0 3.5-2.4 5.8-5.5 6.9C4.9 13.3 2.5 11 2.5 7.5v-4z"/>',
    key: '<circle cx="5" cy="11" r="3"/><path d="M7.1 8.9l5.9-5.9 2 2M11 5l1.5 1.5"/>',
    lockcheck: '<rect x="3" y="7" width="10" height="7" rx="1"/><path d="M5.5 7V5a2.5 2.5 0 015 0v2"/><path d="M6.3 10.6l1.2 1.2 2.2-2.4"/>',
    scale: '<path d="M8 2v12M4.5 4.5h7M4.5 4.5L2.5 9h4zM11.5 4.5l-2 4.5h4zM5.5 14h5"/>',
    audit: '<rect x="3" y="1.5" width="10" height="13" rx="1.5"/><path d="M5.5 5h5M5.5 8h5M5.5 11h3"/>',
    pulse: '<path d="M1.5 8h3l1.5-4.5L9 12l1.5-4h3"/>',
    badge: '<circle cx="8" cy="5.5" r="4"/><path d="M5.6 8.8L4.5 14.5 8 12.6l3.5 1.9-1.1-5.7"/>',
    users: '<circle cx="6" cy="6" r="2.4"/><path d="M1.6 13a4.4 4.4 0 018.8 0"/><path d="M11 4.3a2.4 2.4 0 010 4.5M11.5 13h3a4 4 0 00-3-3.9"/>',
    user: '<circle cx="8" cy="5" r="3"/><path d="M2.6 14a5.4 5.4 0 0110.8 0"/>',
    usercog: '<circle cx="6" cy="5" r="2.4"/><path d="M2.2 13a4 4 0 016.3-3.1"/><circle cx="12" cy="11" r="2"/><path d="M12 8.4v.9M12 13.1v.9M14.2 11h-.9M10.7 11h-.9"/>',
    plug: '<path d="M6 1.8v3.4M10 1.8v3.4M4.5 5.2h7v2.3a3.5 3.5 0 01-7 0zM8 11V14"/>',
    server: '<rect x="2" y="2" width="12" height="4.5" rx="1"/><rect x="2" y="9.5" width="12" height="4.5" rx="1"/><circle cx="4.8" cy="4.2" r=".6" fill="currentColor"/><circle cx="4.8" cy="11.8" r=".6" fill="currentColor"/>',
    nodes: '<circle cx="8" cy="3" r="1.7"/><circle cx="3" cy="12" r="1.7"/><circle cx="13" cy="12" r="1.7"/><path d="M8 4.7v2.8M6.6 9L4.4 10.6M9.4 9l2.2 1.6"/>',
    database: '<ellipse cx="8" cy="4" rx="5.2" ry="2.4"/><path d="M2.8 4v8c0 1.3 2.3 2.4 5.2 2.4s5.2-1.1 5.2-2.4V4"/><path d="M2.8 8c0 1.3 2.3 2.4 5.2 2.4S13.2 9.3 13.2 8"/>',
    bell: '<path d="M8 1.6a3.4 3.4 0 00-3.4 3.4c0 3.9-1.5 4.9-1.5 4.9h9.8s-1.5-1-1.5-4.9A3.4 3.4 0 008 1.6zM6.6 12.9a1.5 1.5 0 002.8 0"/>',
    webhook: '<circle cx="5" cy="5" r="2"/><path d="M6.4 6.4L9 11h3.5M9 11a2 2 0 11-1.9 1.3"/><circle cx="12.5" cy="4.5" r="2"/>',
    billing: '<rect x="1.5" y="3" width="13" height="10" rx="1.5"/><path d="M1.5 6.4h13"/>',
    chart: '<path d="M2 13.5V2.5M2 13.5h12M5 11V8M8 11V5.5M11 11V9"/>',
    card: '<rect x="1.5" y="3.5" width="13" height="9" rx="1.5"/><path d="M4 9.5h3"/>',
    receipt: '<path d="M4 1.5h8v13l-2-1-2 1-2-1-2 1zM6 5h4M6 8h4M6 11h2"/>',
    sliders: '<path d="M2 5h5M11 5h3M2 11h3M9 11h5"/><circle cx="9" cy="5" r="1.7"/><circle cx="7" cy="11" r="1.7"/>',
    flag: '<path d="M3.5 14.5V2M3.5 2.5h8l-1.6 2.6L11.5 8h-8"/>',
  };

  // ── Persistence (a3t-settings CRUD flow; upsert = delete then set) ──
  let settings = $state<Record<string, string>>({});
  const org = () => $currentUser?.org_id ?? 'org-default';
  let loaded = $state(false);
  async function loadAll() {
    const res = await execFlow('a3t-settings', 'a3t', { input: { action: 'list', org_id: org() } }, { silent: true });
    if (res.ok) {
      const rows = Array.isArray(res.data) ? res.data : (res.data?.rows ?? res.data?.output ?? []);
      const map: Record<string, string> = {};
      for (const r of rows) if (r?.key != null) map[r.key] = r.value;
      settings = map;
      // Pull edition + menu from settingsCRUD; fall back sensibly.
      edition = (map['_edition'] === 'ce' || map['_edition'] === 'ee')
        ? (map['_edition'] as 'ce' | 'ee')
        : (getApiOverride() === ceBase ? 'ce' : 'ee');
      if (map['_menu']) { try { const m = JSON.parse(map['_menu']); if (Array.isArray(m) && m.length) navData = m; } catch {} }
      ensureInfraLeaves();
    }
    loaded = true;
  }

  // A runtime-seeded `_menu` may predate newer leaves; make sure these are always
  // present: Logs & Retention and Storage Backend under Infrastructure, and
  // Organisations under General.
  function ensureInfraLeaves() {
    const nav = navData.map(g => ({ ...g, items: [...g.items] }));
    const want: { group: string; icon: string; leaf: Leaf }[] = [
      { group: 'Infrastructure', icon: 'server', leaf: { key: 'infra.backend', label: 'Storage Backend', icon: 'database', ee: true } },
      { group: 'Infrastructure', icon: 'server', leaf: { key: 'infra.logs', label: 'Logs & Retention', icon: 'audit' } },
      { group: 'General', icon: 'general', leaf: { key: 'general.orgs', label: 'Organisations', icon: 'users' } },
    ];
    for (const { group, icon, leaf } of want) {
      let g = nav.find(x => x.label === group);
      if (!g) { g = { label: group, icon, items: [] }; nav.push(g); }
      if (!g.items.some(it => it.key === leaf.key)) g.items.push(leaf);
    }
    navData = nav;
  }
  async function saveSetting(key: string, value: string): Promise<boolean> {
    await execFlow('a3t-settings', 'a3t', { input: { action: 'delete', org_id: org(), key } }, { silent: true });
    const res = await execFlow('a3t-settings', 'a3t', {
      input: { action: 'set', org_id: org(), key, value, user_id: $currentUser?.id ?? 'usr-superadmin', timestamp: new Date().toISOString() }
    }, { silent: true });
    if (res.ok) settings = { ...settings, [key]: value };
    return res.ok;
  }
  onMount(loadAll);

  // ── Navigation taxonomy ──
  interface Leaf { key: string; label: string; icon: string; ee?: boolean }
  interface Group { label: string; icon: string; ee?: boolean; items: Leaf[] }
  const NAV: Group[] = [
    { label: 'General', icon: 'general', items: [
      { key: 'general.platform', label: 'Platform', icon: 'box' },
      { key: 'general.runtime', label: 'Runtime', icon: 'swap' },
      { key: 'general.appearance', label: 'Appearance', icon: 'palette' },
      { key: 'general.orgs', label: 'Organisations', icon: 'users' },
    ]},
    { label: 'Security', icon: 'shield', ee: true, items: [
      { key: 'security.authn', label: 'Authentication', icon: 'key', ee: true },
      { key: 'security.authz', label: 'Authorization', icon: 'lockcheck', ee: true },
      { key: 'security.policy', label: 'Policy Engine', icon: 'scale', ee: true },
      { key: 'security.audit', label: 'Audit Log', icon: 'audit', ee: true },
      { key: 'security.monitoring', label: 'Monitoring', icon: 'pulse', ee: true },
      { key: 'security.compliance', label: 'Compliance', icon: 'badge', ee: true },
    ]},
    { label: 'Identity & Access', icon: 'users', ee: true, items: [
      { key: 'iam.users', label: 'Users', icon: 'user', ee: true },
      { key: 'iam.roles', label: 'Roles', icon: 'usercog', ee: true },
      { key: 'iam.idp', label: 'Identity Providers', icon: 'plug', ee: true },
    ]},
    { label: 'Infrastructure', icon: 'server', items: [
      { key: 'infra.clusters', label: 'Clusters', icon: 'nodes' },
      { key: 'infra.storage', label: 'Storage & Data', icon: 'database' },
      { key: 'infra.backend', label: 'Storage Backend', icon: 'database', ee: true },
      { key: 'infra.platformdb', label: 'Platform Database', icon: 'database', ee: true },
      { key: 'infra.adbembedding', label: 'ADB Embeddings', icon: 'database', ee: true },
      { key: 'infra.adbcache', label: 'ADB Memory Cache', icon: 'database', ee: true },
      { key: 'infra.logs', label: 'Logs & Retention', icon: 'audit' },
      { key: 'infra.registry', label: 'Image Registry', icon: 'box' },
    ]},
    { label: 'Integrations', icon: 'plug', items: [
      { key: 'integrations.notifications', label: 'Notifications', icon: 'bell' },
      { key: 'integrations.webhooks', label: 'Webhooks', icon: 'webhook' },
    ]},
    { label: 'Billing', icon: 'billing', ee: true, items: [
      { key: 'billing.plan', label: 'Plan & Usage', icon: 'chart', ee: true },
      { key: 'billing.payment', label: 'Payment Methods', icon: 'card', ee: true },
      { key: 'billing.invoices', label: 'Invoices', icon: 'receipt', ee: true },
    ]},
    { label: 'Advanced', icon: 'sliders', items: [
      { key: 'advanced.apikeys', label: 'API Keys', icon: 'key' },
      { key: 'advanced.flags', label: 'Feature Flags', icon: 'flag' },
      { key: 'advanced.telemetry', label: 'Telemetry', icon: 'pulse' },
    ]},
  ];
  // The menu is pulled from settingsCRUD (`_menu`) on load and on runtime
  // change; NAV is the built-in fallback if the runtime hasn't seeded one.
  let navData = $state<Group[]>(NAV);
  const LEAF = $derived.by(() => {
    const m: Record<string, Leaf & { group: string }> = {};
    for (const g of navData) for (const it of g.items) m[it.key] = { ...it, group: g.label };
    return m;
  });
  // Edition of the connected runtime (from settingsCRUD `_edition`); EE features
  // are locked unless the connected runtime reports 'ee'.
  let edition = $state<'ce' | 'ee'>('ee');

  // Platform database (ADB stage G): where A3T's registry tables live.
  let pdb = $state<PlatformDbStatus | null>(null);
  let pdbUrl = $state('http://adb.a3t.svc.cluster.local:3000');
  let pdbFlight = $state('');
  let pdbBusy = $state(false);
  let pdbMsg = $state('');
  let pdbOk = $state(false);
  async function loadPlatformDb() {
    const r = await getPlatformDb();
    if (r.ok && r.data) {
      pdb = r.data;
      if (r.data.config?.url) pdbUrl = String(r.data.config.url);
      if (r.data.config?.flight_url) pdbFlight = String(r.data.config.flight_url);
    } else {
      pdbMsg = r.error ?? 'Could not read the platform database setting';
      pdbOk = false;
    }
  }
  async function switchTo(backend: 'adb' | 'lancedb') {
    const where = backend === 'adb' ? `ADB at ${pdbUrl}` : 'the embedded LanceDB';
    if (!confirm(`Copy all platform data (agents, templates, canvases, users, settings, audit) to ${where} and switch A3T to it?\n\nThe copy replaces what is there. Avoid edits in the console while it runs.`)) return;
    pdbBusy = true;
    pdbMsg = '';
    const r = await switchPlatformDb(backend === 'adb' ? { backend, url: pdbUrl, ...(pdbFlight ? { flight_url: pdbFlight } : {}) } : { backend });
    pdbBusy = false;
    pdbOk = r.ok;
    pdbMsg = r.ok && r.data ? `Switched to ${r.data.to}: ${r.data.tables} tables, ${r.data.rows} rows copied.` : (r.error ?? 'Switch failed');
    await loadPlatformDb();
  }
  $effect(() => { if (leaf === 'infra.platformdb' && !pdb) loadPlatformDb(); });

  // ADB embeddings (ADB W9.10): which model ADB embeds memories with.
  let embUrl = $state('http://adb.a3t.svc.cluster.local:3000');
  let emb = $state<AdbEmbeddingStatus | null>(null);
  let embForm = $state<AdbEmbeddingSettings>({ provider: 'local', model: 'bge-small-en-v1.5', dimension: 384 });
  let embBusy = $state(false);
  let embMsg = $state('');
  let embOk = $state(false);
  async function loadEmbedding() {
    embMsg = '';
    const r = await getAdbEmbedding(embUrl);
    if (r.ok && r.data?.success && r.data.data) {
      emb = r.data.data;
      embForm = { ...r.data.data.settings };
    } else {
      emb = null;
      embOk = false;
      embMsg = r.data?.error ?? r.error ?? 'Could not read the ADB embedding settings';
    }
  }
  async function saveEmbedding() {
    if (!confirm(`Save these embedding settings on ${embUrl}?\n\nADB checks them with a sample embedding, then restarts (a few seconds). A different model or size re-embeds every memory in the background; recall by meaning is partial until it finishes.`)) return;
    embBusy = true;
    embMsg = '';
    const settings: AdbEmbeddingSettings = embForm.provider === 'local'
      ? { provider: 'local', model: embForm.model, dimension: Number(embForm.dimension) }
      : { provider: 'openai', model: embForm.model, dimension: Number(embForm.dimension), url: embForm.url, ...(embForm.api_key_env ? { api_key_env: embForm.api_key_env } : {}) };
    const r = await setAdbEmbedding(embUrl, settings);
    embBusy = false;
    embOk = !!(r.ok && r.data?.success);
    embMsg = embOk
      ? `Saved. ADB is restarting${r.data?.data?.reembed ? ' and will re-embed every memory' : ''}; refresh in a few seconds.`
      : (r.data?.error ?? r.error ?? 'Save failed');
  }
  $effect(() => { if (leaf === 'infra.adbembedding' && !emb && !embMsg) loadEmbedding(); });

  // ADB memory cache (ADB W10): what ADB keeps in memory, per memory type and scope.
  type KeepKind = 'all' | 'none' | 'last' | 'bytes' | 'within';
  type CacheRow = { memory: AdbCacheRule['memory']; scope: string; kind: KeepKind; value: string };
  const MEMORIES: AdbCacheRule['memory'][] = ['working', 'episodic', 'semantic', 'procedural', 'tools'];
  let cacheUrl = $state('http://adb.a3t.svc.cluster.local:3000');
  let cache = $state<AdbCacheStatus | null>(null);
  let cacheRows = $state<CacheRow[]>([]);
  let cacheMax = $state('256MB');
  let cacheVectors = $state(true);
  let cacheBusy = $state(false);
  let cacheMsg = $state('');
  let cacheOk = $state(false);
  const mb = (n: number) => `${(n / 1048576).toFixed(1)} MB`;
  function toRow(r: AdbCacheRule): CacheRow {
    const k = r.keep;
    if (k === 'all' || k === 'none') return { memory: r.memory, scope: r.scope ?? '*', kind: k, value: '' };
    if ('last' in k) return { memory: r.memory, scope: r.scope ?? '*', kind: 'last', value: String(k.last) };
    if ('bytes' in k) return { memory: r.memory, scope: r.scope ?? '*', kind: 'bytes', value: typeof k.bytes === 'number' ? mb(k.bytes).replace(' ', '') : k.bytes };
    return { memory: r.memory, scope: r.scope ?? '*', kind: 'within', value: k.within };
  }
  function toRule(r: CacheRow): AdbCacheRule {
    const keep: AdbCacheKeep = r.kind === 'all' || r.kind === 'none' ? r.kind
      : r.kind === 'last' ? { last: Number(r.value) }
      : r.kind === 'bytes' ? { bytes: r.value }
      : { within: r.value };
    return { memory: r.memory, scope: r.scope.trim() || '*', keep };
  }
  async function loadCache() {
    cacheMsg = '';
    const r = await getAdbCache(cacheUrl);
    if (r.ok && r.data?.success && r.data.data) {
      cache = r.data.data;
      cacheRows = cache.settings.rules.map(toRow);
      cacheMax = typeof cache.settings.max_bytes === 'number' ? mb(cache.settings.max_bytes).replace(' ', '') : cache.settings.max_bytes;
      cacheVectors = cache.settings.vectors ?? true;
    } else {
      cache = null;
      cacheOk = false;
      cacheMsg = r.data?.error ?? r.error ?? 'Could not read the ADB cache settings';
    }
  }
  async function saveCache() {
    cacheBusy = true;
    cacheMsg = '';
    const r = await setAdbCache(cacheUrl, { max_bytes: cacheMax, rules: cacheRows.map(toRule), vectors: cacheVectors });
    cacheBusy = false;
    cacheOk = !!(r.ok && r.data?.success);
    cacheMsg = cacheOk ? `Saved and applied in ${r.data?.data?.took_ms ?? 0} ms.` : (r.data?.error ?? r.error ?? 'Save failed');
    if (cacheOk) await loadCache();
  }
  $effect(() => { if (leaf === 'infra.adbcache' && !cache && !cacheMsg) loadCache(); });
  const isLocked = (key: string) => !!LEAF[key]?.ee && edition !== 'ee';
  const slug = (s: string) => s.toLowerCase().replace(/[^a-z0-9]+/g, '_').replace(/^_|_$/g, '');

  // Representative controls per scaffold page (disabled = EE-locked / coming soon).
  type Ctl = { type: 'toggle' | 'select' | 'text'; label: string; hint?: string; options?: string[]; on?: boolean };
  const META: Record<string, { title: string; desc: string; ee?: boolean; controls: Ctl[] }> = {
    'general.appearance': { title: 'Appearance', desc: 'Theme and console display.', controls: [
      { type: 'select', label: 'Theme', options: ['System', 'Light', 'Dark'] },
      { type: 'toggle', label: 'Compact density', on: false },
    ]},
    'security.authn': { title: 'Authentication', desc: 'How users prove who they are.', ee: true, controls: [
      { type: 'toggle', label: 'Single Sign-On (OIDC/SAML)', hint: 'Federate with Entra, Okta, Google', on: false },
      { type: 'toggle', label: 'Require MFA', on: true },
      { type: 'toggle', label: 'Passkeys', on: false },
      { type: 'select', label: 'Session lifetime', options: ['8 hours', '24 hours', '7 days'] },
    ]},
    'security.authz': { title: 'Authorization', desc: 'What users and agents may do.', ee: true, controls: [
      { type: 'toggle', label: 'Enforce RBAC + ABAC', on: true },
      { type: 'select', label: 'Policy language', options: ['Cedar', 'OPA / Rego'] },
      { type: 'toggle', label: 'Field-level authorization', on: true },
    ]},
    'security.policy': { title: 'Policy Engine', desc: 'The decision point for every request.', ee: true, controls: [
      { type: 'select', label: 'Engine', options: ['Cedar (built-in)', 'External PDP'] },
      { type: 'toggle', label: 'Shadow mode (log, don’t enforce)', on: false },
      { type: 'toggle', label: 'Static analysis on publish', on: true },
    ]},
    'security.audit': { title: 'Audit Log', desc: 'Immutable record of authN + authZ decisions.', ee: true, controls: [
      { type: 'toggle', label: 'Immutable append-only log', on: true },
      { type: 'select', label: 'SIEM export', options: ['None', 'Splunk', 'Microsoft Sentinel', 'S3'] },
    ]},
    'security.monitoring': { title: 'Monitoring', desc: 'Detect anomalous access.', ee: true, controls: [
      { type: 'toggle', label: 'Anomaly detection', on: false },
      { type: 'toggle', label: 'UEBA', on: false },
    ]},
    'security.compliance': { title: 'Compliance', desc: 'Evidence for auditors.', ee: true, controls: [
      { type: 'select', label: 'Framework', options: ['SOC 2', 'ISO 27001', 'HIPAA'] },
      { type: 'toggle', label: 'Access certification campaigns', on: false },
    ]},
    'iam.users': { title: 'Users', desc: 'People with access to this platform.', ee: true, controls: [
      { type: 'toggle', label: 'Directory sync (SCIM)', on: false },
      { type: 'toggle', label: 'Auto-disable on IdP removal', on: true },
    ]},
    'iam.roles': { title: 'Roles', desc: 'Named sets of permissions.', ee: true, controls: [
      { type: 'toggle', label: 'Allow custom roles', on: true },
      { type: 'select', label: 'Default role for new users', options: ['Viewer', 'Developer'] },
    ]},
    'iam.idp': { title: 'Identity Providers', desc: 'Federate identity with your directory.', ee: true, controls: [
      { type: 'select', label: 'Provider', options: ['Microsoft Entra ID', 'Okta', 'Google', 'Keycloak'] },
      { type: 'text', label: 'Discovery URL', hint: '.well-known/openid-configuration' },
      { type: 'toggle', label: 'SailPoint / Saviynt (IGA)', on: false },
    ]},
    'infra.clusters': { title: 'Clusters', desc: 'Runtime targets for agents and flows.', controls: [
      { type: 'toggle', label: 'Auto-scale worker pool', on: true },
      { type: 'select', label: 'Default cluster', options: ['vc-dev', 'vprod'] },
    ]},
    'infra.storage': { title: 'Storage & Data', desc: 'Where platform data lives.', controls: [
      { type: 'text', label: 'LanceDB path', hint: '/data/a3t' },
      { type: 'select', label: 'Retention', options: ['30 days', '90 days', '1 year', 'Forever'] },
    ]},
    'infra.backend': { title: 'Storage Backend', desc: 'Which store backs flow state, the event log, and the dispatch queue. LanceDB is embedded (zero-install) and is the only option on Community. Enterprise can point at an external store for HA/scale — applies on restart, set via deployment env/secret.', ee: true, controls: [
      { type: 'select', label: 'Backend', hint: 'Selected at boot; external backends are Enterprise-only', options: ['LanceDB (embedded)', 'Dragonfly', 'Redis / Valkey', 'Iggy', 'ADB / ClawDB'] },
      { type: 'text', label: 'Connection URL', hint: 'e.g. redis://dragonfly:6379 — ignored for LanceDB' },
      { type: 'text', label: 'Event log backend', hint: 'Override just the event log/queue (blank = same as Backend)' },
    ]},
    'infra.adbcache': { title: 'ADB Memory Cache', desc: 'Which memories ADB keeps in memory, per memory type and scope, for fast reads.', ee: true, controls: [] },
    'infra.adbembedding': { title: 'ADB Embeddings', desc: 'The model ADB uses to find memories by meaning: its built-in model or an OpenAI-compatible endpoint.', ee: true, controls: [] },
    'infra.platformdb': { title: 'Platform Database', desc: "Where A3T's own data lives: the embedded LanceDB or ADB (Enterprise). Switch either way; data is copied across.", ee: true, controls: [] },
    'infra.logs': { title: 'Logs & Retention', desc: 'How long run history and decision logs are kept on disk, and how noisy the runtime is.', controls: [
      { type: 'select', label: 'Execution history retention', hint: 'How long finished runs are kept on disk', options: ['7 days', '14 days', '30 days', '90 days', '1 year', 'Forever'] },
      { type: 'select', label: 'Decision log retention', hint: 'Cedar allow/deny audit records', options: ['7 days', '30 days', '90 days', '1 year', 'Forever'] },
      { type: 'text', label: 'Max log disk usage (GB)', hint: 'Purge oldest beyond this cap' },
      { type: 'select', label: 'Runtime log level', hint: 'RUST_LOG verbosity', options: ['error', 'warn', 'info', 'debug', 'trace'] },
      { type: 'toggle', label: 'Auto-purge past retention', hint: 'Prune old runs/logs on a daily sweep', on: true },
    ]},
    'infra.registry': { title: 'Image Registry', desc: 'Where runtime images are pulled from.', controls: [
      { type: 'text', label: 'Registry', hint: 'ghcr.io/srirammails' },
      { type: 'toggle', label: 'Require signed images', on: false },
    ]},
    'integrations.notifications': { title: 'Notifications', desc: 'Where the platform sends alerts.', controls: [
      { type: 'toggle', label: 'Email', on: true },
      { type: 'toggle', label: 'Slack', on: false },
    ]},
    'integrations.webhooks': { title: 'Webhooks', desc: 'Outbound events to external systems.', controls: [
      { type: 'toggle', label: 'Enable outbound webhooks', on: false },
      { type: 'text', label: 'Signing secret', hint: 'whsec_…' },
    ]},
    'billing.plan': { title: 'Plan & Usage', desc: 'Your subscription and consumption.', ee: true, controls: [
      { type: 'select', label: 'Plan', options: ['Community', 'Team', 'Enterprise'] },
    ]},
    'billing.payment': { title: 'Payment Methods', desc: 'How you pay.', ee: true, controls: [
      { type: 'text', label: 'Billing email', hint: 'billing@company.com' },
    ]},
    'billing.invoices': { title: 'Invoices', desc: 'Billing history.', ee: true, controls: [] },
    'advanced.apikeys': { title: 'API Keys', desc: 'Programmatic access to the platform.', controls: [
      { type: 'toggle', label: 'Allow personal API keys', on: true },
    ]},
    'advanced.flags': { title: 'Feature Flags', desc: 'Toggle experimental behavior.', controls: [
      { type: 'toggle', label: 'New flow editor', on: false },
      { type: 'toggle', label: 'Beta agent runtime', on: false },
    ]},
    'advanced.telemetry': { title: 'Telemetry', desc: 'Product analytics and diagnostics.', controls: [
      { type: 'toggle', label: 'Usage telemetry', on: true },
      { type: 'toggle', label: 'Error reporting', on: true },
    ]},
  };

  // Left menu selects a top-level category ('all' or a group label); sub-items
  // are shown inside the category page and drilled into via `leaf`.
  let active = $state('all');
  let leaf = $state('');
  const GROUP = $derived(navData.find(g => g.label === active));
  function selectGroup(label: string) { active = label; leaf = ''; }
  function openLeaf(key: string) { leaf = key; }
  function back() { leaf = ''; }

  // ── general.platform (persisted) ──
  let platformName = $state('');
  let platformSaving = $state(false); let platformMsg = $state('');
  $effect(() => { platformName = settings['general.platform_name'] ?? ''; });
  async function savePlatform() { platformSaving = true; platformMsg = ''; const ok = await saveSetting('general.platform_name', platformName.trim()); platformMsg = ok ? 'Saved' : 'Save failed'; platformSaving = false; }

  // ── general.runtime (switcher) ──
  const defaultBase = getDefaultApiBase();
  const ceBase = defaultBase.replace('://console.', '://console-ce.');
  const currentOverride = getApiOverride();
  let rtMode = $state(!currentOverride ? 'ee' : (currentOverride === ceBase ? 'ce' : 'custom'));
  let customUrl = $state(currentOverride && currentOverride !== ceBase ? currentOverride : '');
  let testMsg = $state(''); let testOk = $state(false); let testing = $state(false);
  const runtimes = [
    { id: 'ee', label: 'Enterprise (EE)', url: defaultBase, note: 'JWT auth · RBAC · Cedar (when licensed)' },
    { id: 'ce', label: 'Community (CE)', url: ceBase, note: 'Permissive — login only, everyone edits' },
    { id: 'custom', label: 'Custom backend', url: '', note: 'Point the console at any runtime URL' },
  ];
  function rtTarget(): string { return rtMode === 'ee' ? '' : rtMode === 'ce' ? ceBase : customUrl.trim().replace(/\/+$/, ''); }
  async function testConn() { const url = rtTarget() || defaultBase; testing = true; testMsg = ''; try { const r = await fetch(`${url}/health`); const d = await r.json().catch(() => ({})); testOk = r.ok; testMsg = r.ok ? `Reachable — ${d.service ?? 'ok'} v${d.version ?? '?'}` : `HTTP ${r.status}`; } catch { testOk = false; testMsg = 'Unreachable'; } testing = false; }
  function applyRuntime() { if (rtMode === 'ee') clearApiOverride(); else { const u = rtTarget(); if (!u) { testMsg = 'Enter a URL'; testOk = false; return; } setApiOverride(u); } window.location.reload(); }
  const activeRt = $derived(!currentOverride ? 'Enterprise (EE) — default' : currentOverride === ceBase ? 'Community (CE)' : 'Custom');

  function svg(name: string) { return IC[name] ?? IC.general; }

</script>

<div class="screen-header">
  <div class="breadcrumb">Settings <span class="sep">/</span> {active === 'all' ? 'All' : active}{#if leaf}<span class="sep">/</span> {LEAF[leaf]?.label}{/if}</div>
  <div class="screen-title-row">
    <h1 class="screen-title">Settings</h1>
    <span class="ed-pill" class:ee={edition === 'ee'}>
      Connected: {edition === 'ee' ? 'Enterprise' : 'Community'}
      <span class="tag {edition === 'ee' ? 'ee' : 'ce'}">{edition.toUpperCase()}</span>
    </span>
  </div>
</div>

<div class="settings-layout">
  <!-- Flat category menu -->
  <nav class="set-menu">
    <button class="set-item" class:active={active === 'all'} onclick={() => selectGroup('all')}>
      <svg class="s-ico" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.4">{@html svg('all')}</svg>
      <span class="s-lbl">All Settings</span>
    </button>
    {#each navData as group}
      <button class="set-item" class:active={active === group.label} onclick={() => selectGroup(group.label)}>
        <svg class="s-ico" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.4">{@html svg(group.icon)}</svg>
        <span class="s-lbl">{group.label}</span>
        {#if group.ee}<span class="tag ee sm">EE</span>{/if}
      </button>
    {/each}
  </nav>

  <!-- Content -->
  <div class="settings-body">
    {#if leaf}
      <button class="back-link" onclick={back}>
        <svg width="13" height="13" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="2"><path d="M10 4l-4 4 4 4"/></svg>
        {active}
      </button>
    {/if}

    {#if !leaf}
      <PageBanner
        headline="Configure your platform."
        highlightWord="platform"
        subtitle="Identity, security, infrastructure and billing — all in one place."
        description="Community Edition runs open — anyone who signs in can edit. Enterprise adds roles, Cedar policy, SSO and audit. Enterprise settings are marked EE."
        accent="spec"
        docsPath="/settings"
      />
    {/if}

    {#if !leaf && active === 'all'}
      <!-- Overview: every group as a section of cards -->
      {#each navData as group}
        <section class="ov-group">
          <div class="ov-head">
            <svg class="ov-ico" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.4">{@html svg(group.icon)}</svg>
            <h3>{group.label}</h3>
            {#if group.ee}<span class="tag ee">Enterprise</span>{/if}
          </div>
          <div class="ov-grid">
            {#each group.items as item}
              <button class="ov-card" onclick={() => openLeaf(item.key)}>
                <svg class="ov-card-ico" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.4">{@html svg(item.icon)}</svg>
                <div class="ov-card-body">
                  <div class="ov-card-name">{item.label}{#if item.ee}<span class="tag ee sm">EE</span>{/if}</div>
                  <div class="ov-card-desc">{META[item.key]?.desc ?? (item.key === 'general.platform' ? 'Platform identity.' : item.key === 'general.runtime' ? 'Switch CE / EE runtime.' : '')}</div>
                </div>
                <svg class="ov-arrow" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.6"><path d="M6 4l4 4-4 4"/></svg>
              </button>
            {/each}
          </div>
        </section>
      {/each}

    {:else if !leaf && GROUP}
      <!-- Selected category: its sub-items as cards to drill into -->
      <div class="cat-intro">
        <svg class="cat-ico" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.4">{@html svg(GROUP.icon)}</svg>
        <div><h2>{GROUP.label}{#if GROUP.ee}<span class="tag ee">Enterprise</span>{/if}</h2></div>
      </div>
      <div class="ov-grid">
        {#each GROUP.items as item}
          <button class="ov-card" onclick={() => openLeaf(item.key)}>
            <svg class="ov-card-ico" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.4">{@html svg(item.icon)}</svg>
            <div class="ov-card-body">
              <div class="ov-card-name">{item.label}{#if item.ee}<span class="tag ee sm">EE</span>{/if}</div>
              <div class="ov-card-desc">{META[item.key]?.desc ?? (item.key === 'general.platform' ? 'Platform identity.' : item.key === 'general.runtime' ? 'Switch CE / EE runtime.' : '')}</div>
            </div>
            <svg class="ov-arrow" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.6"><path d="M6 4l4 4-4 4"/></svg>
          </button>
        {/each}
      </div>

    {:else if leaf === 'general.orgs'}
      <OrgsSettings />

    {:else if leaf === 'infra.platformdb'}
      <div class="card">
        <div class="card-head"><h2>Platform Database</h2>{#if pdb}<span class="active-pill">Active: {pdb.backend === 'adb' ? 'ADB' : 'Embedded LanceDB'}</span>{/if}</div>
        <p class="card-sub">Where A3T keeps its own data: agents, templates, canvases, users, settings and the audit log. The embedded LanceDB lives inside each FlowR; ADB (Enterprise) is one durable copy that every FlowR instance shares. Switching copies everything to the target first, so you can move back at any time.</p>
        {#if pdb}
          <p class="mono dim">Now: {pdb.describe}</p>
          {#if !pdb.adb_supported}
            <p class="msg">This FlowR build is Community Edition: ADB platform tables need Enterprise.</p>
          {/if}
          <label class="pdb-field">ADB URL <input class="mono" bind:value={pdbUrl} placeholder="http://adb.a3t.svc.cluster.local:3000" /></label>
          <label class="pdb-field">Flight URL <input class="mono" bind:value={pdbFlight} placeholder="(blank: the same host, port 3001)" /></label>
          <div class="row-actions">
            {#if pdb.backend === 'adb'}
              <button class="btn ghost" onclick={() => switchTo('lancedb')} disabled={pdbBusy}>{pdbBusy ? 'Copying…' : 'Switch back to embedded LanceDB'}</button>
            {:else}
              <button class="btn spec" onclick={() => switchTo('adb')} disabled={pdbBusy || !pdb.adb_supported}>{pdbBusy ? 'Copying…' : 'Switch to ADB'}</button>
            {/if}
            {#if pdbMsg}<span class="msg" class:ok={pdbOk}>{pdbMsg}</span>{/if}
          </div>
        {:else if pdbMsg}
          <p class="msg">{pdbMsg}</p>
        {:else}
          <p class="dim">Loading…</p>
        {/if}
      </div>

    {:else if leaf === 'infra.adbembedding'}
      <div class="card">
        <div class="card-head"><h2>ADB Embeddings</h2>{#if emb?.active}<span class="active-pill">Active: {emb.active.model} ({emb.active.dimension})</span>{/if}</div>
        <p class="card-sub">ADB turns each memory's text into a vector so agents can recall by meaning. It can run its built-in model on its own CPU, or call an OpenAI-compatible embeddings endpoint (OpenAI, Azure, Ollama, vLLM, TEI…). The endpoint's API key stays in ADB's environment: give the name of a variable set from a Kubernetes Secret.</p>
        <label class="pdb-field">ADB URL <input class="mono" bind:value={embUrl} placeholder="http://adb.a3t.svc.cluster.local:3000" onchange={loadEmbedding} /></label>
        {#if emb}
          <p class="mono dim">{emb.source === 'default' ? 'Default settings' : 'Saved settings'}{#if emb.pending > 0} · {emb.pending} memories waiting for a vector{/if}</p>
          <label class="pdb-field">Provider
            <select bind:value={embForm.provider}>
              <option value="local">Built-in model (on ADB's CPU)</option>
              <option value="openai">OpenAI-compatible endpoint</option>
            </select>
          </label>
          <label class="pdb-field">Model <input class="mono" bind:value={embForm.model} placeholder={embForm.provider === 'local' ? 'bge-small-en-v1.5' : 'text-embedding-3-small'} /></label>
          <label class="pdb-field">Dimension <input class="mono" type="number" min="1" max="8192" bind:value={embForm.dimension} /></label>
          {#if embForm.provider === 'openai'}
            <label class="pdb-field">API base URL <input class="mono" bind:value={embForm.url} placeholder="https://api.openai.com/v1" /></label>
            <label class="pdb-field">API key variable <input class="mono" bind:value={embForm.api_key_env} placeholder="OPENAI_API_KEY (blank: no key)" /></label>
          {/if}
          <div class="row-actions">
            <button class="btn ghost" onclick={loadEmbedding} disabled={embBusy}>Refresh</button>
            <button class="btn spec" onclick={saveEmbedding} disabled={embBusy}>{embBusy ? 'Checking…' : 'Save and restart ADB'}</button>
            {#if embMsg}<span class="msg" class:ok={embOk}>{embMsg}</span>{/if}
          </div>
        {:else if embMsg}
          <p class="msg">{embMsg}</p>
          <div class="row-actions"><button class="btn ghost" onclick={loadEmbedding}>Retry</button></div>
        {:else}
          <p class="dim">Loading…</p>
        {/if}
      </div>

    {:else if leaf === 'infra.adbcache'}
      <div class="card">
        <div class="card-head"><h2>ADB Memory Cache</h2>{#if cache}<span class="active-pill">{cache.mode === 'off' ? 'Off' : cache.mode === 'shadow' ? 'On (checked against storage)' : 'On'}</span>{/if}</div>
        <p class="card-sub">ADB keeps recent memories in its own memory, so agents read them in well under a millisecond instead of from storage. Each rule says how much of a memory type to keep, per scope; the first rule that matches a scope wins (<span class="mono">*</span> matches any part). Reads the cache can't answer whole go to storage, so results never change, only speed. Saving applies at once, without a restart.</p>
        <label class="pdb-field">ADB URL <input class="mono" bind:value={cacheUrl} placeholder="http://adb.a3t.svc.cluster.local:3000" onchange={loadCache} /></label>
        {#if cache}
          <p class="mono dim">{cache.source === 'settings' ? 'Saved rules' : cache.source === 'environment' ? 'Rules from ADB_CACHE_RULES' : 'Default rules'}{#if cache.stats} · {cache.stats.rows} memories, {mb(cache.stats.bytes)} of {mb(cache.stats.max_bytes)} · {cache.stats.hits} reads from the cache, {cache.stats.misses} from storage{#if cache.stats.mismatches > 0} · {cache.stats.mismatches} mismatches{/if}{/if}</p>
          <div class="cache-rules">
            <div class="cache-rule head"><span>Memory type</span><span>Scope</span><span>Keep</span><span></span><span></span></div>
            {#each cacheRows as row, i}
              <div class="cache-rule">
                <select bind:value={row.memory}>{#each MEMORIES as m}<option value={m}>{m}</option>{/each}</select>
                <input class="mono" bind:value={row.scope} placeholder="*" />
                <select bind:value={row.kind}>
                  <option value="all">everything</option>
                  <option value="last">the newest n</option>
                  <option value="bytes">the newest that fit in</option>
                  <option value="within">the last (time)</option>
                  <option value="none">nothing</option>
                </select>
                {#if row.kind === 'all' || row.kind === 'none'}<span></span>{:else}<input class="mono" bind:value={row.value} placeholder={row.kind === 'last' ? '30' : row.kind === 'bytes' ? '10MB' : '24h'} />{/if}
                <button class="btn ghost" title="Remove rule" onclick={() => (cacheRows = cacheRows.filter((_, j) => j !== i))}>✕</button>
              </div>
            {/each}
          </div>
          <div class="row-actions"><button class="btn ghost" onclick={() => (cacheRows = [...cacheRows, { memory: 'episodic', scope: '*', kind: 'last', value: '30' }])}>Add rule</button></div>
          <label class="pdb-field">Cache size limit (whole ADB) <input class="mono" bind:value={cacheMax} placeholder="256MB" /></label>
          <label class="pdb-check"><input type="checkbox" bind:checked={cacheVectors} /> Keep vectors in the cache, so similarity search (LIKE) is answered from memory (about 0.75 KB per memory)</label>
          <div class="row-actions">
            <button class="btn ghost" onclick={loadCache} disabled={cacheBusy}>Refresh</button>
            <button class="btn spec" onclick={saveCache} disabled={cacheBusy || cache.mode === 'off'}>{cacheBusy ? 'Applying…' : 'Save and apply'}</button>
            {#if cache.mode === 'off'}<span class="msg">The cache is off (ADB_CACHE=off).</span>{/if}
            {#if cacheMsg}<span class="msg" class:ok={cacheOk}>{cacheMsg}</span>{/if}
          </div>
          {#if cache.stats && cache.stats.buckets.length > 0}
            <div class="cache-rules buckets">
              <div class="cache-rule head"><span>Memory type</span><span>Scope</span><span>Memories</span><span>Size</span><span>All kept</span></div>
              {#each cache.stats.buckets as b}
                <div class="cache-rule"><span>{b.memory}</span><span class="mono">{b.scope}</span><span>{b.rows}</span><span>{mb(b.bytes)}</span><span>{b.complete ? 'yes' : 'newest only'}</span></div>
              {/each}
            </div>
          {/if}
        {:else if cacheMsg}
          <p class="msg">{cacheMsg}</p>
          <div class="row-actions"><button class="btn ghost" onclick={loadCache}>Retry</button></div>
        {:else}
          <p class="dim">Loading…</p>
        {/if}
      </div>

    {:else if leaf === 'general.runtime'}
      <div class="card">
        <div class="card-head"><h2>Runtime</h2><span class="active-pill">Active: {activeRt}</span></div>
        <p class="card-sub">Flip the console between the CE and EE runtimes to test. Applying reloads so every request uses the selected backend.</p>
        <div class="rt-list">
          {#each runtimes as rt}
            <label class="rt-opt" class:sel={rtMode === rt.id}>
              <input type="radio" name="rt" value={rt.id} bind:group={rtMode} />
              <div class="rt-info">
                <div class="rt-label">{rt.label}{#if rt.id === 'ce'}<span class="tag ce">CE</span>{:else if rt.id === 'ee'}<span class="tag ee">EE</span>{/if}</div>
                <div class="rt-note">{rt.note}</div>
                {#if rt.id !== 'custom' && rt.url}<div class="rt-url mono">{rt.url}</div>{/if}
                {#if rt.id === 'custom' && rtMode === 'custom'}<input class="rt-custom mono" bind:value={customUrl} placeholder="https://console-xyz.dev.a3t.io" />{/if}
              </div>
            </label>
          {/each}
        </div>
        <div class="row-actions">
          <button class="btn ghost" onclick={testConn} disabled={testing}>{testing ? 'Testing…' : 'Test connection'}</button>
          {#if testMsg}<span class="msg" class:ok={testOk}>{testMsg}</span>{/if}
          <span style="flex:1"></span>
          <button class="btn spec" onclick={applyRuntime}>Apply &amp; reload</button>
        </div>
        <p class="mono dim">Current API base: {API_BASE || '(same origin)'}</p>
      </div>

    {:else if leaf === 'general.platform'}
      <div class="card">
        <h2>Platform</h2>
        <p class="card-sub">Basic platform identity. Persisted to the settings store.</p>
        <div class="ctl-row"><div class="ctl-label"><strong>Platform name</strong></div>
          <input class="ctl-input" type="text" bind:value={platformName} placeholder="A3T" />
        </div>
        <div class="row-actions">
          <button class="btn spec" onclick={savePlatform} disabled={platformSaving}>{platformSaving ? 'Saving…' : 'Save'}</button>
          {#if platformMsg}<span class="msg" class:ok={platformMsg === 'Saved'}>{platformMsg}</span>{/if}
        </div>
      </div>

    {:else if META[leaf]}
      {@const m = META[leaf]}
      <div class="card">
        <div class="card-head">
          <div class="card-title">
            <svg class="card-ico" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.4">{@html svg(LEAF[leaf].icon)}</svg>
            <h2>{m.title}</h2>
          </div>
          {#if m.ee}<span class="tag ee">Enterprise Edition</span>{/if}
        </div>
        <p class="card-sub">{m.desc}</p>

        {#if m.controls.length}
          {@const lock = isLocked(leaf)}
          <div class="ctl-list" class:locked={lock}>
            {#each m.controls as c}
              {@const ck = leaf + '.' + slug(c.label)}
              <div class="ctl-row">
                <div class="ctl-label"><strong>{c.label}</strong>{#if c.hint}<span class="ctl-hint">{c.hint}</span>{/if}</div>
                {#if c.type === 'toggle'}
                  <label class="switch">
                    <input type="checkbox" disabled={lock}
                      checked={settings[ck] != null ? settings[ck] === 'true' : !!c.on}
                      onchange={(e) => saveSetting(ck, String((e.currentTarget as HTMLInputElement).checked))} />
                    <span class="track"></span>
                  </label>
                {:else if c.type === 'select'}
                  <select class="ctl-input sel" disabled={lock}
                    value={settings[ck] ?? c.options?.[0]}
                    onchange={(e) => saveSetting(ck, (e.currentTarget as HTMLSelectElement).value)}>
                    {#each c.options ?? [] as o}<option>{o}</option>{/each}
                  </select>
                {:else}
                  <input class="ctl-input" type="text" placeholder={c.hint ?? ''} disabled={lock}
                    value={settings[ck] ?? ''}
                    onchange={(e) => saveSetting(ck, (e.currentTarget as HTMLInputElement).value)} />
                {/if}
              </div>
            {/each}
          </div>
        {/if}

        <div class="scaffold-cta">
          {#if isLocked(leaf)}
            <span>Enterprise feature — not available on this {edition.toUpperCase()} runtime.</span>
            <a class="btn ghost sm" href={`mailto:support@a3t.io?subject=A3T%20Enterprise%20-%20${encodeURIComponent(m.title)}`}>Contact support</a>
          {:else}
            <span class="saved-note">Changes are saved to the settings store.</span>
          {/if}
        </div>
      </div>
    {/if}
  </div>
</div>

<style>
  .screen-header { padding: 18px 28px 12px; border-bottom: 1px solid var(--border); background: var(--bg-page); flex-shrink: 0; }
  .breadcrumb { font-size: 12px; color: var(--text-4); font-family: var(--font-mono); }
  .breadcrumb .sep { margin: 0 6px; color: var(--text-5); }
  .screen-title-row { margin-top: 6px; display: flex; align-items: center; gap: 14px; }
  .ed-pill { display: inline-flex; align-items: center; gap: 7px; font-size: 12px; color: var(--text-3); background: var(--bg-subtle); border: 1px solid var(--border); padding: 3px 6px 3px 10px; border-radius: 20px; }
  .saved-note { font-family: var(--font-mono); font-size: 11px; color: var(--ok, #23987f); }
  .screen-title { font-family: var(--font-serif); font-size: 24px; font-weight: 500; letter-spacing: -0.02em; }

  .settings-layout { flex: 1; display: flex; overflow: hidden; }

  /* Submenu */
  .set-menu { width: 214px; flex-shrink: 0; padding: 10px 8px; border-right: 1px solid var(--border); background: var(--bg-surface); overflow-y: auto; }
  .s-ico { width: 15px; height: 15px; flex-shrink: 0; color: var(--text-4); }
  .set-group { margin-bottom: 4px; }
  .set-group-head { display: flex; align-items: center; gap: 7px; width: 100%; padding: 6px 8px; border: none; background: none; cursor: pointer; font-size: 11px; font-weight: 650; letter-spacing: 0.03em; text-transform: uppercase; color: var(--text-4); }
  .set-group-head .chev { transition: transform 0.15s; flex-shrink: 0; color: var(--text-5); }
  .set-group-head .chev.open { transform: rotate(90deg); }
  .set-group-head .grp { color: var(--text-3); }
  .set-group-head span { flex: 1; text-align: left; }
  .set-item { display: flex; align-items: center; gap: 8px; width: 100%; padding: 7px 8px 7px 10px; margin-bottom: 1px; border: none; border-radius: var(--r-sm); background: none; color: var(--text-2); font-size: 13px; cursor: pointer; text-align: left; transition: all 0.1s; }
  .set-item.all { margin-bottom: 8px; font-weight: 550; }
  .set-item .s-lbl { flex: 1; padding-left: 12px; }
  .set-item:hover { background: var(--bg-subtle); color: var(--text-1); }
  .set-item:hover .s-ico { color: var(--text-2); }
  .set-item.active { background: var(--bg-subtle); color: var(--text-1); font-weight: 550; box-shadow: inset 2px 0 0 var(--spec, #c47028); }
  .set-item.active .s-ico { color: var(--spec, #c47028); }

  .tag { font-family: var(--font-mono); font-size: 9px; font-weight: 700; padding: 1px 5px; border-radius: 3px; letter-spacing: 0.03em; }
  .tag.sm { font-size: 8px; padding: 0 4px; }
  .tag.ce { color: var(--ok, #23987f); background: var(--ok-bg, #e3f4ef); }
  .tag.ee { color: var(--spec, #c47028); background: var(--spec-bg, #f6ede2); }
  .set-group-head .tag, .set-item .tag { margin-left: auto; }

  /* Body */
  .settings-body { flex: 1; overflow-y: auto; padding: 20px 28px 48px; }

  /* Back link + category intro */
  .back-link { display: inline-flex; align-items: center; gap: 5px; margin-bottom: 14px; padding: 4px 8px 4px 4px; border: none; background: none; color: var(--text-3); font-size: 13px; cursor: pointer; border-radius: var(--r-sm); text-transform: capitalize; }
  .back-link:hover { background: var(--bg-subtle); color: var(--text-1); }
  .cat-intro { display: flex; align-items: center; gap: 11px; margin: 4px 0 18px; }
  .cat-intro .cat-ico { width: 22px; height: 22px; color: var(--spec, #c47028); }
  .cat-intro h2 { font-family: var(--font-serif); font-size: 20px; font-weight: 500; display: flex; align-items: center; gap: 10px; }

  /* Overview */
  .ov-group { margin: 4px 0 26px; }
  .ov-head { display: flex; align-items: center; gap: 9px; margin-bottom: 12px; }
  .ov-head .ov-ico { width: 17px; height: 17px; color: var(--text-3); }
  .ov-head h3 { font-family: var(--font-serif); font-size: 16px; font-weight: 500; }
  .ov-head .tag { margin-left: 2px; }
  .ov-grid { display: grid; grid-template-columns: repeat(auto-fill, minmax(280px, 1fr)); gap: 10px; }
  .ov-card { display: flex; align-items: center; gap: 12px; padding: 13px 15px; border: 1px solid var(--border); border-radius: var(--r); background: var(--bg-surface); cursor: pointer; text-align: left; transition: border-color 0.12s, background 0.12s; }
  .ov-card:hover { border-color: var(--spec); background: var(--bg-subtle); }
  .ov-card-ico { width: 20px; height: 20px; color: var(--text-3); flex-shrink: 0; }
  .ov-card-body { flex: 1; min-width: 0; }
  .ov-card-name { font-size: 14px; font-weight: 550; display: flex; align-items: center; gap: 7px; }
  .ov-card-desc { font-size: 12px; color: var(--text-4); margin-top: 2px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .ov-arrow { width: 14px; height: 14px; color: var(--text-5); flex-shrink: 0; }

  /* Cards */
  .card { background: var(--bg-surface); border: 1px solid var(--border); border-radius: var(--r); padding: 20px 22px; max-width: 720px; }
  .card-head { display: flex; align-items: center; justify-content: space-between; gap: 12px; }
  .card-title { display: flex; align-items: center; gap: 9px; }
  .card-ico { width: 18px; height: 18px; color: var(--spec, #c47028); }
  .card-head h2, .card > h2 { font-family: var(--font-serif); font-size: 18px; font-weight: 500; }
  .card-sub { font-size: 13px; color: var(--text-3); margin: 6px 0 18px; line-height: 1.5; }
  .active-pill { font-size: 11px; color: var(--text-3); background: var(--bg-subtle); border: 1px solid var(--border); padding: 3px 9px; border-radius: 20px; }

  /* Control rows */
  .ctl-list.locked { opacity: 0.92; }
  .ctl-row { display: flex; align-items: center; justify-content: space-between; gap: 16px; padding: 12px 0; border-bottom: 1px solid var(--divider, var(--border)); }
  .ctl-row:last-child { border-bottom: none; }
  .ctl-label { display: flex; flex-direction: column; gap: 2px; }
  .ctl-label strong { font-size: 13px; font-weight: 550; color: var(--text-1); }
  .ctl-hint { font-size: 11px; color: var(--text-4); }
  .ctl-input { padding: 8px 11px; border: 1px solid var(--border); border-radius: var(--r-sm); font-size: 13px; background: var(--bg-surface); color: var(--text-1); min-width: 200px; }
  .ctl-input.sel { min-width: 180px; }
  .ctl-input:disabled { opacity: 0.6; cursor: not-allowed; background: var(--bg-subtle); }

  /* Toggle switch */
  .switch { position: relative; display: inline-block; width: 36px; height: 20px; flex-shrink: 0; }
  .switch input { opacity: 0; width: 0; height: 0; }
  .switch .track { position: absolute; inset: 0; background: var(--border-strong, #c9c9c9); border-radius: 20px; transition: 0.15s; }
  .switch .track::before { content: ''; position: absolute; left: 2px; top: 2px; width: 16px; height: 16px; background: #fff; border-radius: 50%; transition: 0.15s; box-shadow: 0 1px 2px rgba(0,0,0,0.2); }
  .switch input:checked + .track { background: var(--spec, #c47028); }
  .switch input:checked + .track::before { transform: translateX(16px); }
  .switch input:disabled + .track { opacity: 0.55; }

  .row-actions { display: flex; align-items: center; gap: 12px; margin-top: 16px; }
  .msg { font-size: 12px; color: var(--err); }
  .pdb-field { display: flex; flex-direction: column; gap: 4px; font-size: 12px; margin: 10px 0; max-width: 520px; }
  .pdb-check { display: flex; align-items: center; gap: 8px; font-size: 12px; margin: 10px 0; max-width: 520px; }
  .pdb-field select { padding: 6px 8px; border: 1px solid var(--border); border-radius: 6px; background: var(--bg-base, transparent); color: inherit; }
  .pdb-field input { padding: 6px 8px; border: 1px solid var(--border); border-radius: 6px; background: var(--bg-base, transparent); color: inherit; }
  .msg.ok { color: var(--ok, #23987f); }
  .cache-rules { display: flex; flex-direction: column; gap: 6px; margin-top: 12px; max-width: 760px; }
  .cache-rules.buckets { margin-top: 20px; }
  .cache-rule { display: grid; grid-template-columns: 1.1fr 1.4fr 1.4fr 0.9fr 0.6fr; gap: 8px; align-items: center; font-size: 12px; }
  .cache-rule.head { color: var(--text-dim, inherit); opacity: 0.7; font-size: 11px; }
  .cache-rule select, .cache-rule input { padding: 6px 8px; border: 1px solid var(--border); border-radius: 6px; background: var(--bg-base, transparent); color: inherit; min-width: 0; }
  @media (max-width: 640px) { .cache-rule { grid-template-columns: 1fr 1fr; } }
  .mono.dim { font-family: var(--font-mono); font-size: 11px; color: var(--text-4); margin-top: 12px; }

  .rt-list { display: flex; flex-direction: column; gap: 8px; }
  .rt-opt { display: flex; gap: 11px; padding: 12px 14px; border: 1px solid var(--border); border-radius: var(--r-sm); cursor: pointer; transition: border-color 0.12s, background 0.12s; }
  .rt-opt:hover { background: var(--bg-subtle); }
  .rt-opt.sel { border-color: var(--spec); box-shadow: 0 0 0 2px var(--spec-bg); }
  .rt-opt input { margin-top: 3px; }
  .rt-info { min-width: 0; flex: 1; }
  .rt-label { font-size: 14px; font-weight: 550; display: flex; align-items: center; gap: 8px; }
  .rt-note { font-size: 12px; color: var(--text-3); margin-top: 2px; }
  .rt-url { font-size: 11px; color: var(--text-4); margin-top: 4px; }
  .rt-custom { margin-top: 8px; width: 100%; padding: 7px 10px; border: 1px solid var(--border); border-radius: var(--r-sm); font-size: 13px; background: var(--bg-page); color: var(--text-1); }

  .scaffold-cta { display: flex; align-items: center; justify-content: space-between; gap: 12px; padding-top: 16px; margin-top: 8px; border-top: 1px solid var(--border); font-size: 12px; color: var(--text-3); }
  .coming { font-family: var(--font-mono); font-size: 11px; color: var(--text-4); background: var(--bg-subtle); padding: 3px 8px; border-radius: 4px; }

  .btn { padding: 8px 16px; border-radius: var(--r-sm); font-size: 13px; font-weight: 550; cursor: pointer; border: 1px solid var(--border); background: var(--bg-surface); color: var(--text-2); }
  .btn.sm { padding: 5px 12px; font-size: 12px; }
  .btn.ghost:hover { background: var(--bg-subtle); }
  .btn.spec { background: var(--spec, #c47028); color: #fff; border-color: var(--spec, #c47028); }
  .btn.spec:hover { opacity: 0.9; }
  .btn:disabled { opacity: 0.5; cursor: not-allowed; }
</style>
