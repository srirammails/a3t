/**
 * A3T Editor types — single source of truth for all editor interfaces.
 * Matches the spec in a3t/docs/A3T_EDITOR_SPEC.md
 */

// ── JSON Schema (simplified for our usage) ──
export type JSONSchema = Record<string, unknown>

// ── Registry types ──

export interface FunctionManifest {
  name: string
  version: string
  description?: string
  with_schema?: JSONSchema
  metadata_schema?: Record<string, MetadataFieldDef>
}

export interface MetadataFieldDef {
  type: 'string' | 'integer' | 'boolean' | 'resource-ref'
  required: boolean
  description?: string
  enum?: string[]
  resource_type?: string
  pattern?: string
}

export interface TemplateManifest {
  name: string
  namespace: string
  version: string
  engine: { call: string; version: string }
  config_schema: JSONSchema
  resource_slots: Record<string, ResourceSlotDef>
  extension_slots: Record<string, ExtensionSlot>
  integration_slots: Record<string, IntegrationSlot>
}

export interface ResourceSlotDef {
  required: boolean
  resource_type: string
  description?: string
}

export interface ExtensionSlot {
  required: boolean
  description?: string
  input_schema: JSONSchema
  output_schema: JSONSchema
  extends: string
  when?: string
}

export interface IntegrationSlot {
  required: boolean
  description?: string
  per_entry: { input_schema: JSONSchema; output_schema: JSONSchema }
}

export interface Resource {
  name: string
  namespace: string
  type: string
  config: Record<string, unknown>
}

// ── Editor document ──

export interface EditorDocument {
  workflow: SWFWorkflow
  agentConfig: AgentConfig | null
  templateRef: string | null
  writerHash: string | null
  // Derived — never persisted
  templateManifest: TemplateManifest | null
  functionManifest: FunctionManifest | null
  validationErrors: ValidationError[]
  slotWiringStatus: Map<string, WiringStatus>
}

export interface SWFWorkflow {
  document: {
    dsl: string
    namespace: string
    name: string
    version: string
    metadata?: Record<string, unknown>
  }
  use?: {
    extensions?: unknown[]
    functions?: Record<string, unknown>
    secrets?: string[]
    errors?: Record<string, unknown>
    retries?: Record<string, unknown>
    timeouts?: Record<string, unknown>
    authentications?: Record<string, unknown>
    catalogs?: Record<string, unknown>
  }
  do?: unknown[]
  input?: { from?: string; schema?: JSONSchema }
  output?: { as?: string; schema?: JSONSchema }
  timeout?: unknown
  schedule?: unknown
  metadata?: Record<string, unknown>
}

export interface AgentConfig {
  config: Record<string, unknown>
  resources: Record<string, string>
  extensions: Record<string, FlowRef | null>
  policyPreset: string
}

export interface FlowRef {
  namespace: string
  name: string
  version: string
}

export interface WiringStatus {
  slotName: string
  wired: boolean
  flowRef?: FlowRef
  schemaMatch: 'ok' | 'mismatch' | 'unknown'
  error?: string
}

export interface ValidationError {
  path: string
  message: string
  severity: 'error' | 'warning'
  source: 'swf-schema' | 'function-manifest' | 'template-manifest' | 'slot-contract'
}

// ── Editor registry (loaded once per session) ──

export interface EditorRegistry {
  swfSchema: JSONSchema
  functions: Map<string, FunctionManifest>
  templates: Map<string, TemplateManifest>
  resources: Map<string, Resource[]>
}

// ── Editor mode — determines which surfaces are available ──

export type EditorMode = 'template' | 'agent' | 'flow' | 'canvas'

export interface EditorContext {
  mode: EditorMode
  registry: EditorRegistry
  document: EditorDocument
}
