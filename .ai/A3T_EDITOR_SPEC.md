# A3T Editor — Spec & Implementation Guide

**Audience:** Claude Code  
**Status:** Authoritative design spec. Deviations require human approval.  
**Stack:** Svelte 5 (runes), `js-yaml`, `ajv@8` (2020-12 build), TypeScript  
**Design system:** Playfair Display + Inter + IBM Plex Mono, ivory base `#faf9f6`, BSL 2.0

---

## 1. Mental model

The editor has three surfaces, one linear authoring flow:

```
Template designer  →  Agent config  →  CRD / deploy
```

Each surface maps to a distinct concern:

| Surface | What it edits | Source of truth |
|---|---|---|
| Template designer | SWF workflow structure + slot contracts | `use.extensions`, `input/output schema`, `document.metadata` |
| Agent config | Slot wiring + resource bindings + config values | Agent CR `spec.*` |
| CRD / deploy | Generated manifest preview + deploy action | `kind: Flow` + `kind: Agent` CRs |

**Key invariant:** The editor never hardcodes knowledge of any specific callable, template, or resource type. Everything is driven by schemas fetched from the registry at runtime.

---

## 2. SWF official schema

The SWF 1.0.3 schema is published and maintained by the spec authors. Do NOT produce or store a copy.

```
URL:   https://serverlessworkflow.io/schemas/1.0.3/workflow.yaml
$id:   https://serverlessworkflow.io/schemas/1.0.3/workflow.yaml
draft: JSON Schema 2020-12
size:  ~1956 lines, 75KB, 47 $defs
```

**Critical facts confirmed by testing:**

- `call: a3t:agent`, `call: clawdb`, `call: llm`, `call: state:get` etc. all validate correctly.  
  The schema has a `CallFunction` branch (branch 6 of `callTask.oneOf`) with  
  `call: { type: string, not: { enum: [asyncapi, grpc, http, openapi, a2a, mcp] } }`  
  and `with: { additionalProperties: true }`. No workaround needed.

- The schema uses `unevaluatedProperties: false` extensively (76 occurrences).  
  **This requires `ajv/dist/2020` — the default `ajv` draft-07 build will not work.**

- Sub-schema validation (e.g. validating just an extension or input block) requires  
  adding the full schema first via `ajv.addSchema(schema)` then compiling a `{ $ref: '...' }`.  
  Direct `ajv.compile(schema.$defs.extension)` throws `MissingRefError` because `$defs`  
  entries reference each other.

- Error `instancePath` format: `/document/namespace`, `/do/0/taskName/call` — JSON Pointer  
  strings, directly usable as UI field addresses after stripping the leading `/`.

---

## 3. Registry API — what the editor calls

All editor behaviour derives from these endpoints. No other hardcoding.

```
GET /registry/functions                 → list all registered callables
GET /registry/functions/:name           → FunctionManifest
GET /registry/templates                 → list all AgentTemplates
GET /registry/templates/:ns/:name       → TemplateManifest
GET /registry/resources?type=<t>        → list resources by type
```

The SWF schema is fetched directly from `serverlessworkflow.io` — not proxied through FlowR.

### 3.1 FunctionManifest shape

```typescript
interface FunctionManifest {
  name: string                    // e.g. "a3t:agent", "http", "grpc", "clawdb"
  version: string
  description?: string
  // JSON Schema for `with:` params — compiled against the registered SWF schema
  // so $refs inside with_schema can reference SWF $defs if needed
  with_schema?: JSONSchema
  // Per-key schema for document.metadata entries this callable expects
  metadata_schema?: Record<string, MetadataFieldDef>
}

interface MetadataFieldDef {
  type: 'string' | 'integer' | 'boolean' | 'resource-ref'
  required: boolean
  description?: string
  enum?: string[]
  resource_type?: string   // populated when type === 'resource-ref'
  pattern?: string
}
```

### 3.2 TemplateManifest shape

