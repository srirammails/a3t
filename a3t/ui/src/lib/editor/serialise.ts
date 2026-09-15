/**
 * Serialiser — EditorDocument → YAML string.
 * One-way only. Never mutates the YAML string directly.
 */

import * as YAML from 'yaml'
import type { EditorDocument, SWFWorkflow, ExtensionSlot, FlowRef } from './types'

/**
 * Serialise an EditorDocument back to CNCF SWF YAML.
 * This is the canonical output — what gets saved/deployed.
 */
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

  // 2. Write resource refs into document.metadata
  if (doc.agentConfig?.resources) {
    wf.document.metadata = wf.document.metadata ?? {}
    for (const [slot, ref] of Object.entries(doc.agentConfig.resources)) {
      wf.document.metadata[`a3t.io/resources.${slot}`] = ref
    }
  }

  // 3. Write template ref + writer hash
  wf.document.metadata = wf.document.metadata ?? {}
  if (doc.templateRef) wf.document.metadata['a3t.io/template'] = doc.templateRef
  wf.document.metadata['a3t.io/writer-hash'] = computeHash(doc)

  return YAML.stringify(wf, { indent: 2, lineWidth: 120 })
}

/**
 * Serialise just the workflow (without agent config overlay).
 * Used for simple template/flow editing.
 */
export function serialiseWorkflow(workflow: SWFWorkflow): string {
  return YAML.stringify(workflow, { indent: 2, lineWidth: 120 })
}

/**
 * Generate an Agent CR from the editor document.
 * Used for the CRD/Deploy surface.
 */
export function generateAgentCR(doc: EditorDocument): string {
  const cr = {
    apiVersion: 'a3t.io/v1',
    kind: 'Agent',
    metadata: {
      name: doc.workflow.document.name,
      namespace: doc.workflow.document.namespace,
      annotations: {
        'a3t.io/managed-by': 'editor',
        'a3t.io/template': doc.templateRef ?? '',
        'a3t.io/writer-hash': doc.writerHash ?? '',
      }
    },
    spec: {
      template: doc.templateManifest ? {
        ref: `${doc.templateManifest.namespace}/${doc.templateManifest.name}`,
        version: doc.templateManifest.version,
      } : undefined,
      resources: doc.agentConfig?.resources ?? {},
      config: doc.agentConfig?.config ?? {},
      extensions: doc.agentConfig?.extensions ?? {},
      policy: {
        preset: doc.agentConfig?.policyPreset ?? 'a3t-system/standard',
        runtime: { mode: 'sandboxed' }
      }
    }
  }
  return YAML.stringify(cr, { indent: 2 })
}

// ── Internal helpers ──

function buildExtensions(
  slots: Record<string, ExtensionSlot>,
  wiring: Record<string, FlowRef | null>
): unknown[] {
  return Object.entries(slots).map(([slotName, slot]) => ({
    [slotName]: {
      extend: slot.extends,
      ...(slot.when ? { when: slot.when } : {}),
      before: wiring[slotName] ? [{
        [`run_${slotName}`]: {
          run: {
            workflow: {
              namespace: wiring[slotName]!.namespace,
              name: wiring[slotName]!.name,
              version: wiring[slotName]!.version,
            }
          }
        }
      }] : [],
      after: []
    }
  }))
}

function computeHash(doc: EditorDocument): string {
  // Simple hash for change detection
  const content = JSON.stringify({
    workflow: doc.workflow,
    config: doc.agentConfig?.config,
    resources: doc.agentConfig?.resources,
    extensions: doc.agentConfig?.extensions,
  })
  let hash = 0
  for (let i = 0; i < content.length; i++) {
    const chr = content.charCodeAt(i)
    hash = ((hash << 5) - hash) + chr
    hash |= 0
  }
  return Math.abs(hash).toString(36)
}
