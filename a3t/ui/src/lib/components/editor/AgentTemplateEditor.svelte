<script lang="ts">
  import { namespacesFor } from '$lib/stores/context.svelte.ts';
  import { API_BASE } from '$lib/config';
  import * as YAML from 'yaml';
  import { mcpCall, getAccessToken } from '$lib/services/api';
  import { discoverServerTools, allToolNames, toolSections, type ServerTools } from '$lib/services/mcpTools';
  import SchemaConfigForm from './SchemaConfigForm.svelte';
  import { getEditorContext, initEditorContext, getFunctionManifest } from '$lib/editor/context';
  import { validateWithParams } from '$lib/editor/schema-utils';
  import { configDefaultsExpr, parseConfigDefaults } from '$lib/editor/yaml-utils';
  import type { FunctionManifest } from '$lib/editor/types';

  let {
    onsection,
    template,
    scrollTo = '',
    onchange,
    onopensub,
    readOnly = false,
    configSeed = null,
  }: {
    onsection?: (section: string) => void;
    template: any;
    scrollTo?: string;
    onchange?: (yaml: string) => void;
    onopensub?: (type: string, data?: any) => void;
    readOnly?: boolean;
    // When the template's `with.config` is a runtime expression (e.g. `${ .config }`,
    // as in the raw agent-engine flow) it carries no literal values, so the form is
    // blank. For an AGENT's designer we seed the form with the agent's own resolved
    // config (LLM, prompt, MCP servers) so it reflects what the agent actually runs.
    configSeed?: Record<string, any> | null;
  } = $props();

  let contentEl: HTMLDivElement;

  $effect(() => {
    if (scrollTo && contentEl) {
      const el = contentEl.querySelector(`#ate-${scrollTo}`);
      if (el) el.scrollIntoView({ behavior: 'smooth', block: 'start' });
    }
  });

  // ── Registry: fetch function manifest for the call type ──
  let manifest = $state<FunctionManifest | null>(null);
  let manifestLoading = $state(true);
  let manifestError = $state('');
  let _lastManifestCall = '';

  // Fetch manifest once per call type (guarded to prevent loops)
  $effect(() => {
    if (!template) return;
    const callType = detectCallType(template);
    if (!callType || callType === _lastManifestCall) return;
    _lastManifestCall = callType;

    const ctx = getEditorContext();
    if (ctx?.ready) {
      manifest = ctx.functions.get(callType) ?? null;
      manifestLoading = false;
      if (!manifest) manifestError = `Function "${callType}" not found in registry`;
    } else {
      initEditorContext().then(c => {
        manifest = c.functions.get(callType) ?? null;
        manifestLoading = false;
        if (!manifest) manifestError = c.error ?? `Function "${callType}" not found in registry`;
      });
    }
  });

  function detectCallType(tmpl: any): string | null {
    for (const task of tmpl?.do ?? []) {
      if (typeof task !== 'object') continue;
      for (const def of Object.values(task) as any[]) {
        if (def?.call) return def.call;
      }
    }
    return null;
  }

  // ── State ──
  let name = $state('');
  let description = $state('');
  let version = $state('1.0.0');
  let dsl = $state('1.0.3');
  let namespace = $state('a3t');

  // Config values — flat Record driven by schema
  let configValues = $state<Record<string, unknown>>({});

  // Top-level with values (session_id, message, etc.)
  let withValues = $state<Record<string, unknown>>({});

  // Extensions from use.extensions
  let extensions = $state<any[]>([]);

  // ── Template manifest (slots) ──
  let templateManifest = $state<any>(null);
  let availableResources = $state<{ name: string; type: string; namespace: string }[]>([]);
  let _lastSlotFetchKey = '';

  // Fetch template manifest when template identity changes
  $effect(() => {
    const key = `${namespace}/${name}`;
    if (!name || !namespace || key === _lastSlotFetchKey) return;
    _lastSlotFetchKey = key;
    const base = API_BASE;
    // /registry/* is JWT-gated — a bare fetch 401s, which left the manifest null
    // and the slot/input pickers empty. (The resources fetch below already worked
    // around the same thing via mcpCall.)
    const token = getAccessToken();
    fetch(`${base}/registry/templates/${encodeURIComponent(namespace)}/${encodeURIComponent(name)}`,
          { headers: token ? { Authorization: `Bearer ${token}` } : {} })
      .then(r => r.ok ? r.json() : null)
      .then(m => { templateManifest = m; })
      .catch(() => {});
  });

  // Fetch resources once on mount
  import { onMount } from 'svelte';
  onMount(async () => {
    // Available resources (LLMs, MCP servers, …) for the pickers — via the AUTHED MCP call.
    // (A raw fetch to /registry/resources is JWT-gated → 401, and returns { resources: [...] }.)
    const seenRes = new Set<string>();
    const resAcc: { name: string; type: string; namespace: string }[] = [];
    for (const ns of namespacesFor('shared')) {
      const res = await mcpCall('list_resources', { namespace: ns });
      if (res.ok && res.data) {
        let raw: any = res.data;
        if (typeof raw === 'string') { try { raw = JSON.parse(raw); } catch { raw = {}; } }
        const list = raw.resources ?? (Array.isArray(raw) ? raw : []);
        for (const r of list) {
          const k = `${r.namespace ?? ns}/${r.name}`;
          if (seenRes.has(k)) continue;
          seenRes.add(k);
          resAcc.push({ name: r.name, type: r.type, namespace: r.namespace ?? ns });
        }
      }
    }
    availableResources = resAcc;
    // Load available flows for extension wiring
    for (const ns of ['system', 'a3t', 'default']) {
      const res = await mcpCall('list_flows', { namespace: ns });
      if (res.ok && res.data) {
        let raw: any = res.data;
        if (typeof raw === 'string') { try { raw = JSON.parse(raw); } catch { raw = {}; } }
        const flows = raw.workflows ?? raw.flows ?? (Array.isArray(raw) ? raw : []);
        for (const f of flows) {
          availableFlows.push({ name: f.name, namespace: f.namespace ?? ns });
        }
      }
    }
    availableFlows = [...availableFlows];
  });

  // Available flows for extension wiring
  let availableFlows = $state<{ name: string; namespace: string }[]>([]);

  // Validation errors
  let validationErrors = $state<{ path: string; message: string }[]>([]);

  function runValidation() {
    if (!manifest?.with_schema) {
      validationErrors = [];
      return;
    }
    try {
      const withBlock = { ...withValues, config: configValues };
      const errs = validateWithParams(withBlock, manifest.with_schema);
      validationErrors = errs.map(e => ({ path: e.path, message: e.message }));
    } catch {
      validationErrors = [];
    }
  }

  // ── Input schema ──
  type SchemaField = { name: string; type: string; description: string; required: boolean; default: string; enum: string };
  let inputFields = $state<SchemaField[]>([]);
  let outputFields = $state<SchemaField[]>([]);
  const SCHEMA_TYPES = ['string', 'number', 'integer', 'boolean', 'object', 'array', 'null'];

  // Coerce a string form value into the JSON type of the field.
  function coerceValue(v: string, type: string): any {
    if (type === 'number' || type === 'integer') { const n = Number(v); return isNaN(n) ? v : (type === 'integer' ? Math.trunc(n) : n); }
    if (type === 'boolean') return v === 'true' || v === '1';
    if (type === 'array' || type === 'object') { try { return JSON.parse(v); } catch { return v; } }
    if (type === 'null') return null;
    return v;
  }

  // Serialize an editable field list back into a JSON Schema object.
  function fieldsToSchema(fields: SchemaField[]): any {
    const props: Record<string, any> = {};
    const req: string[] = [];
    for (const f of fields) {
      if (!f.name) continue;
      const p: any = { type: f.type };
      if (f.description) p.description = f.description;
      if (f.enum && f.enum.trim()) p.enum = f.enum.split(',').map(s => s.trim()).filter(Boolean);
      if (f.default !== '' && f.default != null) p.default = coerceValue(f.default, f.type);
      props[f.name] = p;
      if (f.required) req.push(f.name);
    }
    return { type: 'object', properties: props, ...(req.length ? { required: req } : {}) };
  }

  // Parse a JSON Schema's properties into editable fields.
  function schemaToFields(schema: any): SchemaField[] {
    const props = schema?.properties;
    if (!props || typeof props !== 'object') return [];
    const required = new Set(schema?.required ?? []);
    return Object.entries(props).map(([k, v]: [string, any]) => ({
      name: k,
      type: v?.type ?? 'string',
      description: v?.description ?? '',
      required: required.has(k),
      default: v?.default != null ? (typeof v.default === 'object' ? JSON.stringify(v.default) : String(v.default)) : '',
      enum: Array.isArray(v?.enum) ? v.enum.join(', ') : '',
    }));
  }

  function setSchemaField(which: 'input' | 'output', i: number, patch: Partial<SchemaField>) {
    if (which === 'input') { inputFields[i] = { ...inputFields[i], ...patch }; inputFields = [...inputFields]; }
    else { outputFields[i] = { ...outputFields[i], ...patch }; outputFields = [...outputFields]; }
    emitChange();
  }
  function addSchemaField(which: 'input' | 'output') {
    const f: SchemaField = { name: '', type: 'string', description: '', required: false, default: '', enum: '' };
    if (which === 'input') inputFields = [...inputFields, f]; else outputFields = [...outputFields, f];
    emitChange();
  }
  function removeSchemaField(which: 'input' | 'output', i: number) {
    if (which === 'input') inputFields = inputFields.filter((_, j) => j !== i);
    else outputFields = outputFields.filter((_, j) => j !== i);
    emitChange();
  }

  // ── Triggers ──
  let triggers = $state<any[]>([]);
  let _triggersLoaded = false;

  async function loadTriggers() {
    if (_triggersLoaded) return;
    _triggersLoaded = true;
    const base = API_BASE;
    try {
      const res = await mcpCall('list_triggers', {});
      if (res.ok && res.data) {
        // list_triggers returns { count, triggers: [...] }
        let d: any = res.data;
        if (typeof d === 'string') { try { d = JSON.parse(d); } catch { d = []; } }
        const list = Array.isArray(d) ? d : (d?.triggers ?? []);
        // Filter triggers targeting this flow
        triggers = list.filter((t: any) =>
          t.targets?.some((tgt: any) => tgt.name === name && (tgt.namespace === namespace || !tgt.namespace))
        );
      }
    } catch {}
  }

  async function createQuickTrigger() {
    // The webhook must target the engine's chat WRAPPER (which assembles config), NOT the
    // template's own name — a template isn't a registered workflow, so targeting it gives
    // "Workflow not found". agent-template → SWF wrapper; everything else → native engine.
    const wrapperName = (name === 'agent-template') ? 'a3t-chat-agent' : 'a3t-chat-engine';
    const slug = name.toLowerCase().replace(/[^a-z0-9]+/g, '-').replace(/^-|-$/g, '');
    const triggerName = `${slug}-http`;
    const path = `/${slug}`;
    await mcpCall('create_trigger', {
      name: triggerName, namespace, config: { type: 'webhook', path, method: 'POST' },
      targets: [{ type: 'workflow', namespace: 'a3t', name: wrapperName }], enabled: true,
      description: `HTTP trigger for ${name} → a3t/${wrapperName}`,
    });
    _triggersLoaded = false;
    await loadTriggers();
  }

  // ── Test ──
  let testPayload = $state('');
  let testRunning = $state(false);
  let testResult = $state<any>(null);
  let testError = $state('');
  let showTest = $state(false);

  function getDefaultTestPayload(): string {
    const payload: Record<string, any> = { message: 'Hello from test!' };
    // The template's with.session_id is usually an expression (${ .session_id }) — never
    // put that in the test input; use a concrete session id instead.
    const wsid = withValues.session_id;
    payload.session_id = (typeof wsid === 'string' && !wsid.includes('${') && wsid) ? wsid : `test-${Date.now()}`;
    for (const f of inputFields) {
      if (!f.name || payload[f.name] !== undefined) continue;
      if (f.default !== '' && f.default != null) { payload[f.name] = coerceValue(f.default, f.type); continue; }
      if (f.enum && f.enum.trim()) { payload[f.name] = f.enum.split(',')[0].trim(); continue; }
      if (!f.required) continue;
      payload[f.name] = f.type === 'string' ? 'test'
        : (f.type === 'number' || f.type === 'integer') ? 0
        : f.type === 'boolean' ? false
        : f.type === 'array' ? []
        : f.type === 'object' ? {}
        : null;
    }
    return JSON.stringify(payload, null, 2);
  }

  // Validate a test payload against the input schema before executing.
  function validateTestPayload(payload: any): string | null {
    for (const f of inputFields) {
      if (!f.name) continue;
      const v = payload[f.name];
      if (f.required && (v === undefined || v === null || v === '')) return `Missing required field: ${f.name}`;
      if (v === undefined || v === null) continue;
      const t = f.type;
      const ok = t === 'string' ? typeof v === 'string'
        : (t === 'number' || t === 'integer') ? typeof v === 'number'
        : t === 'boolean' ? typeof v === 'boolean'
        : t === 'array' ? Array.isArray(v)
        : t === 'object' ? (typeof v === 'object' && !Array.isArray(v))
        : true;
      if (!ok) return `Field "${f.name}" should be ${t}`;
      if (f.enum && f.enum.trim()) {
        const opts = f.enum.split(',').map(s => s.trim()).filter(Boolean);
        if (!opts.includes(String(v))) return `Field "${f.name}" must be one of: ${opts.join(', ')}`;
      }
    }
    return null;
  }

  async function runTest() {
    testRunning = true; testResult = null; testError = ''; showTest = true;
    if (!testPayload.trim()) testPayload = getDefaultTestPayload();

    let payload: any;
    try { payload = JSON.parse(testPayload); } catch { testError = 'Invalid JSON'; testRunning = false; return; }

    const schemaErr = validateTestPayload(payload);
    if (schemaErr) { testError = schemaErr; testRunning = false; return; }

    try {
      let instanceId: string | null = null;

      {
        // A template is a definition, NOT a registered runnable workflow, and its own
        // trigger/endpoint targets the template NAME as a workflow — which isn't
        // registered → "Workflow not found". So we IGNORE triggers for the Test and
        // always run through the engine's chat WRAPPER, which assembles a valid `config`
        // from the flat body. agent-template → the SWF wrapper; everything else → native.
        const tns = 'a3t';
        const tname = (name === 'agent-template') ? 'a3t-chat-agent' : 'a3t-chat-engine';
        // Carry the template's own config into the flat body so the test reflects it.
        const body: any = { ...payload };
        const llm = cfgGet('llm_resource');
        if (llm && body.llm_resource == null) body.llm_resource = String(llm).includes('/') ? llm : `a3t/${llm}`;
        const sp = cfgGet('system_prompt'); if (sp && body.system_prompt == null) body.system_prompt = sp;
        const mcps = cfgGetArr('mcp_servers'); if (mcps.length && body.mcp_servers == null) body.mcp_servers = mcps;
        if (permRestricted() && body.enabled_tools == null) body.enabled_tools = cfgGetArr('enabled_tools');
        const res = await mcpCall('execute_flow', { name: tname, namespace: tns, input: body });
        if (!res.ok) { testError = res.error ?? 'Execute failed'; testRunning = false; return; }
        const text = typeof res.data === 'string' ? res.data : JSON.stringify(res.data ?? '');
        const match = text.match(/Instance ID:\s*`?([0-9a-f-]+)`?/);
        instanceId = match?.[1] ?? null;
        testResult = { phase: 'running', instanceId, via: tname !== name ? `wrapper ${tname}` : 'direct' };
      }

      if (!instanceId) { testError = 'No instance ID'; testRunning = false; return; }

      // Poll
      for (let i = 0; i < 30; i++) {
        await new Promise(r => setTimeout(r, 1000));
        const s = await mcpCall('get_execution_status', { instance_id: instanceId });
        if (!s.ok) continue;
        let d: any = s.data;
        if (typeof d === 'string') {
          try { d = JSON.parse(d); }
          catch {
            // Non-JSON status text (e.g. "Execution <id> failed: …" / "not found").
            const txt = s.data as string;
            if (/fail|error|not found|invalid|missing/i.test(txt)) { testResult = { ...testResult, phase: 'failed' }; testError = txt; testRunning = false; return; }
            testResult = { ...testResult, poll: i + 1 }; continue;
          }
        }
        if (d.status === 'completed' || d.status === 'Completed') {
          testResult = { ...testResult, phase: 'completed', output: d.output }; testRunning = false; return;
        }
        if (d.status === 'failed' || d.status === 'Failed') {
          testResult = { ...testResult, phase: 'failed' }; testError = d.error ?? 'Failed'; testRunning = false; return;
        }
        testResult = { ...testResult, poll: i + 1 };
      }
      testError = 'Timed out (30s)'; testRunning = false;
    } catch (e) { testError = String(e); testRunning = false; }
  }

  // Tab state
  let activeTab = $state<'agent' | 'llm' | 'tools' | 'permissions' | 'memory' | 'io' | 'test'>('agent');
  // Tell the host which section is on screen. The form is how a user discovers
  // what is configurable, so "tighten this" only resolves if the agent can see
  // the same section the user is reading.
  $effect(() => { onsection?.(activeTab); });

  // Derived config helpers for specific tabs
  // What the agent engine does when a tier is left unset (FlowR agent_memory.rs, F3).
  const MEMORY_TIER_DEFAULTS: Record<string, { read: boolean; write: boolean; limit?: number }> = {
    working: { read: true, write: true, limit: 20 },
    episodic: { read: true, write: true, limit: 20 },
    semantic: { read: true, write: false, limit: 5 },
    procedural: { read: false, write: false },
  };
  // Set one setting of a memory tier; an empty value removes it (the tier default).
  function setTier(tier: string, key: string, val: any) {
    const obj = { ...cfgGetObj('memory_config') };
    const t = { ...(obj[tier] ?? {}) };
    if (val === undefined || val === '' || Number.isNaN(val)) delete t[key]; else t[key] = val;
    obj[tier] = t;
    cfgSet('memory_config', obj);
  }
  function cfgGet(key: string): any { return configValues[key]; }
  function cfgSet(key: string, val: any) { if (val === undefined || val === '') delete configValues[key]; else configValues[key] = val; configValues = { ...configValues }; emitChange(); }
  function cfgGetArr(key: string): any[] { return (configValues[key] as any[]) ?? []; }
  function cfgSetArr(key: string, val: any[]) { configValues[key] = val; configValues = { ...configValues }; emitChange(); }
  function cfgGetObj(key: string): Record<string, any> { return (configValues[key] as Record<string, any>) ?? {}; }

  // Capability ceiling (Permissions tab). The tool list is DISCOVERED from the
  // template's selected MCP servers (list_mcp_tools), not a static catalog.
  // `enabled_tools` absent = unrestricted; a list (even an empty one) = the only tools
  // an agent may use. That is how the runtime reads it: an empty list allows nothing.
  let serverTools = $state<ServerTools[]>([]);
  let availableTools = $derived(allToolNames(serverTools));
  let toolsLoading = $state(false);
  let _lastToolsKey = '__init__';
  $effect(() => {
    const servers = cfgGetArr('mcp_servers').map(String);
    const key = servers.join('|');
    if (key === _lastToolsKey) return;
    _lastToolsKey = key;
    if (servers.length === 0) { serverTools = []; toolsLoading = false; return; }
    toolsLoading = true;
    discoverServerTools(servers, namespace || 'a3t').then(found => {
      if (_lastToolsKey !== key) return;   // the selection changed meanwhile
      serverTools = found;
      // Reconcile: drop any allowed tool that the current servers no longer expose.
      const set = new Set(allToolNames(found));
      const cur = cfgGetArr('enabled_tools');
      if (permRestricted()) {
        const kept = cur.filter((t: string) => set.has(t));
        if (kept.length !== cur.length) cfgSetArr('enabled_tools', kept);
      }
      toolsLoading = false;
    });
  });
  const isDestructive = (t: string) => /delete|remove|drop|purge|destroy|clear|wipe/i.test(t);
  function permRestricted() { return Array.isArray(cfgGet('enabled_tools')); }
  // A tool is allowed when there is no list (unrestricted) or it is on the list.
  function permAllowed(t: string) { return !permRestricted() || cfgGetArr('enabled_tools').includes(t); }
  function permToggle(t: string) {
    const s = new Set(permRestricted() ? cfgGetArr('enabled_tools') : availableTools);   // "all" before removing one
    s.has(t) ? s.delete(t) : s.add(t);
    const arr = [...s];
    // Everything allowed again → drop the list (unrestricted) rather than store it all.
    if (availableTools.every(x => s.has(x))) cfgSet('enabled_tools', undefined);
    else cfgSetArr('enabled_tools', arr);
  }

  // LLM picker: current value + the LLM-typed options. Kept here (not inline
  // {@const}) so they can live directly inside the .res-picker element.
  let curLlm = $derived(String(cfgGet('llm_resource') ?? ''));
  let llmOpts = $derived(availableResources.filter(r => r.type === 'openai' || r.type === 'anthropic' || r.type === 'groq' || r.type === 'ollama' || r.type.includes('llm')));

  // Derived lists for tool pickers
  let usedMcps = $derived(new Set(cfgGetArr('mcp_servers')));
  let freeMcps = $derived(availableResources.filter(r => r.type === 'mcp' && !usedMcps.has(r.name)));
  let usedFlowKeys = $derived(new Set(Object.values(cfgGetObj('tool_flows')).map((r: any) => `${r.namespace}/${r.name}`)));
  let freeFlows = $derived(availableFlows.filter(f => !usedFlowKeys.has(`${f.namespace}/${f.name}`)));

  // Search combos for tools tab
  let mcpSearch = $state('');
  let mcpOpen = $state(false);
  let flowSearch = $state('');
  let flowOpen = $state(false);
  let filteredMcps = $derived(freeMcps.filter(r => !mcpSearch || r.name.toLowerCase().includes(mcpSearch.toLowerCase())));
  let filteredToolFlows = $derived(freeFlows.filter(f => !flowSearch || f.name.toLowerCase().includes(flowSearch.toLowerCase()) || f.namespace.toLowerCase().includes(flowSearch.toLowerCase())));

  // Section open state within tabs
  let open = $state<Record<string, boolean>>({ identity: true, config: true, extensions: false, slots: false, input: true, output: false, triggers: true });

  // ── Init from CNCF YAML ──
  let lastInitId = '';
  let callTaskName = '';

  $effect(() => {
    if (!template) return;
    const doc = template.document ?? {};
    const id = doc.name ?? template.name ?? '';
    // Re-init when the template OR the seeded agent config changes, so two agents
    // sharing one template (e.g. agent-engine) each show their own values.
    const initKey = id + '|' + (configSeed ? JSON.stringify(configSeed) : '');
    if (initKey === lastInitId) return;
    lastInitId = initKey;

    // Identity
    name = doc.name ?? '';
    version = doc.version ?? '1.0.0';
    namespace = doc.namespace ?? 'a3t';
    dsl = doc.dsl ?? '1.0.3';
    description = doc.metadata?.description ?? '';

    // Find the call task and extract with values
    for (const task of template.do ?? []) {
      if (typeof task !== 'object') continue;
      for (const [taskName, def] of Object.entries(task) as [string, any][]) {
        if (def?.call) {
          callTaskName = taskName;
          const w = def.with ?? {};
          // Separate config from other with params
          const { config: cfg, ...rest } = w;
          // cfg may be an expression string like "${ .config }" — no literal values
          // to show. Fall back to the agent's seeded config so the form reflects the
          // agent (not a blank template).
          configValues = (cfg && typeof cfg === 'object') ? cfg : (parseConfigDefaults(cfg) ?? { ...(configSeed ?? {}) });
          withValues = rest;
          break;
        }
      }
    }

    // Extensions
    extensions = (template.use?.extensions ?? []).map((ext: any, i: number) => {
      if (typeof ext !== 'object') return { name: `ext-${i}`, extend: 'all', raw: ext };
      const extName = Object.keys(ext)[0] ?? `ext-${i}`;
      const def = ext[extName] ?? {};
      return { name: extName, ...def };
    });

    // Input & output schemas
    inputFields = schemaToFields(template.input?.schema);
    outputFields = schemaToFields(template.output?.schema);

    // Load triggers for this template
    loadTriggers();

    // Set default test payload
    testPayload = getDefaultTestPayload();
  });

  // ── Schema for the "with" block ──
  // The with_schema has top-level properties like "message", "session_id", "config"
  // We split: config gets its own section, the rest are "with" params
  let withSchema = $derived.by(() => {
    if (!manifest?.with_schema) return null;
    const schema = manifest.with_schema as Record<string, unknown>;
    const props = { ...(schema.properties ?? {}) } as Record<string, unknown>;
    // Remove config — it gets its own section
    delete props.config;
    return { ...schema, properties: props };
  });

  let configSchema = $derived.by(() => {
    if (!manifest?.with_schema) return null;
    const schema = manifest.with_schema as Record<string, unknown>;
    const props = (schema.properties ?? {}) as Record<string, Record<string, unknown>>;
    return props.config ?? null;
  });

  // ── Build CNCF YAML from form state ──
  function buildCncfYaml(): string {
    const doc: any = {
      document: { dsl, namespace, name, version },
    };
    if (description) {
      doc.document.metadata = { description };
    }

    // Input & output schemas
    if (inputFields.some(f => f.name)) doc.input = { schema: fieldsToSchema(inputFields) };
    if (outputFields.some(f => f.name)) doc.output = { schema: fieldsToSchema(outputFields) };

    // Extensions
    if (extensions.length > 0) {
      doc.use = {
        extensions: extensions.map(e => {
          const { name: extName, ...rest } = e;
          return { [extName]: rest };
        }),
      };
    }

    // Build the call task with values
    const callType = detectCallType(template) ?? 'a3t:agent';
    // The template's values are DEFAULTS merged under each caller's config (see
    // configDefaultsExpr): a literal config here overrode every agent's own LLM,
    // MCP servers and tool ceiling.
    const templateDefaults: Record<string, unknown> = Object.keys(configValues).length > 0 ? configValues : {
        system_prompt: 'You are a helpful assistant.',
        max_iterations: 10,
        max_tokens: 4096,
        temperature: 0.7,
        tools: [],
        tool_flows: {},
        mcp_servers: [],
        memory_namespace: name || 'default',
        ttl: 1800,
      };
    const withBlock: any = {
      config: configDefaultsExpr(templateDefaults),
      session_id: '${ .session_id // "default" }',
      message: '${ .message }',
      ui_context: '${ .ui_context }',
    };

    doc.do = [{
      [callTaskName || 'run_agent']: {
        call: callType,
        with: withBlock,
      },
    }];

    return YAML.stringify(doc, { indent: 2, lineWidth: 120 });
  }

  // ── Handle schema form changes ──
  function onConfigChange(path: string[], val: unknown) {
    // path is like ["config", "system_prompt"] or deeper
    // We need to set the value at the right depth in configValues
    const keys = path.slice(path.indexOf('config') + 1);
    if (keys.length === 0) return;

    let obj = configValues;
    for (let i = 0; i < keys.length - 1; i++) {
      if (obj[keys[i]] === undefined || typeof obj[keys[i]] !== 'object') {
        obj[keys[i]] = {};
      }
      obj = obj[keys[i]] as Record<string, unknown>;
    }
    const lastKey = keys[keys.length - 1];
    if (val === undefined || val === '') {
      delete obj[lastKey];
    } else {
      obj[lastKey] = val;
    }
    configValues = { ...configValues };
    emitChange();
  }

  function onWithChange(path: string[], val: unknown) {
    // path starts after "with" context
    const key = path[path.length - 1];
    if (!key) return;
    if (val === undefined || val === '') {
      delete withValues[key];
    } else {
      withValues[key] = val;
    }
    withValues = { ...withValues };
    emitChange();
  }

  // ── Emit changes (debounced) ──
  let emitTimer: ReturnType<typeof setTimeout> | null = null;
  function emitChange() {
    if (emitTimer) clearTimeout(emitTimer);
    emitTimer = setTimeout(() => {
      onchange?.(buildCncfYaml());
      runValidation();
    }, 300);
  }

  function toggle(k: string) { (open as any)[k] = !(open as any)[k]; }

  // Agent lifecycle phases — the only extension names that matter
  const agentPhases = [
    { name: 'on_turn_start',  desc: 'Beginning of each turn' },
    { name: 'before_recall',  desc: 'Before memory recall' },
    { name: 'after_recall',   desc: 'After memory recall' },
    { name: 'before_llm',     desc: 'Before LLM call (RAG, prompt injection)' },
    { name: 'on_llm_response', desc: 'After LLM responds' },
    { name: 'on_tool_call',   desc: 'Before tool dispatch' },
    { name: 'on_tool_error',  desc: 'When a tool fails' },
    { name: 'before_store',   desc: 'Before memory save' },
    { name: 'after_response', desc: 'After response emitted' },
    { name: 'on_turn_end',    desc: 'End of turn cleanup' },
  ];
  const usedPhases = $derived(new Set(extensions.map((e: any) => e.name)));

  function addExtension() {
    const next = agentPhases.find(p => !usedPhases.has(p.name));
    if (!next) return; // all phases used
    extensions = [...extensions, { name: next.name, extend: 'all', before: [], after: [] }];
    emitChange();
  }

  function addExtFlow(extIdx: number, phase: 'before' | 'after') {
    const ext = extensions[extIdx];
    const arr = [...(ext[phase] ?? [])];
    arr.push({ [`${phase}_wf_${arr.length + 1}`]: { call: 'run:workflow', with: { namespace: 'a3t', name: '' } } });
    extensions[extIdx] = { ...ext, [phase]: arr };
    extensions = [...extensions];
    emitChange();
  }

  function setExtFlowRef(extIdx: number, phase: 'before' | 'after', taskIdx: number, ns: string, flowName: string) {
    const ext = extensions[extIdx];
    const arr = [...(ext[phase] ?? [])];
    const taskKey = Object.keys(arr[taskIdx])[0];
    arr[taskIdx] = { [taskKey]: { call: 'run:workflow', with: { namespace: ns, name: flowName } } };
    extensions[extIdx] = { ...ext, [phase]: arr };
    extensions = [...extensions];
    emitChange();
  }

  function removeExtFlow(extIdx: number, phase: 'before' | 'after', taskIdx: number) {
    const ext = extensions[extIdx];
    const arr = (ext[phase] ?? []).filter((_: any, j: number) => j !== taskIdx);
    extensions[extIdx] = { ...ext, [phase]: arr };
    extensions = [...extensions];
    emitChange();
  }

  function getExtFlowRef(task: any): { namespace: string; name: string } {
    const def = Object.values(task)[0] as any;
    return { namespace: def?.with?.namespace ?? '', name: def?.with?.name ?? '' };
  }

  function removeExtension(idx: number) {
    extensions = extensions.filter((_, i) => i !== idx);
    emitChange();
  }

  function updateExtension(idx: number, field: string, val: unknown) {
    extensions[idx] = { ...extensions[idx], [field]: val };
    extensions = [...extensions];
    emitChange();
  }
