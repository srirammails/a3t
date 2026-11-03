<script lang="ts">
  import * as YAML from 'yaml';
  import SchemaConfigForm from './SchemaConfigForm.svelte';
  import { getEditorContext } from '$lib/editor/context';
  import { isCncfDsl, normaliseTasks } from './workflowParser';
  import type { FunctionManifest } from '$lib/editor/types';

  let { value = '', onchange, focus = null }: {
    value: string; onchange?: (val: string) => void;
    /// Open this step and bring it into view (a stamp per request).
    focus?: { name: string; n: number } | null;
  } = $props();

  // Get registry manifests for schema-driven config rendering
  function getManifest(callType: string): FunctionManifest | null {
    const ctx = getEditorContext();
    if (!ctx?.ready) return null;
    // Try exact match first, then with common prefixes
    return ctx.functions.get(callType) ??
           ctx.functions.get(callType.replace(/^call:/, '')) ??
           null;
  }

  function handleSchemaConfigChange(idx: number, path: string[], val: unknown) {
    const step = parsed.steps[idx];
    if (!step.config) step.config = {};
    // path is like ["config", "field"] or deeper
    let obj = step.config;
    const keys = path.length > 0 && path[0] === 'config' ? path.slice(1) : path;
    for (let i = 0; i < keys.length - 1; i++) {
      if (obj[keys[i]] === undefined) obj[keys[i]] = {};
      obj = obj[keys[i]];
    }
    const lastKey = keys[keys.length - 1];
    if (lastKey) {
      if (val === undefined || val === '') delete obj[lastKey];
      else obj[lastKey] = val;
    }
    parsed = { ...parsed };
    emit();
  }

  // Parse YAML into structured object
  let parsed = $state<any>(null);
  let parseError = $state('');

  // CNCF DSL 1.0.3 keeps tasks in `do:` as single-key maps; the legacy FlowR
  // shape uses `steps:` with name/type/config. This form was written against
  // `steps`, so a CNCF flow rendered only its document fields and none of its
  // tasks. Normalise `do` into `steps` for editing, then write it back on emit.
  let cncf = $state(false);

  $effect(() => {
    try {
      const next = YAML.parse(value) ?? {};
      cncf = isCncfDsl(next) && Array.isArray(next.do);
      if (cncf) {
        const src = next.do as any[];
        next.steps = normaliseTasks(src).map((t: any, i: number) => ({
          ...t,
          // Keep the original entry so edits are merged into it rather than
          // the task being rebuilt from `type` — reconstructing composite
          // tasks (switch/for/fork/try) from their normalised form would
          // silently mangle them.
          __src: src[i],
        }));
      }
      parsed = next;
      parseError = '';
    } catch (e: any) {
      parseError = e.message ?? 'Invalid YAML';
    }
  });

  /// Deep-clone YAML-shaped data (arrays / plain objects / primitives).
  /// `structuredClone` cannot be used here: `parsed` is `$state`, so Svelte
  /// hands back deeply reactive Proxies, and the structured-clone algorithm
  /// rejects them ("could not be cloned"). Walking the structure ourselves is
  /// proxy-transparent and exact for anything `YAML.parse` can produce.
  function plainClone<T>(v: T): T {
    if (Array.isArray(v)) return v.map(plainClone) as unknown as T;
    if (v && typeof v === 'object') {
      const out: any = {};
      for (const k of Object.keys(v as any)) out[k] = plainClone((v as any)[k]);
      return out;
    }
    return v;
  }

  /// Where in a CNCF task body its editable config sits: `with`, `set`, or the
  /// process under `run` (`run.workflow`, `run.shell`…).
  function configSlot(body: any): string[] | null {
    if (!body || typeof body !== 'object') return null;
    if (body.call !== undefined) return ['with'];
    if (body.set !== undefined) return ['set'];
    if (body.run !== undefined) {
      if (body.with !== undefined) return ['with'];
      const kind = ['workflow', 'container', 'script', 'shell'].find((k) => body.run && typeof body.run === 'object' && body.run[k] && typeof body.run[k] === 'object');
      return kind ? ['run', kind] : ['run'];
    }
    return null;   // composite task — leave its body alone
  }

  /// Rebuild `do:` from the edited steps, preserving anything the form does not
  /// model. Only leaf tasks (call/set/run) have their config written back.
  function stepsToDo(steps: any[]): any[] {
    return steps.map((st: any) => {
      const orig = st.__src;
      if (!orig || typeof orig !== 'object') {
        // Added in the form (legacy shape) — emit it as a CNCF set task.
        return { [st.name ?? 'step']: { set: st.config ?? {} } };
      }
      const key = Object.keys(orig)[0];
      const body = plainClone(orig[key]);
      const slot = configSlot(body);
      if (slot) {
        let at = body;
        for (const k of slot.slice(0, -1)) at = at[k];
        at[slot[slot.length - 1]] = st.config ?? {};
      }
      return { [st.name ?? key]: body };
    });
  }

  /// Document fields live under `document:` in CNCF DSL and at the top level in
  /// the legacy FlowR shape. Reading/writing the wrong one made NAME render
  /// empty while NAMESPACE/VERSION showed their *fallback* defaults rather than
  /// the document's real values — so the form looked populated but edits went
  /// to keys nothing reads.
  const DOC_KEY: Record<string, string> = {
    name: 'name', namespace: 'namespace', version: 'version',
    description: 'summary',   // CNCF `document` has summary/title, not description
  };
  function docPath(key: string): string[] {
    return cncf ? ['document', DOC_KEY[key] ?? key] : [key];
  }
  function docVal(key: string): string {
    const v = cncf ? parsed?.document?.[DOC_KEY[key] ?? key] : parsed?.[key];
    return v === undefined || v === null ? '' : String(v);
  }

  function emit() {
    try {
      const out = { ...parsed };
      if (cncf) {
        out.do = stepsToDo(parsed.steps ?? []);
        delete out.steps;      // `steps` was a view, never part of the document
      }
      // Serialise a plain snapshot, never the `$state` proxies themselves, so
      // the output never depends on Svelte's reactive wrappers.
      const text = YAML.stringify(plainClone(out), { indent: 2, lineWidth: 120 });
      onchange?.(text);
    } catch (e) {
      // A throw here used to be swallowed, so a form edit that could not be
      // serialised looked exactly like an edit that did nothing.
      console.error('[SchemaForm] emit failed:', e);
      parseError = `Could not apply this change: ${(e as Error).message ?? e}`;
    }
  }

  function updateField(path: string[], val: any) {
    let obj = parsed;
    for (let i = 0; i < path.length - 1; i++) {
      if (obj[path[i]] === undefined) obj[path[i]] = {};
      obj = obj[path[i]];
    }
    const key = path[path.length - 1];
    if (val === '' || val === undefined) {
      delete obj[key];
    } else {
      obj[key] = val;
    }
    parsed = { ...parsed };
    emit();
  }

  function updateStepField(idx: number, field: string, val: any) {
    const step = parsed.steps[idx];
    if (val === '' || val === undefined) {
      delete step[field];
    } else {
      step[field] = val;
    }
    parsed = { ...parsed };
    emit();
  }

  /// A CNCF task's resource is `with._resource` (what the engine reads); the
  /// legacy shape keeps it on the step.
  const resourceOf = (step: any): string => String((cncf ? step.config?._resource : step.resource) ?? '');
  function setResource(idx: number, val: string) {
    if (cncf) updateStepConfig(idx, '_resource', val);
    else updateStepField(idx, 'resource', val);
  }

  function updateStepConfig(idx: number, key: string, val: any) {
    const step = parsed.steps[idx];
    if (!step.config) step.config = {};
    if (val === '' || val === undefined) {
      delete step.config[key];
    } else {
      step.config[key] = val;
    }
    parsed = { ...parsed };
    emit();
  }

  function updateStepConfigNested(idx: number, path: string[], val: any) {
    const step = parsed.steps[idx];
    if (!step.config) step.config = {};
    let obj = step.config;
    for (let i = 0; i < path.length - 1; i++) {
      if (obj[path[i]] === undefined) obj[path[i]] = {};
      obj = obj[path[i]];
    }
    obj[path[path.length - 1]] = val;
    parsed = { ...parsed };
    emit();
  }

  function changeStepType(idx: number, newType: string) {
    const step = parsed.steps[idx];
    const oldType = step.type;
    step.type = newType;
    // Reset config to defaults for new type
    step.config = getDefaultConfig(newType);
    // Clear resource if moving to a type that doesn't need one
    if (!NEEDS_RESOURCE.has(newType)) {
      delete step.resource;
    }
    parsed = { ...parsed };
    emit();
  }

  function removeStep(idx: number) {
    if (parsed?.steps) {
      parsed.steps = parsed.steps.filter((_: any, i: number) => i !== idx);
      parsed = { ...parsed };
      emit();
    }
  }

  function addStep() {
    if (!parsed.steps) parsed.steps = [];
    parsed.steps = [...parsed.steps, { name: `step-${parsed.steps.length + 1}`, type: 'set', config: { key: 'value' } }];
    parsed = { ...parsed };
    emit();
  }

  function moveStep(idx: number, dir: -1 | 1) {
    const arr = parsed.steps;
    const newIdx = idx + dir;
    if (newIdx < 0 || newIdx >= arr.length) return;
    [arr[idx], arr[newIdx]] = [arr[newIdx], arr[idx]];
    parsed = { ...parsed };
    emit();
  }

  let expandedSteps = $state<Record<number, boolean>>({});
  let root = $state<HTMLElement | null>(null);
  let focused = 0;
  // A step asked for (dropped on the flow): open it, close the others, show it.
  $effect(() => {
    if (!focus || focus.n === focused || !parsed?.steps) return;
    const idx = parsed.steps.findIndex((s: any) => s.name === focus.name);
    if (idx < 0) return;
    focused = focus.n;
    expandedSteps = { [idx]: true };
    queueMicrotask(() => root?.querySelector(`[data-step="${CSS.escape(focus.name)}"]`)?.scrollIntoView({ block: 'start', behavior: 'smooth' }));
  });

  // ── Task type schemas ──

  interface FieldDef {
    key: string;
    label: string;
    type: 'text' | 'number' | 'select' | 'textarea' | 'boolean' | 'object';
    placeholder?: string;
    options?: string[];
    required?: boolean;
    nested?: FieldDef[];
  }

  const TASK_TYPES = [
    'set', 'call:http', 'call:lancedb', 'call:llm', 'call:agent', 'call:sql',
    'call:qdrant', 'call:adb', 'call:mcp', 'call:human',
    'call:redis',
    'run:workflow', 'run:shell', 'run:script',
    'inline:lua', 'inline:js',
    'switch', 'for', 'fork', 'try', 'raise', 'wait',
    'state:get', 'state:set', 'input:file', 'output:file',
  ];

  const NEEDS_RESOURCE = new Set([
    'call:http', 'call:lancedb', 'call:llm', 'call:agent', 'call:sql',
    'call:qdrant', 'call:adb', 'call:mcp', 'call:human', 'call:redis',
  ]);

  const TYPE_SCHEMAS: Record<string, FieldDef[]> = {
    'set': [
      // set is freeform key-value, handled separately
    ],
    'call:http': [
      { key: 'method', label: 'Method', type: 'select', options: ['GET', 'POST', 'PUT', 'DELETE', 'PATCH'], required: true },
      { key: 'endpoint', label: 'Endpoint', type: 'object', nested: [
        { key: 'uri', label: 'URI', type: 'text', placeholder: 'https://api.example.com/path', required: true },
      ]},
      { key: 'headers', label: 'Headers', type: 'object' },
      { key: 'body', label: 'Body', type: 'object' },
      { key: 'query', label: 'Query Params', type: 'object' },
      { key: 'timeout', label: 'Timeout (s)', type: 'number', placeholder: '30' },
    ],
    'call:llm': [
      { key: 'prompt', label: 'Prompt', type: 'textarea', required: true, placeholder: 'User prompt / instruction' },
      { key: 'system_prompt', label: 'System Prompt', type: 'textarea', placeholder: 'Override system prompt' },
      { key: 'model', label: 'Model', type: 'text', placeholder: 'Override model name' },
      { key: 'temperature', label: 'Temperature', type: 'number', placeholder: '0.7' },
      { key: 'max_tokens', label: 'Max Tokens', type: 'number', placeholder: '4096' },
      { key: 'output_schema', label: 'Output Schema', type: 'object' },
    ],
    'call:agent': [
      { key: 'prompt', label: 'Prompt', type: 'textarea', required: true },
      { key: 'session_id', label: 'Session ID', type: 'text' },
      { key: 'agent_id', label: 'Agent ID', type: 'text' },
      { key: 'max_iterations', label: 'Max Iterations', type: 'number', placeholder: '10' },
      { key: 'context', label: 'Context', type: 'object' },
    ],
    'call:lancedb': [
      { key: 'operation', label: 'Operation', type: 'select', options: ['query', 'insert', 'delete', 'create_table', 'list_tables'], required: true },
      { key: 'table', label: 'Table', type: 'text', required: true },
      { key: 'filter', label: 'Filter', type: 'text', placeholder: "column = 'value'" },
      { key: 'limit', label: 'Limit', type: 'number', placeholder: '100' },
      { key: 'select', label: 'Select Columns', type: 'text', placeholder: 'col1, col2' },
      { key: 'records', label: 'Records', type: 'object' },
      { key: 'schema', label: 'Schema', type: 'object' },
    ],
    'call:sql': [
      { key: 'query', label: 'SQL Query', type: 'textarea', required: true, placeholder: 'SELECT * FROM ...' },
      { key: 'params', label: 'Parameters', type: 'object' },
    ],
    'call:adb': [
      { key: 'query', label: 'AQL Query', type: 'textarea', required: true, placeholder: 'RECALL FROM WORKING WHERE ...' },
      { key: 'variables', label: 'Variables', type: 'object' },
    ],
    'call:qdrant': [
      { key: 'operation', label: 'Operation', type: 'select', options: ['search', 'upsert', 'delete', 'get', 'create_collection'], required: true },
      { key: 'collection', label: 'Collection', type: 'text' },
      { key: 'vector', label: 'Vector', type: 'text', placeholder: '[0.1, 0.2, ...]' },
      { key: 'limit', label: 'Limit', type: 'number', placeholder: '10' },
      { key: 'filter', label: 'Filter', type: 'object' },
      { key: 'points', label: 'Points', type: 'object' },
    ],
    'call:redis': [
      { key: 'operation', label: 'Operation', type: 'select', options: ['get', 'set', 'del', 'hget', 'hset', 'lpush', 'rpush', 'lrange', 'publish', 'keys'], required: true },
      { key: 'key', label: 'Key', type: 'text' },
      { key: 'value', label: 'Value', type: 'text' },
      { key: 'field', label: 'Field', type: 'text' },
      { key: 'ttl', label: 'TTL (s)', type: 'number' },
      { key: 'pattern', label: 'Pattern', type: 'text' },
      { key: 'channel', label: 'Channel', type: 'text' },
    ],
    'call:mcp': [
      { key: 'tool', label: 'Tool Name', type: 'text', required: true },
      { key: 'arguments', label: 'Arguments', type: 'object' },
    ],
    'call:human': [
      { key: 'prompt', label: 'Prompt', type: 'textarea', required: true },
      { key: 'timeout', label: 'Timeout (s)', type: 'number', placeholder: '3600' },
    ],
    'run:workflow': [
      { key: 'namespace', label: 'Namespace', type: 'text', placeholder: 'default', required: true },
      { key: 'name', label: 'Workflow Name', type: 'text', required: true },
      { key: 'version', label: 'Version', type: 'text', placeholder: 'latest' },
      { key: 'input', label: 'Input', type: 'object' },
    ],
    'run:shell': [
      { key: 'command', label: 'Command', type: 'textarea', required: true, placeholder: 'echo hello' },
      { key: 'timeout', label: 'Timeout (s)', type: 'number', placeholder: '30' },
    ],
    'run:script': [
      { key: 'language', label: 'Language', type: 'select', options: ['lua', 'js', 'python'] },
      { key: 'code', label: 'Code', type: 'textarea', placeholder: 'Inline script code' },
      { key: 'source', label: 'Source File', type: 'text', placeholder: '/path/to/script' },
      { key: 'arguments', label: 'Arguments', type: 'text' },
    ],
    'inline:lua': [
      { key: 'code', label: 'Lua Code', type: 'textarea', required: true, placeholder: 'return { result = state.input.x + 1 }' },
    ],
    'inline:js': [
      { key: 'code', label: 'JavaScript Code', type: 'textarea', required: true, placeholder: 'return { result: input.x + 1 };' },
    ],
    'switch': [
      // switch config is an array — handled specially
    ],
    'for': [
      { key: 'in', label: 'Collection', type: 'text', required: true, placeholder: '${ .items }' },
      { key: 'each', label: 'Item Variable', type: 'text', required: true, placeholder: 'item' },
      { key: 'at', label: 'Index Variable', type: 'text', placeholder: 'idx' },
      { key: 'do', label: 'Steps', type: 'object' },
    ],
    'fork': [
      { key: 'branches', label: 'Branches', type: 'object' },
    ],
    'try': [
      { key: 'do', label: 'Try Steps', type: 'object' },
      { key: 'catch', label: 'Catch Steps', type: 'object' },
      { key: 'retry', label: 'Retry Count', type: 'number', placeholder: '0' },
    ],
    'raise': [
      { key: 'error', label: 'Error', type: 'object' },
    ],
    'wait': [
      { key: 'duration', label: 'Duration (ISO 8601)', type: 'text', required: true, placeholder: 'PT5S' },
    ],
    'state:get': [
      { key: 'key', label: 'Key', type: 'text', required: true },
    ],
    'state:set': [
      { key: 'key', label: 'Key', type: 'text', required: true },
      { key: 'value', label: 'Value', type: 'text', required: true },
      { key: 'ttl_secs', label: 'TTL (s)', type: 'number' },
    ],
    'input:file': [
      { key: 'path', label: 'File Path', type: 'text', required: true },
    ],
    'output:file': [
      { key: 'path', label: 'File Path', type: 'text', required: true },
      { key: 'content', label: 'Content', type: 'textarea' },
    ],
  };

  function getDefaultConfig(type: string): any {
    const schema = TYPE_SCHEMAS[type];
    if (!schema || schema.length === 0) return {};
    const config: Record<string, any> = {};
    for (const f of schema) {
      if (f.required) {
        if (f.type === 'text' || f.type === 'textarea') config[f.key] = '';
        else if (f.type === 'number') config[f.key] = 0;
        else if (f.type === 'select' && f.options?.length) config[f.key] = f.options[0];
        else if (f.type === 'object' && f.nested) {
          config[f.key] = {};
          for (const n of f.nested) {
            if (n.required) config[f.key][n.key] = '';
          }
        }
        else if (f.type === 'boolean') config[f.key] = false;
      }
    }
    return config;
  }

  function getSchema(type: string): FieldDef[] {
    return TYPE_SCHEMAS[type] ?? [];
  }
