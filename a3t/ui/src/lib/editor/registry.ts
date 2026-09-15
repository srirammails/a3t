/**
 * Registry client — session-cached fetch for function/template/resource manifests.
 * All editor behaviour derives from registry data. No hardcoding.
 */

import type { FunctionManifest, TemplateManifest, Resource, FlowRef, SWFWorkflow } from './types'
import { getAccessToken } from '$lib/services/api'

export class RegistryClient {
  private readonly base: string
  private readonly cache = new Map<string, unknown>()

  constructor(base: string) {
    this.base = base.replace(/\/$/, '')
  }

  async getFunction(name: string): Promise<FunctionManifest | null> {
    return this.cached<FunctionManifest>(`/registry/functions/${encodeURIComponent(name)}`)
  }

  async listFunctions(): Promise<FunctionManifest[]> {
    return (await this.cached<FunctionManifest[]>('/registry/functions')) ?? []
  }

  async getTemplate(ref: string): Promise<TemplateManifest | null> {
    const atIdx = ref.lastIndexOf('@')
    const nsname = atIdx > -1 ? ref.slice(0, atIdx) : ref
    const version = atIdx > -1 ? ref.slice(atIdx + 1) : 'latest'
    const [ns, name] = nsname.split('/')
    return this.cached<TemplateManifest>(`/registry/templates/${ns}/${name}?version=${version}`)
  }

  async listTemplates(): Promise<TemplateManifest[]> {
    return (await this.cached<TemplateManifest[]>('/registry/templates')) ?? []
  }

  async getResources(type: string): Promise<Resource[]> {
    return (await this.cached<Resource[]>(`/registry/resources?type=${encodeURIComponent(type)}`)) ?? []
  }

  async getWorkflow(ref: FlowRef): Promise<SWFWorkflow | null> {
    return this.cached<SWFWorkflow>(
      `/flows/${ref.namespace}/${ref.name}?version=${encodeURIComponent(ref.version)}`
    )
  }

  private async cached<T>(path: string): Promise<T | null> {
    if (this.cache.has(path)) return this.cache.get(path) as T
    try {
      // Every /registry/* route is JWT-gated. A bare fetch sends no token, so
       // these all 401'd — which is why the editor context reported
       // "0 functions loaded" and template manifests never arrived.
      const token = getAccessToken()
      const res = await fetch(this.base + path, {
        headers: token ? { Authorization: `Bearer ${token}` } : {},
      })
      if (!res.ok) return null
      const ct = res.headers.get('content-type') ?? ''
      let data: unknown
      if (ct.includes('yaml')) {
        const { load } = await import('js-yaml')
        data = load(await res.text())
      } else {
        data = await res.json()
      }
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