```typescript
interface TemplateManifest {
  name: string
  namespace: string
  version: string
  engine: { call: string; version: string }  // e.g. { call: 'a3t:agent', version: '2.0.0' }
  config_schema: JSONSchema
  resource_slots: Record<string, ResourceSlotDef>
  extension_slots: Record<string, ExtensionSlot>
  integration_slots: Record<string, IntegrationSlot>
}

interface ResourceSlotDef {
  required: boolean
  resource_type: string
  description?: string
}

interface ExtensionSlot {
  required: boolean
  description?: string
  input_schema: JSONSchema
  output_schema: JSONSchema
  extends: string    // one of the SWF extension.extend enum values
  when?: string
}

interface IntegrationSlot {
  required: boolean
  description?: string
  per_entry: { input_schema: JSONSchema; output_schema: JSONSchema }
}
```

---

## 4. Data layer — TypeScript types

```typescript
// Loaded once per editor session
interface EditorRegistry {
  swfSchema: JSONSchema          // fetched from serverlessworkflow.io
  functions: Map<string, FunctionManifest>
  templates: Map<string, TemplateManifest>
  resources: Map<string, Resource[]>    // keyed by resource_type
}

// Single source of truth for all three surfaces
interface EditorDocument {
  workflow: SWFWorkflow
  agentConfig: AgentConfig | null
  templateRef: string | null                  // "namespace/name@version"
  writerHash: string | null
  // Derived — never persisted
  templateManifest: TemplateManifest | null
  functionManifest: FunctionManifest | null
  validationErrors: ValidationError[]
  slotWiringStatus: Map<string, WiringStatus>
}

interface AgentConfig {
  config: Record<string, unknown>
  resources: Record<string, string>           // slotName → "ns/name"
  extensions: Record<string, FlowRef | null>  // slotName → wired flow or null
  policyPreset: string
}

interface FlowRef {
  namespace: string
  name: string
  version: string
}

interface WiringStatus {
  slotName: string
  wired: boolean
  flowRef?: FlowRef
  schemaMatch: 'ok' | 'mismatch' | 'unknown'
  error?: string
}

interface ValidationError {
  path: string         // JSON Pointer e.g. "/document/namespace", "/do/0/callAgent/call"
  message: string
  severity: 'error' | 'warning'
  source: 'swf-schema' | 'function-manifest' | 'template-manifest' | 'slot-contract'
}
```

---

## 5. AJV setup — `src/lib/schema-utils.ts`

This is the only place in the codebase that touches ajv. All other modules call these functions.