</script>

<div class="schema-form" bind:this={root}>
  {#if parseError}
    <div class="form-error">{parseError}</div>
  {:else if parsed}
    <!-- Template metadata -->
    <div class="form-section">
      <h4 class="form-section-title">Template</h4>
      <div class="form-grid">
        <label class="form-label">
          <span>name</span>
          <input type="text" value={docVal('name')} placeholder="my-flow" oninput={(e) => updateField(docPath('name'), e.currentTarget.value)} />
        </label>
        <label class="form-label">
          <span>namespace</span>
          <input type="text" value={docVal('namespace')} placeholder="default" oninput={(e) => updateField(docPath('namespace'), e.currentTarget.value)} />
        </label>
        <label class="form-label">
          <span>version</span>
          <input type="text" value={docVal('version')} placeholder="1.0.0" oninput={(e) => updateField(docPath('version'), e.currentTarget.value)} />
        </label>
        <label class="form-label full">
          <span>description</span>
          <input type="text" value={docVal('description')} oninput={(e) => updateField(docPath('description'), e.currentTarget.value)} />
        </label>
        {#if !cncf}
        <!-- `enabled` is registry state toggled elsewhere, not part of a CNCF
             document — offering it here would edit a key nothing reads. -->
        <label class="form-label">
          <span>enabled</span>
          <select value={String(parsed.enabled ?? true)} onchange={(e) => updateField(['enabled'], e.currentTarget.value === 'true')}>
            <option value="true">true</option>
            <option value="false">false</option>
          </select>
        </label>
        {/if}
      </div>
    </div>

    <!-- Steps -->
    <div class="form-section">
      <div class="form-section-header">
        <h4 class="form-section-title">Steps</h4>
        <span class="step-count">{parsed.steps?.length ?? 0}</span>
        <span style="flex:1;"></span>
        <button class="btn ghost sm" onclick={addStep}>+ Add step</button>
      </div>

      {#if parsed.steps?.length}
        <div class="steps-list">
          {#each parsed.steps as step, idx}
            {@const schema = getSchema(step.type)}
            <div class="step-card" class:expanded={expandedSteps[idx]} data-step={step.name}>
              <div class="step-header" role="button" tabindex="0" onclick={() => expandedSteps[idx] = !expandedSteps[idx]} onkeydown={(e) => { if (e.key === 'Enter') expandedSteps[idx] = !expandedSteps[idx]; }}>
                <svg class="step-chevron" class:open={expandedSteps[idx]} width="12" height="12" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="2"><path d="M6 4l4 4-4 4"/></svg>
                <span class="step-num">{idx + 1}</span>
                <span class="step-name">{step.name ?? 'unnamed'}</span>
                <span class="step-type-badge">{step.type ?? '?'}</span>
                {#if resourceOf(step)}<span class="step-resource">{resourceOf(step)}</span>{/if}
                {#if step.then}<span class="step-then">&#8594; {step.then}</span>{/if}
                <span class="step-actions">
                  <button class="step-btn" onclick={(e) => { e.stopPropagation(); moveStep(idx, -1); }} title="Move up" disabled={idx === 0}>&#9650;</button>
                  <button class="step-btn" onclick={(e) => { e.stopPropagation(); moveStep(idx, 1); }} title="Move down" disabled={idx === parsed.steps.length - 1}>&#9660;</button>
                  <button class="step-btn danger" onclick={(e) => { e.stopPropagation(); removeStep(idx); }} title="Remove">&times;</button>
                </span>
              </div>

              {#if expandedSteps[idx]}
                <div class="step-body">
                  <!-- Step-level fields -->
                  <div class="form-grid">
                    <label class="form-label">
                      <span>name</span>
                      <input type="text" value={step.name ?? ''} oninput={(e) => updateStepField(idx, 'name', e.currentTarget.value)} />
                    </label>
                    <label class="form-label">
                      <span>type</span>
                      <select value={step.type ?? ''} onchange={(e) => changeStepType(idx, e.currentTarget.value)}>
                        <option value="">-- select --</option>
                        {#each TASK_TYPES as t}<option value={t}>{t}</option>{/each}
                      </select>
                    </label>
                    {#if NEEDS_RESOURCE.has(step.type)}
                      <label class="form-label">
                        <span>resource</span>
                        <input type="text" value={resourceOf(step)} oninput={(e) => setResource(idx, e.currentTarget.value)} placeholder="namespace/name" />
                      </label>
                    {/if}
                    <label class="form-label">
                      <span>then</span>
                      <input type="text" value={step.then ?? ''} oninput={(e) => updateStepField(idx, 'then', e.currentTarget.value)} placeholder="next step or end" />
                    </label>
                    <label class="form-label">
                      <span>export.as</span>
                      <input type="text" value={step['export.as'] ?? ''} oninput={(e) => updateStepField(idx, 'export.as', e.currentTarget.value)} placeholder='e.g. "."' />
                    </label>
                  </div>

                  <!-- Type-specific config fields -->
                  {#if step.type && getManifest(step.type)?.with_schema}
                    <!-- Registry-driven config (schema from function manifest) -->
                    <div class="config-section">
                      <h5>config <span class="config-type-hint">{getManifest(step.type)?.name} v{getManifest(step.type)?.version}</span></h5>
                      <SchemaConfigForm
                        schema={getManifest(step.type)?.with_schema ?? {}}
                        value={step.config ?? {}}
                        path={['config']}
                        onchange={(path, val) => handleSchemaConfigChange(idx, path, val)}
                      />
                    </div>
                  {:else if schema.length > 0}
                    <div class="config-section">
                      <h5>config <span class="config-type-hint">{step.type}</span></h5>
                      <div class="config-fields">
                        {#each schema as field}
                          {#if field.type === 'text'}
                            <label class="form-label" class:full={field.key === 'command'}>
                              <span>{field.label}{#if field.required}<em>*</em>{/if}</span>
                              <input type="text" value={step.config?.[field.key] ?? ''} placeholder={field.placeholder ?? ''} oninput={(e) => updateStepConfig(idx, field.key, e.currentTarget.value)} />
                            </label>
                          {:else if field.type === 'number'}
                            <label class="form-label">
                              <span>{field.label}{#if field.required}<em>*</em>{/if}</span>
                              <input type="number" value={step.config?.[field.key] ?? ''} placeholder={field.placeholder ?? ''} oninput={(e) => updateStepConfig(idx, field.key, e.currentTarget.value ? Number(e.currentTarget.value) : undefined)} />
                            </label>
                          {:else if field.type === 'select'}
                            <label class="form-label">
                              <span>{field.label}{#if field.required}<em>*</em>{/if}</span>
                              <select value={step.config?.[field.key] ?? ''} onchange={(e) => updateStepConfig(idx, field.key, e.currentTarget.value)}>
                                <option value="">-- select --</option>
                                {#each field.options ?? [] as opt}<option value={opt}>{opt}</option>{/each}
                              </select>
                            </label>
                          {:else if field.type === 'boolean'}
                            <label class="form-label">
                              <span>{field.label}</span>
                              <select value={String(step.config?.[field.key] ?? false)} onchange={(e) => updateStepConfig(idx, field.key, e.currentTarget.value === 'true')}>
                                <option value="false">false</option>
                                <option value="true">true</option>
                              </select>
                            </label>
                          {:else if field.type === 'textarea'}
                            <label class="form-label full">
                              <span>{field.label}{#if field.required}<em>*</em>{/if}</span>
                              <textarea rows="3" placeholder={field.placeholder ?? ''} oninput={(e) => updateStepConfig(idx, field.key, e.currentTarget.value)}>{step.config?.[field.key] ?? ''}</textarea>
                            </label>
                          {:else if field.type === 'object' && field.nested}
                            <div class="nested-group full">
                              <span class="nested-label">{field.label}</span>
                              <div class="form-grid nested">
                                {#each field.nested as nf}
                                  <label class="form-label">
                                    <span>{nf.label}{#if nf.required}<em>*</em>{/if}</span>
                                    <input type="text" value={step.config?.[field.key]?.[nf.key] ?? ''} placeholder={nf.placeholder ?? ''} oninput={(e) => updateStepConfigNested(idx, [field.key, nf.key], e.currentTarget.value)} />
                                  </label>
                                {/each}
                              </div>
                            </div>
                          {:else if field.type === 'object'}
                            <label class="form-label full">
                              <span>{field.label}</span>
                              <textarea rows="3" placeholder="YAML object" oninput={(e) => {
                                try {
                                  const v = YAML.parse(e.currentTarget.value);
                                  if (v) updateStepConfig(idx, field.key, v);
                                } catch {}
                              }}>{step.config?.[field.key] ? YAML.stringify(step.config[field.key], { indent: 2 }).trim() : ''}</textarea>
                            </label>
                          {/if}
                        {/each}
                      </div>
                    </div>
                  {:else if step.type === 'set'}
                    <!-- set: freeform key-value editor -->
                    <div class="config-section">
                      <h5>config <span class="config-type-hint">key-value pairs</span></h5>
                      {#if step.config && typeof step.config === 'object'}
                        {#each Object.entries(step.config) as [key, val]}
                          <div class="kv-row">
                            <input class="kv-key" type="text" value={key} oninput={(e) => {
                              const newKey = e.currentTarget.value;
                              if (newKey && newKey !== key) {
                                step.config[newKey] = step.config[key];
                                delete step.config[key];
                                parsed = {...parsed};
                                emit();
                              }
                            }} />
                            <input class="kv-val" type="text" value={String(val ?? '')} oninput={(e) => updateStepConfig(idx, key, e.currentTarget.value)} />
                            <button class="step-btn danger" onclick={() => { delete step.config[key]; parsed = {...parsed}; emit(); }}>&times;</button>
                          </div>
                        {/each}
                      {/if}
                      <button class="btn ghost sm" style="margin-top: 6px;" onclick={() => {
                        if (!step.config) step.config = {};
                        step.config[`key_${Object.keys(step.config).length + 1}`] = '';
                        parsed = {...parsed};
                        emit();
                      }}>+ Add pair</button>
                    </div>
                  {:else}
                    <!-- Unknown type: show raw config as YAML textarea -->
                    <div class="config-section">
                      <h5>config <span class="config-type-hint">raw</span></h5>
                      <textarea class="raw-config" rows="5" oninput={(e) => {
                        try {
                          const v = YAML.parse(e.currentTarget.value);
                          if (v !== undefined) { step.config = v; parsed = {...parsed}; emit(); }
                        } catch {}
                      }}>{step.config ? YAML.stringify(step.config, { indent: 2 }).trim() : ''}</textarea>
                    </div>
                  {/if}
                </div>
              {/if}
            </div>
          {/each}
        </div>
      {:else}
        <div class="empty-steps">No steps defined. Click "+ Add step" to begin.</div>
      {/if}
    </div>
  {/if}
</div>

<style>
  .schema-form { height: 100%; overflow-y: auto; padding: 16px 20px; }

  .form-error { padding: 12px 16px; background: var(--err-bg, #2a1515); color: var(--err, #ff6b6b); border-radius: var(--r-sm); font-size: 12px; font-family: var(--font-mono); margin-bottom: 16px; }

  .form-section { margin-bottom: 24px; }
  .form-section-title { font-family: var(--font-serif); font-size: 14px; font-weight: 500; color: var(--text-2); margin-bottom: 10px; }
  .form-section-header { display: flex; align-items: center; gap: 8px; margin-bottom: 10px; }
  .form-section-header .form-section-title { margin-bottom: 0; }
  .step-count { font-family: var(--font-mono); font-size: 11px; color: var(--text-4); background: var(--bg-subtle); padding: 1px 6px; border-radius: 3px; }

  .form-grid { display: grid; grid-template-columns: 1fr 1fr; gap: 8px; }
  .form-grid.nested { margin-top: 4px; }
  .form-label { display: flex; flex-direction: column; gap: 3px; font-size: 11px; }
  .form-label.full { grid-column: 1 / -1; }
  .form-label span { color: var(--text-2); font-size: 11px; font-weight: 600; text-transform: uppercase; letter-spacing: 0.04em; }
  .form-label em { color: var(--err); font-style: normal; margin-left: 2px; }
  /* Same fields as the agent-template form (AgentTemplateEditor), not the
     roomier page-form defaults from app.css. */
  .form-label input, .form-label select, .form-label textarea {
    padding: 7px 10px; font-size: 13px; font-family: var(--font-mono);
  }
  .form-label input:focus, .form-label select:focus, .form-label textarea:focus { box-shadow: none; }

  /* Steps */
  .steps-list { display: flex; flex-direction: column; gap: 4px; }
  .step-card { background: var(--bg-surface); border: 1px solid var(--border); border-radius: var(--r-sm); overflow: hidden; }
  .step-card.expanded { border-color: var(--spec); }
  .step-header {
    display: flex; align-items: center; gap: 8px; padding: 8px 12px;
    width: 100%; cursor: pointer; font-size: 12px;
  }
  .step-header:hover { background: var(--bg-hover); }
  .step-chevron { color: var(--text-4); transition: transform 0.15s; flex-shrink: 0; }
  .step-chevron.open { transform: rotate(90deg); }
  .step-num { font-family: var(--font-mono); font-size: 10px; color: var(--text-4); background: var(--bg-subtle); width: 20px; height: 20px; border-radius: 3px; display: grid; place-items: center; flex-shrink: 0; }
  .step-name { font-weight: 550; color: var(--text-1); }
  .step-type-badge { font-family: var(--font-mono); font-size: 10px; color: var(--spec-ink); background: var(--spec-bg); padding: 1px 5px; border-radius: 3px; }
  .step-resource { font-family: var(--font-mono); font-size: 10px; color: var(--text-3); }
  .step-then { font-family: var(--font-mono); font-size: 10px; color: var(--text-4); margin-left: auto; }
  .step-actions { display: flex; gap: 2px; margin-left: auto; }
  .step-btn { width: 20px; height: 20px; border: 1px solid var(--border); border-radius: 3px; background: var(--bg-surface); color: var(--text-4); cursor: pointer; font-size: 10px; display: grid; place-items: center; }
  .step-btn:hover:not(:disabled) { background: var(--bg-hover); color: var(--text-1); }
  .step-btn:disabled { opacity: 0.3; cursor: default; }
  .step-btn.danger:hover:not(:disabled) { background: var(--err-bg); color: var(--err); border-color: var(--err); }

  .step-body { padding: 10px 12px 14px; border-top: 1px solid var(--divider); }

  /* Config */
  .config-section { margin-top: 12px; padding-top: 10px; border-top: 1px solid var(--divider); }
  .config-section h5 { font-size: 10px; font-weight: 600; color: var(--text-2); text-transform: uppercase; letter-spacing: 0.05em; margin-bottom: 8px; display: flex; align-items: center; gap: 6px; }
  .config-type-hint { font-size: 10px; color: var(--spec-ink); background: var(--spec-bg); padding: 0 4px; border-radius: 2px; text-transform: none; }
  .config-fields { display: grid; grid-template-columns: 1fr 1fr; gap: 8px; }

  .nested-group { display: flex; flex-direction: column; gap: 4px; }
  .nested-label { font-size: 10px; font-weight: 600; color: var(--text-2); text-transform: uppercase; letter-spacing: 0.05em; }

  /* KV editor for set */
  .kv-row { display: flex; gap: 4px; align-items: center; margin-bottom: 4px; }
  .kv-key { width: 140px; padding: 4px 8px; border: 1px solid var(--border); border-radius: var(--r-sm); background: var(--bg-subtle); color: var(--text-2); font-family: var(--font-mono); font-size: 11px; outline: none; }
  .kv-val { flex: 1; padding: 4px 8px; border: 1px solid var(--border); border-radius: var(--r-sm); background: var(--bg-surface); color: var(--text-1); font-family: var(--font-mono); font-size: 11px; outline: none; }
  .kv-key:focus, .kv-val:focus { border-color: var(--spec); }

  .raw-config {
    width: 100%; padding: 8px 10px; border: 1px solid var(--border); border-radius: var(--r-sm);
    background: var(--bg-surface); color: var(--text-1); font-family: var(--font-mono); font-size: 11px;
    outline: none; resize: vertical;
  }
  .raw-config:focus { border-color: var(--spec); }

  .empty-steps { padding: 20px; text-align: center; color: var(--text-4); font-size: 13px; background: var(--bg-surface); border: 1px solid var(--border); border-radius: var(--r-sm); }
</style>
