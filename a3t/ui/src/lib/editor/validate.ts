/**
 * Validation pipeline — 4 passes run in sequence.
 * Results merged into EditorDocument.validationErrors.
 */

import {
  validateWorkflow, validateWithParams, validateMetadata,
  validateConfig, validateSlotOutputContract
} from './schema-utils'
import { RegistryClient } from './registry'
import type { EditorDocument, ValidationError, SWFWorkflow } from './types'

/**
 * Run the full 4-pass validation pipeline.
 *
 * Pass 1: SWF structural validation (schema from serverlessworkflow.io)
 * Pass 2: Function manifest validation (with: params + metadata)
 * Pass 3: Template manifest validation (config values + resource slots)
 * Pass 4: Slot contract validation (async — fetches wired workflow schemas)
 */
export async function runValidation(
  doc: EditorDocument,
  registry: RegistryClient
): Promise<ValidationError[]> {
  const errors: ValidationError[] = []

  // Pass 1 — SWF structural validation
  errors.push(...validateWorkflow(doc.workflow))

  // Pass 2 — function manifest validation
  const primaryCall = findPrimaryCall(doc.workflow)
  if (primaryCall && doc.functionManifest) {
    if (doc.functionManifest.with_schema && primaryCall.with) {
      errors.push(...validateWithParams(primaryCall.with, doc.functionManifest.with_schema))
    }
    if (doc.functionManifest.metadata_schema) {
      errors.push(...validateMetadata(
        doc.workflow.document?.metadata ?? {},
        doc.functionManifest.metadata_schema
      ))
    }
  }

  // Pass 3 — template manifest validation
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

  // Pass 4 — slot contract validation (async)
  if (doc.templateManifest && doc.agentConfig) {
    for (const [slotName, slot] of Object.entries(doc.templateManifest.extension_slots)) {
      const wiring = doc.agentConfig.extensions[slotName]
      if (!wiring) continue
      try {
        const wiredWorkflow = await registry.getWorkflow(wiring)
        if (!wiredWorkflow?.output?.schema) {
          errors.push({
            path: `/extensions/${slotName}`,
            message: 'wired workflow has no output.schema — cannot verify slot contract',
            severity: 'warning',
            source: 'slot-contract'
          })
          continue
        }
        errors.push(...validateSlotOutputContract(
          wiredWorkflow.output.schema as Record<string, unknown>,
          slot.output_schema
        ))
      } catch {
        errors.push({
          path: `/extensions/${slotName}`,
          message: 'could not fetch wired workflow to verify slot contract',
          severity: 'warning',
          source: 'slot-contract'
        })
      }
    }
  }

  return errors
}

/**
 * Find the primary callable task — first task in do: that has a call: key.
 */
export function findPrimaryCall(workflow: SWFWorkflow): { call: string; with?: unknown } | null {
  for (const taskItem of workflow.do ?? []) {
    if (typeof taskItem !== 'object' || taskItem === null) continue
    const values = Object.values(taskItem as Record<string, unknown>)
    for (const task of values) {
      if (task && typeof task === 'object' && 'call' in task) {
        return task as { call: string; with?: unknown }
      }
    }
  }
  return null
}