```typescript
import Ajv2020 from 'ajv/dist/2020'
import addFormats from 'ajv-formats'
import jsYaml from 'js-yaml'

// SWF schema $id — used for $ref compilation
const SWF_SCHEMA_ID = 'https://serverlessworkflow.io/schemas/1.0.3/workflow.yaml'
const SWF_SCHEMA_URL = SWF_SCHEMA_ID

let _ajv: InstanceType<typeof Ajv2020> | null = null
let _swfSchema: unknown | null = null

// Call once at session start. Fetches schema from serverlessworkflow.io,
// registers it, and returns the configured ajv instance.
export async function initAjv(): Promise<InstanceType<typeof Ajv2020>> {
  if (_ajv) return _ajv

  const res = await fetch(SWF_SCHEMA_URL)
  if (!res.ok) throw new Error(`Failed to fetch SWF schema: ${res.status}`)
  const yamlText = await res.text()
  _swfSchema = jsYaml.load(yamlText)

  _ajv = new Ajv2020({ strict: false, allErrors: true })
  addFormats(_ajv)
  // MUST addSchema before any compile() — $defs reference each other
  _ajv.addSchema(_swfSchema)

  return _ajv
}

export function getSwfSchema(): unknown {
  if (!_swfSchema) throw new Error('AJV not initialised — call initAjv() first')
  return _swfSchema
}

// Validate a complete SWF workflow document
export function validateWorkflow(doc: unknown): ValidationError[] {
  const ajv = getAjv()
  const validate = ajv.getSchema(SWF_SCHEMA_ID)!
  const valid = validate(doc)
  if (valid) return []
  return mapErrors(validate.errors!, 'swf-schema')
}

// Validate `with:` params against a function manifest's with_schema
export function validateWithParams(
  withValue: unknown,
  withSchema: JSONSchema
): ValidationError[] {
  const ajv = getAjv()
  const validate = ajv.compile(withSchema)
  const valid = validate(withValue)
  if (valid) return []
  return mapErrors(validate.errors!, 'function-manifest')
}

// Validate document.metadata keys against a function manifest's metadata_schema.
// The SWF schema allows any metadata (additionalProperties: true) so this is
// purely manifest-driven, not schema-driven.
export function validateMetadata(
  metadata: Record<string, unknown>,
  metadataSchema: FunctionManifest['metadata_schema']
): ValidationError[] {
  if (!metadataSchema) return []
  const errors: ValidationError[] = []
  for (const [key, def] of Object.entries(metadataSchema)) {
    if (def.required && metadata[key] == null) {
      errors.push({ path: `/document/metadata/${key}`, message: `required field missing`, severity: 'error', source: 'function-manifest' })
      continue
    }
    if (metadata[key] == null) continue
    if (def.type === 'resource-ref') {
      if (typeof metadata[key] !== 'string' || !metadata[key].includes('/')) {
        errors.push({ path: `/document/metadata/${key}`, message: `must be a resource ref in format "namespace/name"`, severity: 'error', source: 'function-manifest' })
      }
    } else if (def.enum && !def.enum.includes(metadata[key] as string)) {
      errors.push({ path: `/document/metadata/${key}`, message: `must be one of: ${def.enum.join(', ')}`, severity: 'error', source: 'function-manifest' })
    } else if (def.pattern && !new RegExp(def.pattern).test(metadata[key] as string)) {
      errors.push({ path: `/document/metadata/${key}`, message: `must match pattern ${def.pattern}`, severity: 'error', source: 'function-manifest' })
    }
  }
  return errors
}

// Validate config values against a template's config_schema
export function validateConfig(
  config: unknown,
  configSchema: JSONSchema
): ValidationError[] {
  const ajv = getAjv()
  const validate = ajv.compile(configSchema)
  const valid = validate(config)
  if (valid) return []
  return mapErrors(validate.errors!, 'template-manifest')
}

// Validate a slot workflow's output schema satisfies the slot contract.
// wiredOutputSchema: the output.schema.document from the wired workflow
// slotOutputSchema:  the output_schema from the ExtensionSlot definition
export function validateSlotOutputContract(
  wiredOutputSchema: JSONSchema,
  slotOutputSchema: JSONSchema
): ValidationError[] {
  // Check all required fields in slot contract are present in the wired schema
  const errors: ValidationError[] = []
  const required = slotOutputSchema.required ?? []
  const wiredProps = wiredOutputSchema.properties ?? {}
  for (const field of required) {
    if (!wiredProps[field]) {
      errors.push({
        path: `/output/schema/document/properties/${field}`,
        message: `slot contract requires output field "${field}" — not declared in wired workflow output schema`,
        severity: 'error',
        source: 'slot-contract'
      })
    }
  }
  return errors
}

// Compile a $defs sub-schema by ref — use for validating extension blocks etc.
// Must call initAjv() before using this.
export function compileRef(defPath: string): ReturnType<typeof _ajv.compile> {
  const ajv = getAjv()
  return ajv.compile({ $ref: `${SWF_SCHEMA_ID}#/${defPath}` })
}

// Internal helpers
function getAjv(): InstanceType<typeof Ajv2020> {
  if (!_ajv) throw new Error('AJV not initialised — call initAjv() first')
  return _ajv
}

function mapErrors(
  errors: InstanceType<typeof Ajv2020>['errors'],
  source: ValidationError['source']
): ValidationError[] {
  return (errors ?? []).map(e => ({
    path: e.instancePath || '/',
    message: e.message ?? 'validation error',
    severity: 'error' as const,
    source
  }))
}
```

**Package dependencies:**
```json
{
  "ajv": "^8.17.1",
  "ajv-formats": "^3.0.1",
  "js-yaml": "^4.1.0"
}
```

**Import path is `ajv/dist/2020` — not `ajv`.** The default `ajv` export is draft-07 and silently ignores `unevaluatedProperties`, producing false positives. This will cause incorrect validation without any error message.

---

## 6. Validation pipeline — `src/lib/validate.ts`

Four passes run in sequence. Results merged into `EditorDocument.validationErrors`.

```typescript
import {
  validateWorkflow, validateWithParams, validateMetadata,
  validateConfig, validateSlotOutputContract, compileRef
} from './schema-utils'

