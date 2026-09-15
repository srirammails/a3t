/**
 * Schema utilities — ONLY place ajv is imported.
 * Uses ajv/dist/2020 for JSON Schema 2020-12 (required by SWF schema).
 *
 * Per spec: the default ajv export is draft-07 and silently ignores
 * unevaluatedProperties, producing false positives.
 */

import Ajv2020 from 'ajv/dist/2020'
import addFormats from 'ajv-formats'
import type { ValidationError, JSONSchema, FunctionManifest } from './types'

const SWF_SCHEMA_ID = 'https://serverlessworkflow.io/schemas/1.0.3/workflow.yaml'
const SWF_SCHEMA_URL = SWF_SCHEMA_ID

let _ajv: InstanceType<typeof Ajv2020> | null = null
let _swfSchema: unknown | null = null

/**
 * Initialize ajv with the official SWF 1.0.3 schema.
 * Call once at editor session start.
 * Fetches the schema from serverlessworkflow.io and registers it.
 */
export async function initAjv(): Promise<InstanceType<typeof Ajv2020>> {
  if (_ajv) return _ajv

  _ajv = new Ajv2020({ strict: false, allErrors: true })
  addFormats(_ajv)

  // Try to fetch the SWF schema with a timeout — non-fatal if it fails
  try {
    const controller = new AbortController()
    const timeout = setTimeout(() => controller.abort(), 8000)
    const res = await fetch(SWF_SCHEMA_URL, { signal: controller.signal })
    clearTimeout(timeout)

    if (res.ok) {
      const yamlText = await res.text()
      const { load } = await import('js-yaml')
      _swfSchema = load(yamlText)
      // MUST addSchema before any compile() — $defs reference each other
      _ajv.addSchema(_swfSchema as Record<string, unknown>)
    } else {
      console.warn(`[schema-utils] SWF schema fetch returned ${res.status} — Pass 1 validation disabled`)
    }
  } catch (e) {
    console.warn('[schema-utils] SWF schema fetch failed — Pass 1 validation disabled:', e)
    // ajv still works for Pass 2 (function manifest validation)
  }

  return _ajv
}

export function getSwfSchema(): unknown {
  if (!_swfSchema) throw new Error('AJV not initialised — call initAjv() first')
  return _swfSchema
}

function getAjv(): InstanceType<typeof Ajv2020> {
  if (!_ajv) throw new Error('AJV not initialised — call initAjv() first')
  return _ajv
}

/**
 * Validate a complete SWF workflow document against the official schema.
 * Pass 1 of the validation pipeline.
 */
export function validateWorkflow(doc: unknown): ValidationError[] {
  const ajv = getAjv()
  const validate = ajv.getSchema(SWF_SCHEMA_ID)
  if (!validate) return [] // SWF schema not available — skip Pass 1 silently
  const valid = validate(doc)
  if (valid) return []
  return mapErrors(validate.errors, 'swf-schema')
}

/**
 * Validate call.with params against a function manifest's with_schema.
 * Pass 2a of the validation pipeline.
 */
export function validateWithParams(withValue: unknown, withSchema: JSONSchema): ValidationError[] {
  const ajv = getAjv()
  const validate = ajv.compile(withSchema)
  const valid = validate(withValue)
  if (valid) return []
  return mapErrors(validate.errors, 'function-manifest')
}

/**
 * Validate document.metadata keys against a function manifest's metadata_schema.
 * Pass 2b of the validation pipeline.
 */
export function validateMetadata(
  metadata: Record<string, unknown>,
  metadataSchema: FunctionManifest['metadata_schema']
): ValidationError[] {
  if (!metadataSchema) return []
  const errors: ValidationError[] = []
  for (const [key, def] of Object.entries(metadataSchema)) {
    if (def.required && metadata[key] == null) {
      errors.push({ path: `/document/metadata/${key}`, message: 'required field missing', severity: 'error', source: 'function-manifest' })
      continue
    }
    if (metadata[key] == null) continue
    if (def.type === 'resource-ref') {
      if (typeof metadata[key] !== 'string' || !(metadata[key] as string).includes('/')) {
        errors.push({ path: `/document/metadata/${key}`, message: 'must be a resource ref in format "namespace/name"', severity: 'error', source: 'function-manifest' })
      }
    } else if (def.enum && !def.enum.includes(metadata[key] as string)) {
      errors.push({ path: `/document/metadata/${key}`, message: `must be one of: ${def.enum.join(', ')}`, severity: 'error', source: 'function-manifest' })
    } else if (def.pattern && !new RegExp(def.pattern).test(metadata[key] as string)) {
      errors.push({ path: `/document/metadata/${key}`, message: `must match pattern ${def.pattern}`, severity: 'error', source: 'function-manifest' })
    }
  }
  return errors
}

/**
 * Validate config values against a template's config_schema.
 * Pass 3 of the validation pipeline.
 */
export function validateConfig(config: unknown, configSchema: JSONSchema): ValidationError[] {
  const ajv = getAjv()
  const validate = ajv.compile(configSchema)
  const valid = validate(config)
  if (valid) return []
  return mapErrors(validate.errors, 'template-manifest')
}

/**
 * Validate a slot workflow's output schema satisfies the slot contract.
 * Pass 4 of the validation pipeline.
 */
export function validateSlotOutputContract(
  wiredOutputSchema: JSONSchema,
  slotOutputSchema: JSONSchema
): ValidationError[] {
  const errors: ValidationError[] = []
  const required = (slotOutputSchema.required ?? []) as string[]
  const wiredProps = (wiredOutputSchema.properties ?? {}) as Record<string, unknown>
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

/**
 * Compile a $defs sub-schema by ref — use for validating extension blocks etc.
 */
export function compileRef(defPath: string) {
  const ajv = getAjv()
  return ajv.compile({ $ref: `${SWF_SCHEMA_ID}#/${defPath}` })
}

// ── Internal helpers ──

function mapErrors(errors: unknown[] | null | undefined, source: ValidationError['source']): ValidationError[] {
  if (!errors) return []
  return (errors as Array<{ instancePath?: string; message?: string }>).map(e => ({
    path: e.instancePath || '/',
    message: e.message ?? 'validation error',
    severity: 'error' as const,
    source
  }))
}