</script>

<div class="ate" class:ate-readonly={readOnly} bind:this={contentEl}>

  <!-- Tab bar -->
  <div class="ate-tabs">
    <button class="ate-tab" class:active={activeTab === 'agent'} onclick={() => activeTab = 'agent'}>Agent</button>
    <button class="ate-tab" class:active={activeTab === 'llm'} onclick={() => activeTab = 'llm'}>LLM</button>
    <button class="ate-tab" class:active={activeTab === 'tools'} onclick={() => activeTab = 'tools'}>Tools</button>
    <button class="ate-tab" class:active={activeTab === 'permissions'} onclick={() => activeTab = 'permissions'}>Permissions</button>
    <button class="ate-tab" class:active={activeTab === 'memory'} onclick={() => activeTab = 'memory'}>Memory</button>
    <button class="ate-tab" class:active={activeTab === 'io'} onclick={() => activeTab = 'io'}>I/O</button>
    <button class="ate-tab" class:active={activeTab === 'test'} onclick={() => activeTab = 'test'}>
      Test
      {#if testResult?.phase === 'completed'}<span class="tab-dot ok"></span>{/if}
      {#if testResult?.phase === 'failed'}<span class="tab-dot err"></span>{/if}
    </button>
  </div>

  {#if activeTab === 'agent'}
  <!-- ═══ AGENT TAB ═══ -->
  <div class="tab-body">
    <div class="sub-label">IDENTITY</div>
    <div class="row2">
      <label>Name <input type="text" bind:value={name} oninput={emitChange} /></label>
      <label>Version <input type="text" bind:value={version} oninput={emitChange} /></label>
    </div>
    <div class="row2">
      <label>Namespace <input type="text" bind:value={namespace} oninput={emitChange} /></label>
      <label>DSL <input type="text" value={dsl} readonly /></label>
    </div>
    <label class="full">Description <input type="text" bind:value={description} oninput={emitChange} /></label>

    <div class="sub-label">LIFECYCLE HOOKS</div>
    {#each agentPhases as phase}
      {@const ext = extensions.find(e => e.name === phase.name)}
      {@const extIdx = extensions.findIndex(e => e.name === phase.name)}
      {@const hasBefore = (ext?.before ?? []).length > 0}
      {@const hasAfter = (ext?.after ?? []).length > 0}
      <div class="ext-card" class:ext-empty={!hasBefore && !hasAfter}>
        <div class="ext-top">
          <span class="ext-phase-name">{phase.name.replace(/_/g, ' ')}</span>
          <span class="ext-desc-inline">{phase.desc}</span>
        </div>
        <div class="ext-flows">
          <div class="ext-phase">
            <span class="ext-phase-label">before</span>
            {#if ext}
              {#each (ext.before ?? []) as task, j}
                {@const ref = getExtFlowRef(task)}
                <div class="ext-flow-row">
                  <select value={`${ref.namespace}/${ref.name}`}
                    onchange={(e) => { const [ns, ...rest] = e.currentTarget.value.split('/'); setExtFlowRef(extIdx, 'before', j, ns, rest.join('/')); }}>
                    <option value="/">-- select --</option>
                    {#each availableFlows as f}<option value="{f.namespace}/{f.name}">{f.namespace}/{f.name}</option>{/each}
                  </select>
                  <button class="rm" onclick={() => removeExtFlow(extIdx, 'before', j)}>&times;</button>
                </div>
              {/each}
            {/if}
            <button class="add-sm" onclick={() => { if (!ext) { extensions = [...extensions, { name: phase.name, extend: 'all', before: [], after: [] }]; emitChange(); } setTimeout(() => addExtFlow(extensions.findIndex(e => e.name === phase.name), 'before'), 0); }}>+ flow</button>
          </div>
          <div class="ext-phase">
            <span class="ext-phase-label">after</span>
            {#if ext}
              {#each (ext.after ?? []) as task, j}
                {@const ref = getExtFlowRef(task)}
                <div class="ext-flow-row">
                  <select value={`${ref.namespace}/${ref.name}`}
                    onchange={(e) => { const [ns, ...rest] = e.currentTarget.value.split('/'); setExtFlowRef(extIdx, 'after', j, ns, rest.join('/')); }}>
                    <option value="/">-- select --</option>
                    {#each availableFlows as f}<option value="{f.namespace}/{f.name}">{f.namespace}/{f.name}</option>{/each}
                  </select>
                  <button class="rm" onclick={() => removeExtFlow(extIdx, 'after', j)}>&times;</button>
                </div>
              {/each}
            {/if}
            <button class="add-sm" onclick={() => { if (!ext) { extensions = [...extensions, { name: phase.name, extend: 'all', before: [], after: [] }]; emitChange(); } setTimeout(() => addExtFlow(extensions.findIndex(e => e.name === phase.name), 'after'), 0); }}>+ flow</button>
          </div>
        </div>
      </div>
    {/each}
  </div>

  {:else if activeTab === 'llm'}
  <!-- ═══ LLM TAB ═══ -->
  <div class="tab-body">
    <div class="sub-label">SYSTEM PROMPT</div>
    <textarea rows="6" value={String(cfgGet('system_prompt') ?? '')}
      oninput={(e) => cfgSet('system_prompt', e.currentTarget.value)}
      placeholder="Agent instructions..."></textarea>

    <div class="sub-label">LLM RESOURCE</div>
    <div class="res-picker">
      <select value={curLlm}
        onchange={(e) => cfgSet('llm_resource', e.currentTarget.value)}>
        <option value="">-- select LLM resource --</option>
        {#if curLlm && !llmOpts.some(r => r.name === curLlm)}
          <option value={curLlm}>{curLlm}</option>
        {/if}
        {#each llmOpts as r}
          <option value={r.name}>{r.name} ({r.type})</option>
        {/each}
      </select>
      <button class="new-res-btn" onclick={() => onopensub?.('resource-new', { type: 'openai', label: 'LLM' })}>+ New</button>
    </div>
    {#if cfgGet('llm_resource')}
      {@const res = availableResources.find(r => r.name === cfgGet('llm_resource'))}
      {#if res}
        <div class="res-detail">
          <div class="res-detail-row"><span>Type</span><span>{res.type}</span></div>
          <div class="res-detail-row"><span>Namespace</span><span>{res.namespace}</span></div>
        </div>
      {/if}
    {/if}

    <div class="sub-label">REASONING & GENERATION</div>
    <div class="row3">
      <label>Max iterations <input type="number" value={cfgGet('max_iterations') ?? 10}
        oninput={(e) => cfgSet('max_iterations', Number(e.currentTarget.value) || 10)} /></label>
      <label>Max tokens <input type="number" value={cfgGet('max_tokens') ?? 4096}
        oninput={(e) => cfgSet('max_tokens', Number(e.currentTarget.value) || 4096)} /></label>
      <label>Temperature <input type="number" step="0.1" min="0" max="2" value={cfgGet('temperature') ?? 0.7}
        oninput={(e) => cfgSet('temperature', Number(e.currentTarget.value))} /></label>
    </div>

  </div>

  {:else if activeTab === 'tools'}
  <!-- ═══ TOOLS TAB ═══ -->
  <div class="tab-body">
    <div class="sub-label">MCP SERVERS <span class="cnt">{cfgGetArr('mcp_servers').length}</span></div>
    {#each cfgGetArr('mcp_servers') as server, i}
      <div class="tool-item">
        <span class="tool-item-icon mcp">M</span>
        <span class="tool-item-name">{server || '(empty)'}</span>
        <button class="rm" onclick={() => cfgSetArr('mcp_servers', cfgGetArr('mcp_servers').filter((_: any, j: number) => j !== i))}>&times;</button>
      </div>
    {/each}
    <!-- MCP search-select -->
    <div class="combo">
      <div class="combo-input">
        <svg width="12" height="12" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5"><circle cx="7" cy="7" r="4"/><path d="M10 10l3.5 3.5"/></svg>
        <input type="text" bind:value={mcpSearch} placeholder="Search MCP resources..."
          onfocus={() => mcpOpen = true}
          onblur={() => setTimeout(() => mcpOpen = false, 150)} />
      </div>
      {#if mcpOpen && filteredMcps.length > 0}
        <div class="combo-list">
          {#each filteredMcps as r}
            <button class="combo-item" onmousedown={(e) => { e.preventDefault(); cfgSetArr('mcp_servers', [...cfgGetArr('mcp_servers'), r.name]); mcpSearch = ''; mcpOpen = false; }}>
              <span class="tool-item-icon mcp">M</span>
              <span>{r.name}</span>
            </button>
          {/each}
        </div>
      {/if}
    </div>
    <button class="new-res-btn" onclick={() => onopensub?.('resource-new', { type: 'mcp', label: 'MCP Server' })}>+ New MCP Resource</button>

    <div class="sub-label">TOOL FLOWS <span class="cnt">{Object.keys(cfgGetObj('tool_flows')).length}</span></div>
    {#each Object.entries(cfgGetObj('tool_flows')) as [toolName, ref]}
      <div class="tool-item">
        <span class="tool-item-icon flow">F</span>
        <input type="text" value={toolName} placeholder="tool name" class="tool-name-input"
          oninput={(e) => {
            const obj = { ...cfgGetObj('tool_flows') };
            const val = obj[toolName];
            delete obj[toolName];
            obj[e.currentTarget.value] = val;
            cfgSet('tool_flows', obj);
          }} />
        <span class="tool-item-ref">{(ref as any)?.namespace}/{(ref as any)?.name}</span>
        <button class="rm" onclick={() => { const obj = { ...cfgGetObj('tool_flows') }; delete obj[toolName]; cfgSet('tool_flows', obj); }}>&times;</button>
      </div>
    {/each}
    <!-- Flow search-select -->
    <div class="combo">
      <div class="combo-input">
        <svg width="12" height="12" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5"><circle cx="7" cy="7" r="4"/><path d="M10 10l3.5 3.5"/></svg>
        <input type="text" bind:value={flowSearch} placeholder="Search flows..."
          onfocus={() => flowOpen = true}
          onblur={() => setTimeout(() => flowOpen = false, 150)} />
      </div>
      {#if flowOpen && filteredToolFlows.length > 0}
        <div class="combo-list">
          {#each filteredToolFlows as f}
            <button class="combo-item" onmousedown={(e) => {
              e.preventDefault();
              const obj = { ...cfgGetObj('tool_flows') };
              obj[f.name] = { namespace: f.namespace, name: f.name };
              cfgSet('tool_flows', obj);
              flowSearch = ''; flowOpen = false;
            }}>
              <span class="tool-item-icon flow">F</span>
              <span>{f.namespace}/{f.name}</span>
            </button>
          {/each}
        </div>
      {/if}
    </div>
  </div>

  {:else if activeTab === 'permissions'}
  <!-- ═══ PERMISSIONS TAB ═══ -->
  <div class="tab-body">
    <div class="sub-label">CAPABILITY CEILING</div>
    {#if cfgGetArr('mcp_servers').length === 0}
      <p class="perm-hint">This template has no tools to govern yet. Add one or more <strong>MCP servers</strong> on the <button type="button" class="linkish" onclick={() => activeTab = 'tools'}>Tools</button> tab first — the ceiling controls which of <em>their</em> tools an agent may use.</p>
    {:else if toolsLoading}
      <p class="perm-hint">Discovering tools from the selected MCP servers…</p>
    {:else}
      {#if availableTools.length > 0}
        <p class="perm-hint">Uncheck a tool to block it for agents from this template. All checked = unrestricted; the policy engine denies anything unchecked even if the model tries it.</p>
        <div class="perm-summary">
          {#if !permRestricted()}
            <span class="perm-badge ok">All {availableTools.length} allowed</span>
          {:else}
            <span class="perm-badge warn">{cfgGetArr('enabled_tools').length} of {availableTools.length} allowed</span>
            <button class="perm-all" onclick={() => cfgSet('enabled_tools', undefined)}>Allow all</button>
          {/if}
        </div>
      {/if}
      {#each serverTools as srv (srv.server)}
        {#if srv.tools.length === 0}
          <div class="sub-label">{srv.name}</div>
          <p class="perm-hint perm-err">No tools listed: {srv.error}</p>
        {/if}
        {#each toolSections(srv) as g (g.group)}
          <div class="sub-label">{g.group} <span class="perm-count">{g.tools.length}</span></div>
          <div class="perm-tools">
            {#each g.tools as t (t)}
              <label class="perm-chk" class:on={permAllowed(t)} class:danger={isDestructive(t)}>
                <input type="checkbox" checked={permAllowed(t)} onchange={() => permToggle(t)} /><span>{t}</span>
              </label>
            {/each}
          </div>
        {/each}
      {/each}
    {/if}
  </div>

  {:else if activeTab === 'memory'}
  <!-- ═══ MEMORY TAB ═══ -->
  <div class="tab-body">
    <div class="sub-label">MEMORY RESOURCE</div>
    <div class="res-picker">
      <select value={String(cfgGet('memory_resource') ?? '')}
        onchange={(e) => cfgSet('memory_resource', e.currentTarget.value || undefined)}>
        <option value="">-- none --</option>
        {#each availableResources.filter(r => r.type === 'adb' || r.type === 'clawdb') as r}
          <option value={r.name}>{r.name}</option>
        {/each}
      </select>
      <button class="new-res-btn" onclick={() => onopensub?.('resource-new', { type: 'adb', label: 'Memory DB' })}>+ New</button>
    </div>

    <div class="sub-label">NAMESPACE & SESSION</div>
    <div class="row2">
      <label>Memory namespace <input type="text" value={String(cfgGet('memory_namespace') ?? '')}
        oninput={(e) => cfgSet('memory_namespace', e.currentTarget.value)} placeholder="default" /></label>
      <label>Session TTL (seconds) <input type="number" value={cfgGet('ttl') ?? 1800}
        oninput={(e) => cfgSet('ttl', Number(e.currentTarget.value) || 1800)} /></label>
    </div>

    <div class="sub-label">MEMORY TIERS</div>
    {#each ['working', 'episodic', 'semantic', 'procedural'] as tier}
      {@const t = (cfgGetObj('memory_config')[tier] ?? {}) as Record<string, any>}
      {@const d = MEMORY_TIER_DEFAULTS[tier]}
      <div class="mem-tier">
        <span class="mem-tier-name">{tier}</span>
        <label class="mem-chk"><input type="checkbox" checked={t.read ?? d.read}
          onchange={(e) => { const obj = { ...cfgGetObj('memory_config') }; obj[tier] = { ...(obj[tier] ?? {}), read: e.currentTarget.checked }; cfgSet('memory_config', obj); }} /> read</label>
        <label class="mem-chk"><input type="checkbox" checked={t.write ?? d.write}
          onchange={(e) => { const obj = { ...cfgGetObj('memory_config') }; obj[tier] = { ...(obj[tier] ?? {}), write: e.currentTarget.checked }; cfgSet('memory_config', obj); }} /> write</label>
        {#if d.limit !== undefined}
          <label class="mem-num" title="How many records a read brings back (AQL LIMIT)">limit
            <input type="number" min="1" max="200" value={t.limit ?? ''} placeholder={String(d.limit)}
              oninput={(e) => setTier(tier, 'limit', e.currentTarget.value === '' ? '' : Number(e.currentTarget.value))} /></label>
        {/if}
        {#if tier === 'semantic'}
          <label class="mem-num" title="The least similarity (0–1) a fact needs to reach the prompt (AQL MIN_CONFIDENCE); empty = no threshold">min conf
            <input type="number" min="0" max="1" step="0.05" value={t.min_confidence ?? ''} placeholder="—"
              oninput={(e) => setTier(tier, 'min_confidence', e.currentTarget.value === '' ? '' : Number(e.currentTarget.value))} /></label>
        {/if}
      </div>
    {/each}

    <div class="mem-hint">
      <strong>Working</strong> — this session's user turns<br/>
      <strong>Episodic</strong> — this session's replies<br/>
      <strong>Semantic</strong> — facts (saved with the <code>remember</code> tool); read puts the ones most relevant to the message into the prompt (up to <em>limit</em>, at least <em>min conf</em> similar)<br/>
      <strong>Procedural</strong> — when read is on, a procedure matching the request answers without the LLM
    </div>
  </div>

  {:else if activeTab === 'io'}
  <!-- ═══ I/O TAB ═══ -->

  <!-- Input Schema -->
  <div class="ate-sec" id="ate-input">
    <button class="ate-head" onclick={() => toggle('input')}>
      <svg class="chev" class:open={open.input} width="10" height="10" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="2"><path d="M6 4l4 4-4 4"/></svg>
      <span class="ico inp">In</span><h4>Input Schema</h4><span class="cnt">{inputFields.length}</span>
    </button>
    {#if open.input}
      <div class="body">
        {#each inputFields as f, i}
          <div class="sf-row">
            <input type="text" value={f.name} placeholder="field name" class="sf-name"
              oninput={(e) => setSchemaField('input', i, { name: e.currentTarget.value })} />
            <select value={f.type} onchange={(e) => setSchemaField('input', i, { type: e.currentTarget.value })}>
              {#each SCHEMA_TYPES as t}<option>{t}</option>{/each}
            </select>
            <label class="inp-req"><input type="checkbox" checked={f.required}
              onchange={(e) => setSchemaField('input', i, { required: e.currentTarget.checked })} /> req</label>
            <button class="rm" onclick={() => removeSchemaField('input', i)}>&times;</button>
            <input type="text" value={f.description} placeholder="description" class="sf-desc"
              oninput={(e) => setSchemaField('input', i, { description: e.currentTarget.value })} />
            <input type="text" value={f.default} placeholder="default" class="sf-def"
              oninput={(e) => setSchemaField('input', i, { default: e.currentTarget.value })} />
            <input type="text" value={f.enum} placeholder="enum (a, b, c)" class="sf-enum"
              oninput={(e) => setSchemaField('input', i, { enum: e.currentTarget.value })} />
          </div>
        {/each}
        <button class="add" onclick={() => addSchemaField('input')}>+ Add Field</button>
      </div>
    {/if}
  </div>

  <!-- Output Schema -->
  <div class="ate-sec" id="ate-output">
    <button class="ate-head" onclick={() => toggle('output')}>
      <svg class="chev" class:open={open.output} width="10" height="10" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="2"><path d="M6 4l4 4-4 4"/></svg>
      <span class="ico out">Out</span><h4>Output Schema</h4><span class="cnt">{outputFields.length}</span>
    </button>
    {#if open.output}
      <div class="body">
        {#each outputFields as f, i}
          <div class="sf-row">
            <input type="text" value={f.name} placeholder="field name" class="sf-name"
              oninput={(e) => setSchemaField('output', i, { name: e.currentTarget.value })} />
            <select value={f.type} onchange={(e) => setSchemaField('output', i, { type: e.currentTarget.value })}>
              {#each SCHEMA_TYPES as t}<option>{t}</option>{/each}
            </select>
            <label class="inp-req"><input type="checkbox" checked={f.required}
              onchange={(e) => setSchemaField('output', i, { required: e.currentTarget.checked })} /> req</label>
            <button class="rm" onclick={() => removeSchemaField('output', i)}>&times;</button>
            <input type="text" value={f.description} placeholder="description" class="sf-desc"
              oninput={(e) => setSchemaField('output', i, { description: e.currentTarget.value })} />
            <input type="text" value={f.default} placeholder="default" class="sf-def"
              oninput={(e) => setSchemaField('output', i, { default: e.currentTarget.value })} />
            <input type="text" value={f.enum} placeholder="enum (a, b, c)" class="sf-enum"
              oninput={(e) => setSchemaField('output', i, { enum: e.currentTarget.value })} />
          </div>
        {/each}
        <button class="add" onclick={() => addSchemaField('output')}>+ Add Field</button>
      </div>
    {/if}
  </div>

  <!-- Triggers -->
  <div class="ate-sec" id="ate-triggers">
    <button class="ate-head" onclick={() => { toggle('triggers'); loadTriggers(); }}>
      <svg class="chev" class:open={open.triggers} width="10" height="10" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="2"><path d="M6 4l4 4-4 4"/></svg>
      <span class="ico trg">T</span><h4>Triggers</h4><span class="cnt">{triggers.length}</span>
    </button>
    {#if open.triggers}
      <div class="body">
        {#each triggers as t}
          <div class="trg-item">
            <span class="trg-item-icon">{t.config?.type === 'webhook' ? 'W' : t.config?.type === 'cron' ? 'C' : 'E'}</span>
            <span class="trg-item-name">{t.name}</span>
            {#if t.config?.type === 'webhook'}<span class="trg-item-detail">{t.config.method ?? 'POST'} {t.config.path}</span>{/if}
            <span class="trg-item-status" class:active={t.enabled}>{t.enabled ? 'active' : 'paused'}</span>
            <button class="trg-item-edit" onclick={() => onopensub?.('trigger', t)} title="Edit">
              <svg width="10" height="10" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5"><path d="M11 1l4 4-9 9H2v-4z"/></svg>
            </button>
            <button class="rm" onclick={async () => { await mcpCall('delete_trigger', { id: t.id }); _triggersLoaded = false; loadTriggers(); }} title="Delete">&times;</button>
          </div>
        {/each}
        {#if triggers.length === 0}
          <div class="trg-empty-msg">No triggers attached to this template.</div>
        {/if}
        <div class="trg-actions-row">
          <button class="add" onclick={createQuickTrigger}>+ Quick HTTP</button>
          <button class="add" onclick={() => onopensub?.('trigger-new', { namespace, name })}>+ New Trigger</button>
        </div>
      </div>
    {/if}
  </div>


  {:else if activeTab === 'test'}
  <!-- ═══ TEST TAB ═══ -->
  <div class="test-full">
    <div class="test-split">
      <div class="test-pane">
        <div class="test-pane-label">INPUT</div>
        <textarea class="test-editor" bind:value={testPayload} spellcheck="false"></textarea>
      </div>
      <div class="test-pane">
        <div class="test-pane-label">OUTPUT
          {#if testResult?.phase === 'completed'}<span class="test-badge ok">completed</span>{/if}
          {#if testResult?.phase === 'failed'}<span class="test-badge err">failed</span>{/if}
          {#if testResult?.phase === 'running'}<span class="test-badge run">running</span>{/if}
        </div>
        <div class="test-output">
          {#if testError}<div class="test-err">{testError}</div>{/if}
          {#if testResult?.output}
            <pre class="test-json">{JSON.stringify(testResult.output, null, 2)}</pre>
          {:else if testRunning}
            <div class="test-waiting">Running... ({testResult?.poll ?? 0}s)</div>
          {:else}
            <div class="test-empty">Click Run to execute</div>
          {/if}
        </div>
      </div>
    </div>
    <div class="test-bar">
      {#if testResult?.instanceId}<span class="test-id">{testResult.instanceId}</span>{/if}
      {#if testResult?.via}<span class="test-via">via {testResult.via}{testResult.trigger ? `: ${testResult.trigger}` : ''}</span>{/if}
      <span style="flex:1"></span>
      <button class="test-run" onclick={runTest} disabled={testRunning}>
        {#if testRunning}
          <span class="test-spin"></span> Running
        {:else}
          <svg width="11" height="11" viewBox="0 0 16 16" fill="currentColor"><polygon points="3,2 13,8 3,14"/></svg>
          Run Test
        {/if}
      </button>
    </div>
  </div>
  {/if}

</div>

<style>
  .ate { overflow-y: auto; padding: 0 14px 10px; height: 100%; scroll-behavior: smooth; }

  /* Read-only (agent view): editing controls locked, but navigation (tabs, section
     headers) and the test runner stay clickable so you can browse and test. */
  .ate-readonly input,
  .ate-readonly select,
  .ate-readonly textarea,
  .ate-readonly button:not(.ate-tab):not(.ate-head):not(.test-run) {
    pointer-events: none;
    opacity: 0.75;
    cursor: default;
  }

  /* Tabs */
  /* Scrolls horizontally rather than clipping: seven tabs do not fit a narrow
     host (the in-chat artifact pane, or the editor's right pane when dragged
     small), and a clipped tab bar silently hides Permissions/Memory/IO/Test. */
  .ate-tabs { display: flex; gap: 0; border-bottom: 1px solid var(--border); margin: 0 -14px 8px; padding: 0 14px; position: sticky; top: 0; background: var(--bg-page); z-index: 2; overflow-x: auto; overflow-y: hidden; scrollbar-width: none; -webkit-overflow-scrolling: touch; }
  .ate-tabs::-webkit-scrollbar { display: none; }
  .ate-tab { padding: 8px 16px; border: none; border-bottom: 2px solid transparent; background: none; color: var(--text-3); font-size: 12px; font-weight: 550; cursor: pointer; display: flex; align-items: center; gap: 4px; transition: all 0.12s; flex: 0 0 auto; white-space: nowrap; }
  .ate-tab.active { color: var(--text-1); border-bottom-color: var(--spec, #5530a8); }
  .ate-tab:hover:not(.active) { color: var(--text-2); }
  .tab-dot { width: 6px; height: 6px; border-radius: 50%; }
  .tab-dot.ok { background: var(--runtime, #1a7158); }
  .tab-dot.err { background: var(--err, #dc2626); }

  .ate-sec { border: 1px solid var(--border); border-radius: var(--r-sm); margin-bottom: 4px; overflow: hidden; }
  .ate-head { display: flex; align-items: center; gap: 6px; width: 100%; padding: 7px 10px; border: none; background: var(--bg-surface); cursor: pointer; text-align: left; }
  .ate-head:hover { background: var(--bg-subtle); }
  .ate-head h4 { font-size: 12px; font-weight: 550; }
  .chev { color: var(--text-4); transition: transform 0.15s; flex-shrink: 0; }
  .chev.open { transform: rotate(90deg); }
  .cnt { font-family: var(--font-mono); font-size: 9px; color: var(--text-4); background: var(--bg-subtle); padding: 0 4px; border-radius: 2px; margin-left: auto; }

  .ico { width: 18px; height: 18px; border-radius: 3px; display: grid; place-items: center; font-size: 8px; font-weight: 700; color: white; font-family: var(--font-mono); flex-shrink: 0; }
  .ico.id { background: var(--text-3, #888); }
  .ico.cfg { background: var(--ephemeral, #3a7fff); }
  .ico.ext { background: var(--signal, #7d4bbb); }

  .fn-badge { font-family: var(--font-mono); font-size: 9px; color: var(--spec-ink, #5530a8); background: var(--spec-bg, #f0ebfa); padding: 1px 6px; border-radius: 3px; margin-left: auto; }

  .body { padding: 10px 12px 12px; border-top: 1px solid var(--border); background: var(--bg-page); }

  /* Fields — sized to match Flow Editor */
  .body input, .body select, .body textarea {
    padding: 7px 10px; border: 1px solid var(--border); border-radius: var(--r-sm, 4px);
    background: var(--bg-surface); color: var(--text-1); font-size: 13px; font-family: var(--font-mono); outline: none; flex: 1; min-width: 0;
  }
  .body input:focus, .body select:focus, .body textarea:focus { border-color: var(--spec); }
  .body textarea { width: 100%; resize: vertical; }
  .body label { display: flex; flex-direction: column; gap: 4px; font-size: 12px; color: var(--text-2); font-weight: 500; }
  .body .full { display: block; margin-bottom: 6px; }
  .body .full input { width: 100%; }

  .row2 { display: grid; grid-template-columns: 1fr 1fr; gap: 8px; margin-bottom: 6px; }
  .sub-label { font-size: 11px; font-weight: 600; text-transform: uppercase; letter-spacing: 0.05em; color: var(--text-2); margin: 10px 0 6px; }
  .perm-hint { font-size: 12px; color: var(--text-4); margin: 0 0 12px; line-height: 1.5; }
  .linkish { border: none; background: none; padding: 0; color: var(--spec-ink, #5530a8); font: inherit; cursor: pointer; text-decoration: underline; }
  .perm-summary { display: flex; align-items: center; gap: 12px; margin: 12px 0; }
  .perm-badge { font-family: var(--font-mono); font-size: 11px; font-weight: 600; padding: 3px 9px; border-radius: 100px; }
  .perm-badge.ok { color: var(--ok, #1a7158); background: var(--ok-bg, #e1f1ea); }
  .perm-badge.warn { color: var(--warn, #9f6b07); background: var(--warn-bg, #f5ebd5); }
  .perm-chk.danger.on { background: var(--err-bg, #fde8e8); color: var(--err, #d44); border-color: var(--err, #d44); }
  .perm-toggle { display: flex; align-items: center; gap: 10px; font-size: 13px; color: var(--text-2);
    padding: 10px 12px; border: 1px solid var(--border); border-radius: 7px; background: var(--bg-surface); cursor: pointer; }
  .perm-toggle input { width: 15px; height: 15px; accent-color: var(--spec-ink, #5530a8); }
  .perm-toggle strong { color: var(--text-1); }
  .perm-groups { display: flex; flex-direction: column; gap: 14px; margin-top: 14px; }
  .perm-group-head { display: flex; align-items: center; justify-content: space-between; margin-bottom: 8px; }
  .perm-group-name { font-size: 10.5px; font-weight: 600; letter-spacing: 0.08em; text-transform: uppercase; color: var(--text-4); }
  .perm-group-name.danger { color: var(--err, #d44); }
  .perm-all { font-size: 11px; border: none; background: none; color: var(--spec-ink, #5530a8); cursor: pointer; font-weight: 500; }
  .perm-all:hover { text-decoration: underline; }
  .perm-tools { display: flex; flex-wrap: wrap; gap: 6px; }
  .perm-chk { display: inline-flex; align-items: center; gap: 6px; font-family: var(--font-mono); font-size: 11.5px;
    padding: 4px 9px; border: 1px solid var(--border); border-radius: 100px; background: var(--bg-surface);
    color: var(--text-3); cursor: pointer; user-select: none; }
  .perm-chk input { display: none; }
  .perm-chk.on { background: var(--spec-bg, #efeafc); color: var(--spec-ink, #5530a8); border-color: var(--spec-ink, #5530a8); }
  .sub-label:first-child { margin-top: 0; }

  .status { font-size: 11px; color: var(--text-3); padding: 8px 0; font-style: italic; }
  .status.err { color: var(--err, #dc2626); }

  .raw-yaml {
    width: 100%; padding: 8px; border: 1px solid var(--border); border-radius: 3px;
    background: var(--bg-surface); color: var(--text-1); font-family: var(--font-mono);
    font-size: 11px; outline: none; resize: vertical;
  }

  /* Extensions */
  .ext-card { border: 1px solid var(--border); border-radius: 4px; padding: 6px 8px; margin-bottom: 4px; background: var(--bg-surface); }
  .ext-top { display: flex; gap: 4px; align-items: center; }
  .ext-name { font-weight: 550; font-size: 11px; max-width: 140px; }
  .ext-when { display: flex; gap: 4px; align-items: center; margin-top: 3px; font-size: 10px; }
  .ext-when-label { font-size: 9px; color: var(--text-4); font-family: var(--font-mono); flex-shrink: 0; }
  .ext-when input { flex: 1; font-size: 10px; }

  .rm { width: 18px; height: 18px; border: none; border-radius: 2px; background: none; color: var(--text-5); cursor: pointer; display: grid; place-items: center; font-size: 13px; flex-shrink: 0; }
  .rm:hover { color: var(--err); background: var(--err-bg, #fde8e8); }
  .add { border: 1px dashed var(--border); border-radius: 3px; padding: 3px 8px; font-size: 10px; color: var(--text-4); background: none; cursor: pointer; margin-top: 4px; }
  .add:hover { border-color: var(--spec); color: var(--spec); }

  .err-count { font-family: var(--font-mono); font-size: 9px; color: var(--err, #dc2626); background: var(--err-bg, #fde8e8); padding: 0 4px; border-radius: 2px; margin-left: 4px; }

  /* Tab body (direct content, no collapsible sections) */
  .tab-body { padding: 2px 0; }
  .tab-body textarea { width: 100%; padding: 7px 10px; border: 1px solid var(--border); border-radius: var(--r-sm, 4px); background: var(--bg-surface); color: var(--text-1); font-size: 13px; font-family: var(--font-mono); outline: none; resize: vertical; }
  .tab-body textarea:focus { border-color: var(--spec); }
  .tab-body label { display: flex; flex-direction: column; gap: 4px; font-size: 12px; color: var(--text-2); font-weight: 500; }
  .tab-body input, .tab-body select { padding: 7px 10px; border: 1px solid var(--border); border-radius: var(--r-sm, 4px); background: var(--bg-surface); color: var(--text-1); font-size: 13px; font-family: var(--font-mono); outline: none; }
  .tab-body input:focus, .tab-body select:focus { border-color: var(--spec); }
  .tab-body .full { display: block; margin-bottom: 6px; }
  .tab-body .full input { width: 100%; }
  .row3 { display: grid; grid-template-columns: 1fr 1fr 1fr; gap: 8px; margin-bottom: 6px; }

  /* Resource picker with "+ New" button */
  .res-picker { display: flex; gap: 6px; align-items: center; }
  .res-picker select { flex: 1; padding: 7px 10px; border: 1px solid var(--border); border-radius: var(--r-sm, 4px); background: var(--bg-surface); color: var(--text-1); font-size: 13px; font-family: var(--font-mono); outline: none; }
  .res-picker select:focus { border-color: var(--spec); }
  .new-res-btn { padding: 6px 12px; border: 1px dashed var(--spec); border-radius: 5px; background: none; color: var(--spec); font-size: 11px; font-weight: 550; cursor: pointer; white-space: nowrap; transition: all 0.12s; }
  .new-res-btn:hover { background: var(--spec-bg); }
  .add-row { display: flex; gap: 6px; align-items: center; margin-top: 4px; }

  /* Resource detail card */
  .res-detail { background: var(--bg-subtle); border: 1px solid var(--border); border-radius: 4px; padding: 6px 10px; margin-top: 6px; }
  .res-detail-row { display: flex; justify-content: space-between; font-size: 12px; padding: 2px 0; }
  .res-detail-row span:first-child { color: var(--text-3); font-weight: 500; }
  .res-detail-row span:last-child { font-family: var(--font-mono); color: var(--text-1); }

  /* Tool items (attached) */
  .tool-item { display: flex; align-items: center; gap: 8px; padding: 7px 10px; border: 1px solid var(--border); border-radius: 6px; margin-bottom: 4px; background: var(--bg-surface); }
  .tool-item-icon { width: 22px; height: 22px; border-radius: 5px; display: grid; place-items: center; font-size: 10px; font-weight: 700; font-family: var(--font-mono); flex-shrink: 0; }
  .tool-item-icon.mcp { background: var(--control-bg); color: var(--control); }
  .tool-item-icon.flow { background: var(--spec-bg); color: var(--spec-ink); }
  .tool-item-name { font-family: var(--font-mono); font-size: 13px; font-weight: 550; color: var(--text-1); flex: 1; }
  .tool-item-ref { font-family: var(--font-mono); font-size: 11px; color: var(--text-3); }
  .tool-name-input { flex: 0 0 120px !important; font-weight: 550; padding: 4px 8px !important; font-size: 12px !important; }

  /* Search combo */
  .combo { position: relative; margin: 4px 0 8px; }
  .combo-input { display: flex; align-items: center; gap: 6px; padding: 7px 10px; border: 1px solid var(--border); border-radius: 6px; background: var(--bg-surface); color: var(--text-4); }
  .combo-input input { border: none; background: none; flex: 1; font-size: 13px; font-family: var(--font-mono); outline: none; color: var(--text-1); padding: 0; }
  .combo-input:focus-within { border-color: var(--spec); }
  .combo-list { position: absolute; top: 100%; left: 0; right: 0; z-index: 10; border: 1px solid var(--border); border-top: none; border-radius: 0 0 6px 6px; background: var(--bg-surface); box-shadow: 0 4px 12px rgba(0,0,0,0.1); max-height: 180px; overflow-y: auto; }
  .combo-item { display: flex; align-items: center; gap: 8px; width: 100%; padding: 7px 10px; border: none; border-bottom: 1px solid var(--border); background: none; cursor: pointer; text-align: left; font-size: 12px; font-family: var(--font-mono); color: var(--text-1); }
  .combo-item:last-child { border-bottom: none; }
  .combo-item:hover { background: var(--bg-subtle); }

  /* Extension cards */
  .ext-card.ext-empty { opacity: 0.6; }
  .ext-card.ext-empty:hover { opacity: 1; }
  .ext-phase-name { font-family: var(--font-mono); font-size: 13px; font-weight: 600; color: var(--text-1); text-transform: capitalize; }
  .ext-desc-inline { font-size: 11px; color: var(--text-4); margin-left: auto; }
  .ext-flows { display: grid; grid-template-columns: 1fr 1fr; gap: 8px; margin-top: 4px; }

  /* Extension phases (before/after flow wiring) */
  .ext-phase { margin-top: 4px; padding-left: 8px; border-left: 2px solid var(--border); }
  .ext-phase-label { font-size: 10px; font-weight: 600; color: var(--text-3); text-transform: uppercase; letter-spacing: 0.04em; display: block; margin-bottom: 3px; }
  .ext-flow-row { display: flex; gap: 4px; align-items: center; margin-bottom: 3px; }
  .ext-flow-row select { flex: 1; padding: 5px 8px; border: 1px solid var(--border); border-radius: 4px; background: var(--bg-surface); color: var(--text-1); font-size: 12px; font-family: var(--font-mono); outline: none; }
  .ext-flow-row select:focus { border-color: var(--spec); }
  .add-sm { border: 1px dashed var(--border); border-radius: 3px; padding: 2px 8px; font-size: 10px; color: var(--text-4); background: none; cursor: pointer; margin-top: 2px; }
  .add-sm:hover { border-color: var(--spec); color: var(--spec); }

  /* Memory tiers */
  .mem-tier { display: flex; align-items: center; gap: 10px; padding: 8px 10px; border: 1px solid var(--border); border-radius: 4px; margin-bottom: 4px; background: var(--bg-surface); }
  .mem-tier-name { font-family: var(--font-mono); font-size: 13px; font-weight: 550; color: var(--text-1); text-transform: capitalize; min-width: 100px; }
  .mem-chk { display: flex; align-items: center; gap: 4px; font-size: 12px; color: var(--text-2); cursor: pointer; flex-direction: row !important; }
  .mem-num { display: inline-flex; align-items: center; gap: 4px; font-size: 12px; color: var(--text-3); }
  .mem-num input { width: 58px; }
  .mem-chk input { accent-color: var(--spec); }
  .mem-hint { font-size: 11px; color: var(--text-4); line-height: 1.6; margin-top: 10px; padding: 8px 10px; background: var(--bg-subtle); border-radius: 4px; }
  .mem-hint strong { color: var(--text-2); font-weight: 600; }

  .ico.inp { background: #0891b2; }
  .ico.out { background: #9333ea; }
  .ico.trg { background: var(--runtime, #1a7158); }
  .ico.tst { background: var(--runtime, #1a7158); display: grid; place-items: center; }

  /* Input fields */
  .inp-row { display: flex; gap: 4px; align-items: center; margin-bottom: 3px; }
  .inp-name { flex: 1; font-weight: 550; }
  .inp-req { display: flex; align-items: center; gap: 2px; font-size: 9px; color: var(--text-3); white-space: nowrap; }
  .inp-req input { accent-color: var(--spec); }

  /* Schema field rows (input/output): line1 name/type/req/×, line2 description, line3 default+enum */
  .sf-row { display: flex; flex-wrap: wrap; gap: 4px; align-items: center; margin-bottom: 6px; padding-bottom: 6px; border-bottom: 1px solid var(--divider); }
  .sf-row input, .sf-row select { font-size: 11px; padding: 3px 6px; border: 1px solid var(--border); border-radius: 4px; background: var(--bg-surface); color: var(--text-1); font-family: var(--font-mono); min-width: 0; }
  .sf-name { flex: 1 1 120px; font-weight: 550; }
  .sf-desc { flex: 1 1 100%; color: var(--text-2); }
  .sf-def { flex: 1 1 90px; }
  .sf-enum { flex: 2 1 140px; }

  /* Triggers */
  .trg-empty-msg { font-size: 11px; color: var(--text-4); font-style: italic; margin-bottom: 6px; }
  .trg-item { display: flex; align-items: center; gap: 5px; padding: 5px 6px; border: 1px solid var(--border); border-radius: 4px; margin-bottom: 3px; background: var(--bg-surface); font-size: 11px; }
  .trg-item-icon { width: 18px; height: 18px; border-radius: 3px; background: var(--runtime-bg); color: var(--runtime); display: grid; place-items: center; font-size: 8px; font-weight: 700; font-family: var(--font-mono); flex-shrink: 0; }
  .trg-item-name { font-weight: 550; font-family: var(--font-mono); color: var(--text-1); }
  .trg-item-detail { font-family: var(--font-mono); font-size: 10px; color: var(--text-3); }
  .trg-item-status { font-family: var(--font-mono); font-size: 9px; color: var(--text-4); text-transform: uppercase; margin-left: auto; }
  .trg-item-status.active { color: var(--runtime); }
  .trg-item-edit { width: 20px; height: 20px; border: 1px solid var(--border); border-radius: 3px; background: var(--bg-surface); color: var(--text-4); cursor: pointer; display: grid; place-items: center; }
  .trg-item-edit:hover { background: var(--bg-subtle); color: var(--text-1); }
  .trg-actions-row { display: flex; gap: 4px; margin-top: 4px; }

  /* Test */
  .test-badge { font-family: var(--font-mono); font-size: 9px; padding: 1px 5px; border-radius: 3px; margin-left: auto; font-weight: 600; }
  .test-badge.ok { background: var(--runtime-bg); color: var(--runtime); }
  .test-badge.err { background: var(--err-bg, #fde8e8); color: var(--err); }
  .test-badge.run { background: var(--control-bg); color: var(--control); }

  .test-full { display: flex; flex-direction: column; flex: 1; min-height: 62vh; }
  .test-split { display: grid; grid-template-columns: 1fr 1fr; gap: 0; border: 1px solid var(--border); border-radius: 4px; overflow: hidden; flex: 1; min-height: 0; }
  .test-pane { display: flex; flex-direction: column; min-height: 0; }
  .test-pane:first-child { border-right: 1px solid var(--border); }
  .test-pane-label { font-size: 9px; font-weight: 700; color: var(--text-4); text-transform: uppercase; letter-spacing: 0.05em; padding: 4px 8px; background: var(--bg-subtle); border-bottom: 1px solid var(--border); }
  .test-editor { flex: 1; min-height: 280px; border: none; background: var(--bg-page); color: var(--text-1); font-family: var(--font-mono); font-size: 11px; padding: 6px 8px; resize: none; outline: none; }
  .test-output { flex: 1; min-height: 280px; padding: 6px 8px; overflow-y: auto; }
  .test-err { color: var(--err); font-size: 10px; font-family: var(--font-mono); padding: 4px 6px; background: var(--err-bg); border-radius: 3px; margin-bottom: 4px; }
  .test-json { font-family: var(--font-mono); font-size: 10px; color: var(--text-1); margin: 0; white-space: pre-wrap; word-break: break-word; }
  .test-waiting { font-size: 11px; color: var(--control); font-style: italic; }
  .test-empty { font-size: 11px; color: var(--text-4); }

  .test-bar { display: flex; align-items: center; gap: 6px; padding: 8px 0; flex-shrink: 0; }
  .test-id { font-family: var(--font-mono); font-size: 8px; color: var(--text-4); }
  .test-via { font-size: 9px; color: var(--text-3); font-family: var(--font-mono); }
  .test-run { display: flex; align-items: center; gap: 4px; padding: 5px 14px; border: 1px solid var(--runtime); border-radius: 5px; background: var(--runtime); color: white; font-size: 11px; font-weight: 600; cursor: pointer; }
  .test-run:hover:not(:disabled) { opacity: 0.9; }
  .test-run:disabled { opacity: 0.5; cursor: default; }
  .test-spin { width: 10px; height: 10px; border: 2px solid rgba(255,255,255,0.3); border-top-color: white; border-radius: 50%; animation: spin 0.8s linear infinite; }
  @keyframes spin { to { transform: rotate(360deg); } }

  .ico.slot { background: var(--control, #c97c2e); }

  /* Slots */
  .slot-row { display: flex; align-items: center; gap: 5px; padding: 4px 0; font-size: 11px; }
  .slot-name { font-family: var(--font-mono); font-weight: 550; color: var(--text-2); text-transform: capitalize; }
  .slot-type { font-family: var(--font-mono); font-size: 9px; padding: 0 4px; border-radius: 2px; color: var(--text-3); background: var(--bg-subtle); }
  .slot-type.ext { color: var(--signal-ink, #5530a8); background: var(--signal-bg, #f0ebfa); }
  .slot-type.intg { color: var(--runtime, #1a7158); background: var(--runtime-bg, #e6f4ef); }
  .slot-req { font-size: 8px; color: var(--err, #dc2626); font-weight: 600; text-transform: uppercase; }
  .slot-when { font-size: 8px; color: var(--text-4); background: var(--bg-subtle); padding: 0 3px; border-radius: 2px; cursor: help; }
  .slot-pick { flex: 1; min-width: 0; font-size: 10px; }
  .slot-status { font-size: 8px; padding: 1px 5px; border-radius: 3px; font-family: var(--font-mono); text-transform: uppercase; color: var(--text-4); background: var(--bg-subtle); margin-left: auto; }
  .slot-status.wired { color: var(--runtime, #1a7158); background: var(--runtime-bg, #e6f4ef); }
  .slot-desc { font-size: 9px; color: var(--text-4); padding-left: 4px; display: block; margin-bottom: 4px; }
  .perm-count { font-weight: 400; opacity: 0.6; margin-left: 4px; }
  .perm-err { color: var(--danger, #b42318); }
</style>