export async function runValidation(
  doc: EditorDocument,
  registry: EditorRegistry
): Promise<ValidationError[]> {
  const errors: ValidationError[] = []

  // Pass 1 — SWF structural validation
  // Uses the compiled schema from serverlessworkflow.io
  // Catches: missing required fields, wrong task types, bad patterns on
  // namespace/name/version (semver regex), invalid extension.extend values
  errors.push(...validateWorkflow(doc.workflow))

  // Pass 2 — function manifest validation
  // Only runs if the primary call: task has a registered manifest
  const primaryCall = findPrimaryCall(doc.workflow)
  if (primaryCall && doc.functionManifest) {
    // Validate with: params
    if (doc.functionManifest.with_schema && primaryCall.with) {
      errors.push(...validateWithParams(primaryCall.with, doc.functionManifest.with_schema))
    }
    // Validate document.metadata keys
    if (doc.functionManifest.metadata_schema) {
      errors.push(...validateMetadata(
        doc.workflow.document?.metadata ?? {},
        doc.functionManifest.metadata_schema
      ))
    }
  }

  // Pass 3 — template manifest validation
  // Validates config values and required resource slots
  if (doc.templateManifest && doc.agentConfig) {
    errors.push(...validateConfig(doc.agentConfig.config, doc.templateManifest.config_schema))
    for (const [slot, def] of Object.entries(doc.templateManifest.resource_slots)) {
      if (def.required && !doc.agentConfig.resources[slot]) {
        errors.push({
          path: `/spec/resources/${slot}`,
          message: `required resource slot "${slot}" (type: ${def.resource_type}) not bound`,
          severity: 'error',
          source: 'template-manifest'
        })
      }
    }
  }

  // Pass 4 — slot contract validation (async — fetches wired workflow schemas)
  // Runs with 500ms debounce in the UI component
  if (doc.templateManifest && doc.agentConfig) {
    for (const [slotName, slot] of Object.entries(doc.templateManifest.extension_slots)) {
      const wiring = doc.agentConfig.extensions[slotName]
      if (!wiring) continue
      try {
        const wiredWorkflow = await registry.getWorkflow(wiring)
        if (!wiredWorkflow?.output?.schema?.document) {
          errors.push({
            path: `/extensions/${slotName}`,
            message: `wired workflow has no output.schema — cannot verify slot contract`,
            severity: 'warning',
            source: 'slot-contract'
          })
          continue
        }
        errors.push(...validateSlotOutputContract(
          wiredWorkflow.output.schema.document,
          slot.output_schema
        ))
      } catch {
        errors.push({
          path: `/extensions/${slotName}`,
          message: `could not fetch wired workflow to verify slot contract`,
          severity: 'warning',
          source: 'slot-contract'
        })
      }
    }
  }

  return errors
}

// Find the primary callable task — first task in do: that has a `call:` key
function findPrimaryCall(workflow: SWFWorkflow): { call: string; with?: unknown } | null {
  for (const taskItem of workflow.do ?? []) {
    const [task] = Object.values(taskItem)
    if (task && typeof task === 'object' && 'call' in task) {
      return task as { call: string; with?: unknown }
    }
  }
  return null
}
```

**Validation properties:**
- Non-blocking — errors display inline, never modal
- Incremental — passes 1-3 re-run on any edit; pass 4 re-runs only on slot wiring change
- Pass 4 debounced 500ms — shows `unknown` status while fetching, `ok`/`mismatch` once resolved

---

## 7. Registry client — `src/lib/registry.ts`

```typescript
import jsYaml from 'js-yaml'

export class RegistryClient {
  private readonly base: string
  private readonly cache = new Map<string, unknown>()

  constructor(base: string) {
    this.base = base.replace(/\/$/, '')
  }

  async getFunction(name: string): Promise<FunctionManifest | null> {
    return this.cached(`/registry/functions/${encodeURIComponent(name)}`)
  }

