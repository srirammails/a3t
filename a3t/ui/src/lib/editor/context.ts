/**
 * Editor context — module-level singleton per session.
 * Initializes RegistryClient + ajv, caches function manifests.
 *
 * Usage:
 *   await initEditorContext()       // call once at app startup
 *   const ctx = getEditorContext()  // access from any component
 */

import { RegistryClient } from './registry'
import { initAjv } from './schema-utils'
import type { FunctionManifest } from './types'
import { API_BASE } from '$lib/config';

export interface EditorCtx {
  registry: RegistryClient
  functions: Map<string, FunctionManifest>
  ready: boolean
  error: string | null
}

let _ctx: EditorCtx | null = null
let _initPromise: Promise<EditorCtx> | null = null

/**
 * Initialize the editor context. Safe to call multiple times — returns
 * the same promise if already in flight or the cached result if done.
 */
export function initEditorContext(baseUrl?: string): Promise<EditorCtx> {
  if (_ctx?.ready) return Promise.resolve(_ctx)
  if (_initPromise) return _initPromise

  _initPromise = _doInit(baseUrl)
  return _initPromise
}

async function _doInit(baseUrl?: string): Promise<EditorCtx> {
  const base = baseUrl || API_BASE
  const registry = new RegistryClient(base)
  const functions = new Map<string, FunctionManifest>()
  let error: string | null = null

  try {
    // Fetch function manifests (full, with schemas)
    const fnList = await registry.listFunctions()
    for (const fn of fnList) {
      const full = await registry.getFunction(fn.name)
      if (full) functions.set(full.name, full)
    }
  } catch (e) {
    console.warn('[EditorContext] Registry fetch failed:', e)
    error = `Registry: ${e}`
  }

  // Init ajv (fetches SWF schema from serverlessworkflow.io)
  try {
    await initAjv()
  } catch (e) {
    console.warn('[EditorContext] ajv init failed:', e)
    if (!error) error = `AJV: ${e}`
  }

  _ctx = { registry, functions, ready: true, error }
  return _ctx
}

/**
 * Get the editor context. Returns null if not yet initialized.
 */
export function getEditorContext(): EditorCtx | null {
  return _ctx
}

/**
 * Get a function manifest by name. Returns null if context not ready or function not found.
 */
export function getFunctionManifest(name: string): FunctionManifest | null {
  return _ctx?.functions.get(name) ?? null
}

/**
 * Reset the context (for testing or re-init after config change).
 */
export function resetEditorContext() {
  _ctx = null
  _initPromise = null
}
