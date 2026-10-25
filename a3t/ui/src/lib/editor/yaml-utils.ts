/**
 * YAML utility functions — parsing, path helpers, workflow structure queries.
 */

import * as YAML from 'yaml'
import type { SWFWorkflow } from './types'

/**
 * Parse a YAML string into an SWF workflow object.
 * Returns null on parse failure.
 */
export function parseWorkflow(yamlStr: string): SWFWorkflow | null {
  try {
    const parsed = YAML.parse(yamlStr)
    if (!parsed || typeof parsed !== 'object') return null
    return parsed as SWFWorkflow
  } catch {
    return null
  }
}

/**
 * Stringify a workflow object back to YAML.
 */
export function stringifyWorkflow(workflow: SWFWorkflow): string {
  return YAML.stringify(workflow, { indent: 2, lineWidth: 120 })
}

/**
 * Find the primary callable task — first task in do: that has a call: key.
 */
export function findPrimaryCall(workflow: SWFWorkflow): { call: string; with?: Record<string, unknown> } | null {
  for (const taskItem of workflow.do ?? []) {
    if (typeof taskItem !== 'object' || taskItem === null) continue
    for (const task of Object.values(taskItem as Record<string, unknown>)) {
      if (task && typeof task === 'object' && 'call' in task) {
        return task as { call: string; with?: Record<string, unknown> }
      }
    }
  }
  return null
}

const CONFIG_DEFAULTS_RE = /^\$\{\s*(\{[\s\S]*\})\s*\+\s*\(\s*\.config\s*\/\/\s*\{\}\s*\)\s*\}$/

/**
 * A template's config as DEFAULTS under the caller's: `${ {...} + (.config // {}) }`.
 * jq's `+` merges objects with the right side winning, so each agent's own config
 * (LLM, MCP servers, tool ceiling — sent by the chat wrapper) overrides the template,
 * and the template fills in whatever the agent doesn't set. A literal `config:`
 * here replaced every agent's config (e.g. no LLM → `default-llm`).
 */
export function configDefaultsExpr(defaults: Record<string, unknown>): string {
  return `\${ ${JSON.stringify(defaults)} + (.config // {}) }`
}

/** The defaults in a `configDefaultsExpr` string, or null if `v` isn't one. */
export function parseConfigDefaults(v: unknown): Record<string, unknown> | null {
  if (typeof v !== 'string') return null
  const m = v.trim().match(CONFIG_DEFAULTS_RE)
  if (!m) return null
  try {
    const o = JSON.parse(m[1])
    return o && typeof o === 'object' && !Array.isArray(o) ? o : null
  } catch {
    return null
  }
}

/**
 * Get config from the primary call task's with.config (a literal object, or the
 * template defaults of a `configDefaultsExpr`).
 */
export function getCallConfig(workflow: SWFWorkflow): Record<string, unknown> {
  const call = findPrimaryCall(workflow)
  if (!call?.with) return {}
  const w = call.with as Record<string, unknown>
  return (parseConfigDefaults(w.config) ?? w.config ?? w) as Record<string, unknown>
}

/**
 * Get extensions from use.extensions block.
 */
export function getExtensions(workflow: SWFWorkflow): unknown[] {
  return workflow.use?.extensions ?? []
}

/**
 * Get input schema properties (if input block exists).
 */
export function getInputFields(workflow: SWFWorkflow): Array<{ name: string; type: string }> {
  const schema = workflow.input?.schema
  if (!schema || typeof schema !== 'object') return []
  const props = (schema as Record<string, unknown>).properties as Record<string, { type?: string }> | undefined
  if (!props) return []
  return Object.entries(props).map(([name, def]) => ({ name, type: def?.type ?? 'any' }))
}

/**
 * Get output schema properties (if output block exists).
 */
export function getOutputFields(workflow: SWFWorkflow): Array<{ name: string; type: string }> {
  const schema = workflow.output?.schema
  if (!schema || typeof schema !== 'object') return []
  const props = (schema as Record<string, unknown>).properties as Record<string, { type?: string }> | undefined
  if (!props) return []
  return Object.entries(props).map(([name, def]) => ({ name, type: def?.type ?? 'any' }))
}

/**
 * Get emit tasks from the do block.
 */
export function getEmitTasks(workflow: SWFWorkflow): Array<{ name: string; event: string }> {
  const results: Array<{ name: string; event: string }> = []
  for (const taskItem of workflow.do ?? []) {
    if (typeof taskItem !== 'object' || taskItem === null) continue
    for (const [taskName, task] of Object.entries(taskItem as Record<string, unknown>)) {
      if (task && typeof task === 'object' && 'emit' in task) {
        const emitDef = (task as { emit?: { event?: { type?: string } } }).emit
        results.push({ name: taskName, event: emitDef?.event?.type ?? taskName })
      }
    }
  }
  return results
}

/**
 * Get custom error definitions from use.errors.
 */
export function getErrors(workflow: SWFWorkflow): Array<{ name: string; type: string }> {
  const errors = workflow.use?.errors
  if (!errors || typeof errors !== 'object') return []
  return Object.entries(errors).map(([name]) => ({ name, type: 'error' }))
}

/**
 * Resolve a JSON Pointer path to a human-readable field address.
 * e.g. "/document/namespace" → "document > namespace"
 */
export function pointerToPath(pointer: string): string {
  return pointer.replace(/^\//, '').replace(/\//g, ' > ')
}

/**
 * Check if a workflow is an agent template (has call: a3t:agent).
 */
export function isAgentTemplate(workflow: SWFWorkflow): boolean {
  const call = findPrimaryCall(workflow)
  return call?.call === 'a3t:agent' || call?.call === 'agent'
}

/**
 * Create a blank SWF workflow scaffold.
 */
export function createBlankWorkflow(name: string, namespace = 'a3t'): SWFWorkflow {
  return {
    document: { dsl: '1.0.3', namespace, name, version: '1.0.0' },
    do: [{ [`step_1`]: { set: { message: 'Hello' } } }],
  }
}

/**
 * Create a blank agent template scaffold.
 */
export function createBlankAgentTemplate(name: string, namespace = 'a3t'): SWFWorkflow {
  return {
    document: { dsl: '1.0.3', namespace, name, version: '1.0.0' },
    use: {
      extensions: [
        { on_turn_start: { extend: 'all', when: '${ $task.name == "init_turn" }', before: [], after: [] } },
        { before_llm: { extend: 'call', before: [], after: [] } },
        { on_tool_call: { extend: 'all', when: '${ $task.name == "dispatch_tools" }', before: [], after: [] } },
      ]
    },
    do: [{
      run_agent: {
        call: 'a3t:agent',
        with: {
          config: {
            system_prompt: 'You are a helpful assistant.',
            max_iterations: 10,
            max_tokens: 4096,
            temperature: 0.7,
            tools: [],
            tool_flows: {},
            mcp_servers: [],
            memory_namespace: name,
            ttl: 1800,
          },
          session_id: '${ .session_id // "default" }',
          message: '${ .message }',
        }
      }
    }],
  }
}