  async getTemplate(ref: string): Promise<TemplateManifest | null> {
    // ref formats: "namespace/name@version" or "namespace/name" (defaults to latest)
    const atIdx = ref.lastIndexOf('@')
    const nsname = atIdx > -1 ? ref.slice(0, atIdx) : ref
    const version = atIdx > -1 ? ref.slice(atIdx + 1) : 'latest'
    const [ns, name] = nsname.split('/')
    return this.cached(`/registry/templates/${ns}/${name}?version=${version}`)
  }

  async getResources(type: string): Promise<Resource[]> {
    return this.cached(`/registry/resources?type=${encodeURIComponent(type)}`) ?? []
  }

  async getWorkflow(ref: FlowRef): Promise<SWFWorkflow | null> {
    return this.cached(
      `/flows/${ref.namespace}/${ref.name}?version=${encodeURIComponent(ref.version)}`
    )
  }

  private async cached<T>(path: string): Promise<T | null> {
    if (this.cache.has(path)) return this.cache.get(path) as T
    try {
      const res = await fetch(this.base + path)
      if (!res.ok) return null
      const ct = res.headers.get('content-type') ?? ''
      const data = ct.includes('yaml')
        ? jsYaml.load(await res.text())
        : await res.json()
      this.cache.set(path, data)
      return data as T
    } catch {
      return null
    }
  }

  invalidate(path?: string) {
    if (path) this.cache.delete(path)
    else this.cache.clear()
  }
}
```

---

## 8. Bootstrap — `EditorShell.svelte`

```typescript
// EditorShell.svelte <script>
import { initAjv } from '$lib/schema-utils'
import { RegistryClient } from '$lib/registry'
import { runValidation } from '$lib/validate'

const registry = new RegistryClient(import.meta.env.VITE_FLOWR_API)
let editorDoc: EditorDocument | null = $state(null)
let loading = $state(true)
let loadError: string | null = $state(null)

$effect(() => {
  bootstrap()
})

async function bootstrap() {
  try {
    loading = true

    // Step 1 — init ajv with the official SWF 2020-12 schema
    // Fetches https://serverlessworkflow.io/schemas/1.0.3/workflow.yaml
    // and registers it so all $ref compilations resolve correctly
    await initAjv()

    // Step 2 — parse incoming YAML
    const raw = params.yaml ?? BLANK_WORKFLOW_YAML
    const workflow = jsYaml.load(raw) as SWFWorkflow

    // Step 3 — resolve manifests
    const templateRef = workflow.document?.metadata?.['a3t.io/template'] as string | undefined
    const primaryCall = findPrimaryCall(workflow)

    const [templateManifest, functionManifest] = await Promise.all([
      templateRef ? registry.getTemplate(templateRef) : Promise.resolve(null),
      primaryCall ? registry.getFunction(primaryCall.call) : Promise.resolve(null),
    ])

    // Step 4 — pre-fetch resource lists for all slots in the template
    if (templateManifest) {
      const types = [...new Set(
        Object.values(templateManifest.resource_slots).map(s => s.resource_type)
      )]
      await Promise.all(types.map(t => registry.getResources(t)))
    }

    // Step 5 — build EditorDocument
    const doc: EditorDocument = {
      workflow,
      agentConfig: null,
      templateRef: templateRef ?? null,
      writerHash: workflow.document?.metadata?.['a3t.io/writr-hash'] as string ?? null,
      templateManifest,
      functionManifest,
      validationErrors: [],
      slotWiringStatus: new Map(),
    }

    // Step 6 — initial validation
    doc.validationErrors = await runValidation(doc, registry)

    editorDoc = doc
  } catch (err) {
    loadError = (err as Error).message
  } finally {
    loading = false
  }
}
```

---

## 9. Component tree

```
EditorShell                         ← bootstrap: initAjv() + registry load
  ├── EditorTopbar                  ← tab switcher (Template / Agent / CRD)
  │
  ├── TemplateDesigner              ← surface 1
  │   ├── TemplateSectionNav        ← extensions / integrations / config / resources / events / errors
  │   ├── ExtensionSlotsPanel       ← driven by TemplateManifest.extension_slots
  │   │   └── SlotCard[]            ← one per slot, expandable, shows I/O schema
  │   ├── IntegrationSlotsPanel
  │   ├── ConfigSchemaPanel         ← driven by TemplateManifest.config_schema
  │   ├── ResourceSlotsPanel        ← driven by TemplateManifest.resource_slots
  │   ├── EventsPanel               ← from workflow use.extensions metadata
  │   ├── ErrorsPanel               ← from use.errors
  │   └── TemplateMetaAside         ← version, status, slot count
  │
  ├── AgentConfig                   ← surface 2
  │   ├── AgentSectionNav
  │   ├── ConfigForm                ← schema-driven from TemplateManifest.config_schema
  │   ├── ResourceBindings          ← one picker per TemplateManifest.resource_slots entry
  │   ├── ExtensionWiringPanel      ← one WireCard per extension_slot
  │   │   └── WireCard[]            ← FlowRefInput + live slot contract type-check
  │   └── AgentMetaAside            ← wiring status, validation summary
  │
  └── CRDPreview                    ← surface 3
      ├── CRDSectionNav             ← Agent CR / PolicyPreset ref / SWF workflow
      ├── YAMLPreview               ← read-only, syntax highlighted
      ├── ValidationChecklist       ← 4 passes: schema / function / template / slots
      └── DeployBar                 ← dry-run + deploy buttons
```

---

## 10. ConfigForm — schema-driven field rendering

Receives `TemplateManifest.config_schema` (JSON Schema). Renders fields from it. No hardcoded field names anywhere. Recursive for nested objects.

```typescript
// ConfigForm.svelte
let { schema, value = $bindable(), errors = [] }: {
  schema: JSONSchema
  value: Record<string, unknown>
  errors: ValidationError[]
} = $props()

let fields = $derived(
  Object.entries(schema.properties ?? {}).map(([key, def]: [string, JSONSchema]) => ({
    key,
    required: schema.required?.includes(key) ?? false,
    type: def.type as string,
    default: def.default,
    enum: def.enum as string[] | undefined,
    description: def.description as string | undefined,
    properties: def.properties,  // for nested objects → recursive ConfigForm
  }))
)

// Field type → widget
// 'string' + enum    → <select>
// 'string'           → <input type="text">
// 'integer'/'number' → <input type="number">
// 'boolean'          → <input type="checkbox">
// 'object'           → <ConfigForm schema={def} bind:value={value[key]} />
// 'array'            → tag input
```

Error display: each field checks `errors.filter(e => e.path.endsWith(`/${key}`))`  
Shows red border + message below if any errors. Never a modal.

---

## 11. WireCard — slot contract type-check

```typescript
// WireCard.svelte
let { slotName, slot, value = $bindable(), errors = [] }: {
  slotName: string
  slot: ExtensionSlot
  value: FlowRef | null
  errors: ValidationError[]
} = $props()

let status: 'unknown' | 'ok' | 'mismatch' | 'unwired' = $state('unwired')
let checkErrors: ValidationError[] = $state([])

// Debounced type-check on wiring change
let debounceTimer: ReturnType<typeof setTimeout>
$effect(() => {
  if (!value) { status = 'unwired'; return }
  status = 'unknown'
  clearTimeout(debounceTimer)
  debounceTimer = setTimeout(async () => {
    const wired = await registry.getWorkflow(value)
    const contractErrors = wired
      ? validateSlotOutputContract(
          wired.output?.schema?.document ?? {},
          slot.output_schema
        )
      : [{ path: `/${slotName}`, message: 'workflow not found', severity: 'error' as const, source: 'slot-contract' as const }]
    checkErrors = contractErrors
    status = contractErrors.length === 0 ? 'ok' : 'mismatch'
  }, 500)
})
```

---

## 12. Serialiser — `src/lib/serialise.ts`

One-way only: `EditorDocument → YAML string`. Never mutates the YAML string directly.

```typescript
import jsYaml from 'js-yaml'

export function serialise(doc: EditorDocument): string {
  const wf = structuredClone(doc.workflow)

  // 1. Write slot wiring into use.extensions
  if (doc.templateManifest && doc.agentConfig?.extensions) {
    wf.use = wf.use ?? {}
    wf.use.extensions = buildExtensions(
      doc.templateManifest.extension_slots,
      doc.agentConfig.extensions
    )
  }

  // 2. Write resource refs into document.metadata (tooling layer)
  if (doc.agentConfig?.resources) {
    wf.document.metadata = wf.document.metadata ?? {}
    for (const [slot, ref] of Object.entries(doc.agentConfig.resources)) {
      wf.document.metadata[`a3t.io/resources.${slot}`] = ref
    }
  }

  // 3. Write template ref + writr hash
  wf.document.metadata = wf.document.metadata ?? {}
  if (doc.templateRef) wf.document.metadata['a3t.io/template'] = doc.templateRef
  wf.document.metadata['a3t.io/writr-hash'] = computeHash(doc)

  return jsYaml.dump(wf, { indent: 2, lineWidth: 120 })
}

function buildExtensions(
  slots: Record<string, ExtensionSlot>,
  wiring: Record<string, FlowRef | null>
): SWFExtensionItem[] {
  return Object.entries(slots).map(([slotName, slot]) => ({
    [slotName]: {
      extend: slot.extends,
      ...(slot.when ? { when: slot.when } : {}),
      before: [{
        run_slot: {
          if: `${ $context._config.slots.${slotName} != null }`,
          run: {
            workflow: {
              namespace: `${ $context._config.slots.${slotName}.namespace }`,
              name:      `${ $context._config.slots.${slotName}.name }`,
              version:   `${ $context._config.slots.${slotName}.version }`,
              input:     buildSlotInput(slot.input_schema),
            }
          },
          output: {
            schema: { format: 'json', document: slot.output_schema }
          },
          export: { as: buildSlotExport(slotName, slot.output_schema) }
        }
      }]
    }
  }))
}

export function generateAgentCR(doc: EditorDocument): string {
  const cr = {
    apiVersion: 'a3t.io/v1',
    kind: 'Agent',
    metadata: {
      name: doc.workflow.document.name,
      namespace: doc.workflow.document.namespace,
      annotations: {
        'a3t.io/managed-by': 'writr',
        'a3t.io/template': doc.templateRef ?? '',
        'a3t.io/writr-hash': doc.writerHash ?? '',
      }
    },
    spec: {
      template: {
        ref: `${doc.templateManifest?.namespace}/${doc.templateManifest?.name}`,
        version: doc.templateManifest?.version,
      },
      resources: doc.agentConfig?.resources ?? {},
      config: doc.agentConfig?.config ?? {},
      extensions: doc.agentConfig?.extensions ?? {},
      policy: {
        preset: doc.agentConfig?.policyPreset ?? 'a3t-system/standard',
        runtime: { mode: 'sandboxed' }
      }
    }
  }
  return jsYaml.dump(cr, { indent: 2 })
}
```

---

## 13. File structure

```
src/
  lib/
    schema-utils.ts       ← ONLY place ajv is imported. initAjv(), validateWorkflow(),
                            validateWithParams(), validateMetadata(), validateConfig(),
                            validateSlotOutputContract(), compileRef()
    registry.ts           ← RegistryClient (session-cached fetch, yaml+json responses)
    types.ts              ← all TypeScript interfaces (no runtime code)
    validate.ts           ← runValidation() — orchestrates 4 passes
    serialise.ts          ← serialise(), generateAgentCR() (pure functions)
    yaml-utils.ts         ← js-yaml wrapper, findPrimaryCall(), JSON Pointer helpers

  components/
    editor/
      EditorShell.svelte          ← bootstrap: initAjv() + registry + document load
      EditorTopbar.svelte

      template/
        TemplateDesigner.svelte
        TemplateSectionNav.svelte
        ExtensionSlotsPanel.svelte
        SlotCard.svelte             ← expandable, shows input→output schema
        IntegrationSlotsPanel.svelte
        ConfigSchemaPanel.svelte
        ResourceSlotsPanel.svelte
        EventsPanel.svelte
        ErrorsPanel.svelte
        TemplateMetaAside.svelte

      agent/
        AgentConfig.svelte
        AgentSectionNav.svelte
        ConfigForm.svelte           ← schema-driven, recursive
        ResourceBindings.svelte     ← type-driven pickers
        ExtensionWiringPanel.svelte
        WireCard.svelte             ← FlowRefInput + debounced slot contract check
        AgentMetaAside.svelte

      crd/
        CRDPreview.svelte
        CRDSectionNav.svelte
        YAMLPreview.svelte          ← always regenerated from EditorDocument
        ValidationChecklist.svelte  ← shows all 4 pass results
        DeployBar.svelte

    shared/
      SchemaYamlEditor.svelte       ← existing shared YAML editor
      ResourcePicker.svelte         ← used in ResourceBindings + MetadataPanel
      FlowRefInput.svelte           ← namespace/name/version with search
      SchemaViewer.svelte           ← renders JSONSchema as human-readable contract
      ValidationBadge.svelte        ← ok / warning / error pill
      SlotIOSchema.svelte           ← input → output schema side-by-side display
      MetadataPanel.svelte          ← renders document.metadata fields from FunctionManifest

  routes/
    templates/[ns]/[name]/+page.svelte
    agents/[ns]/[name]/+page.svelte
    agents/[ns]/[name]/deploy/+page.svelte
```

---

## 14. Key implementation rules for Claude Code

1. **`import Ajv2020 from 'ajv/dist/2020'` — never `import Ajv from 'ajv'`.** The default export is draft-07 and silently fails on `unevaluatedProperties`.

2. **Call `ajv.addSchema(schema)` before any `ajv.compile()`.** The SWF schema's `$defs` reference each other — compiling a sub-schema without the parent schema registered throws `MissingRefError`.

3. **Use `ajv.getSchema(SWF_SCHEMA_ID)` for full workflow validation**, not `ajv.compile(schema)` on every call. The schema is compiled once on registration.

4. **All ajv usage is in `schema-utils.ts` only.** No other file imports ajv directly.

5. **No hardcoded slot names, field names, or callable names in components.** Everything derives from manifests fetched at runtime.

6. **`EditorDocument` is the single source of truth.** Surfaces read from and write to it. No surface reads another surface's state directly.

7. **Serialiser is pure and one-way.** `EditorDocument → YAML`. Never mutate the YAML string directly.

8. **`ConfigForm` is recursive.** When a schema property is `type: object` with `properties`, render a nested `ConfigForm`. `ajv.compile()` validates the whole tree.

9. **Slot type-checking is async and non-blocking.** `$effect` + 500ms debounce in `WireCard`. Show `unknown` while pending, `ok`/`mismatch` on resolution.

10. **Registry responses are session-cached.** `RegistryClient.cached()` is the single fetch gate. Never call `fetch()` directly in components.

11. **`YAMLPreview` is always regenerated.** Never cache the generated YAML string — recompute from `EditorDocument` on every render of the CRD surface.

12. **Use Svelte 5 runes throughout.** `$state`, `$derived`, `$effect`, `$props()`, `$bindable()`. No legacy `export let` or `$:` reactive statements.

13. **Error paths are JSON Pointers** (`/document/namespace`, `/do/0/callAgent/with`). Map to field addresses by splitting on `/` and walking the document tree.

---

## 15. Deploy action

```typescript
// DeployBar.svelte
async function deploy(doc: EditorDocument): Promise<void> {
  // Block on any error-severity validation failures
  if (doc.validationErrors.some(e => e.severity === 'error')) {
    throw new Error('resolve validation errors before deploying')
  }

  // 1. Register / update the SWF workflow in FlowR
  const workflowYaml = serialise(doc)
  await fetch(`${FLOWR_API}/flows`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/yaml' },
    body: workflowYaml,
  })

  // 2. Apply Agent CR — controller generates Flow CR automatically
  const agentCR = generateAgentCR(doc)
  await fetch(`${K8S_API}/apis/a3t.io/v1/namespaces/${doc.workflow.document.namespace}/agents`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/yaml' },
    body: agentCR,
  })
}
```

The controller watches `kind: Agent` and generates `kind: Flow`. The editor submits only the SWF workflow to FlowR and the Agent CR to Kubernetes — no direct `kind: Flow` submission.
