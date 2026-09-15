<script lang="ts">
  /**
   * MCP Resource Form — two deployment modes:
   * 1. External: connect to an existing MCP server via URL
   * 2. Container: deploy a managed MCP server from a container image
   *
   * Emits the resource config as JSON via onchange.
   */

  import { mcpCall } from '$lib/services/api';
  import { onMount } from 'svelte';

  let {
    value = {},
    resourceName = '',
    onchange,
  }: {
    value: Record<string, any>;
    resourceName?: string;
    onchange?: (config: Record<string, any>) => void;
  } = $props();

  // Mode
  let mode = $state<'external' | 'container'>(value.image ? 'container' : 'external');

  // External mode
  let transport = $state<'sse' | 'stdio' | 'streamable-http'>(value.transport ?? 'sse');
  let url = $state(value.url ?? '');
  let headers = $state<{ key: string; value: string }[]>(
    value.headers ? Object.entries(value.headers).map(([k, v]) => ({ key: k, value: String(v) })) : []
  );
  let secret = $state(value.secret ?? '');
  let reconnect = $state(value.reconnect !== false);

  // Container mode
  let image = $state(value.image ?? '');
  let tag = $state(value.tag ?? 'latest');
  let port = $state(value.port ?? 3001);
  let envVars = $state<{ key: string; value: string }[]>(
    value.env ? Object.entries(value.env).map(([k, v]) => ({ key: k, value: String(v) })) : []
  );
  let healthPath = $state(value.health_check ?? '/health');
  let replicas = $state(value.replicas ?? 1);
  let cpuLimit = $state(value.resources?.cpu ?? '250m');
  let memLimit = $state(value.resources?.memory ?? '256Mi');
  let pullPolicy = $state(value.pull_policy ?? 'IfNotPresent');
  let containerTransport = $state<'sse' | 'streamable-http'>(value.transport ?? 'sse');

  // Endpoint / Trigger
  let epMode = $state<'default' | 'new' | 'existing'>(value.endpoint?.mode ?? 'default');
  let epPath = $state(value.endpoint?.path ?? '');
  let epExistingId = $state(value.endpoint?.trigger_id ?? '');
  let existingTriggers = $state<{ id: string; name: string; path?: string }[]>([]);

  onMount(async () => {
    const res = await mcpCall('list_triggers', {});
    if (res.ok && Array.isArray(res.data)) {
      existingTriggers = res.data.map((t: any) => ({ id: t.id, name: t.name, path: t.config?.path }));
    }
  });

  function emit() {
    if (mode === 'external') {
      const config: Record<string, any> = { transport, url };
      if (headers.length > 0) {
        const h: Record<string, string> = {};
        for (const { key, value } of headers) { if (key) h[key] = value; }
        if (Object.keys(h).length > 0) config.headers = h;
      }
      if (secret) config.secret = secret;
      if (!reconnect) config.reconnect = false;
      onchange?.(config);
    } else {
      const config: Record<string, any> = {
        mode: 'container',
        image: tag ? `${image}:${tag}` : image,
        port,
        transport: containerTransport,
        health_check: healthPath,
        replicas,
        resources: { cpu: cpuLimit, memory: memLimit },
        pull_policy: pullPolicy,
      };
      if (envVars.length > 0) {
        const env: Record<string, string> = {};
        for (const { key, value } of envVars) { if (key) env[key] = value; }
        if (Object.keys(env).length > 0) config.env = env;
      }
      // Endpoint
      if (epMode === 'new') {
        config.endpoint = { mode: 'new', path: epPath || `/mcp/${resourceName || 'server'}` };
      } else if (epMode === 'existing') {
        config.endpoint = { mode: 'existing', trigger_id: epExistingId };
      } else {
        config.endpoint = { mode: 'default' };
      }
      onchange?.(config);
    }
  }

  function addHeader() { headers = [...headers, { key: '', value: '' }]; }
  function removeHeader(i: number) { headers = headers.filter((_, j) => j !== i); emit(); }
  function addEnv() { envVars = [...envVars, { key: '', value: '' }]; }
  function removeEnv(i: number) { envVars = envVars.filter((_, j) => j !== i); emit(); }
</script>

<div class="mcp-form" oninput={emit} onchange={emit}>
  <!-- Mode tabs -->
  <div class="mcp-modes">
    <button class="mcp-mode" class:active={mode === 'external'} onclick={() => { mode = 'external'; emit(); }}>
      <svg width="14" height="14" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5"><circle cx="8" cy="8" r="5.5"/><path d="M8 2.5v11M2.5 8h11"/></svg>
      External URL
    </button>
    <button class="mcp-mode" class:active={mode === 'container'} onclick={() => { mode = 'container'; emit(); }}>
      <svg width="14" height="14" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5"><rect x="2" y="4" width="12" height="10" rx="1.5"/><path d="M5 4V2.5a.5.5 0 01.5-.5h5a.5.5 0 01.5.5V4"/><path d="M6 7h4M6 10h2"/></svg>
      Container
    </button>
  </div>

  {#if mode === 'external'}
    <!-- ═══ EXTERNAL MODE ═══ -->
    <div class="mcp-section">CONNECTION</div>
    <label>Transport
      <select bind:value={transport}>
        <option value="sse">SSE (Server-Sent Events)</option>
        <option value="streamable-http">Streamable HTTP</option>
        <option value="stdio">Stdio (local process)</option>
      </select>
    </label>

    {#if transport !== 'stdio'}
      <label>URL
        <input type="text" bind:value={url} placeholder="https://my-mcp-server.com/mcp" />
      </label>
    {:else}
      <label>Command
        <input type="text" bind:value={url} placeholder="/usr/local/bin/my-mcp-server" />
      </label>
    {/if}

    <div class="mcp-section">AUTHENTICATION</div>
    <label>Secret / API Key
      <input type="password" bind:value={secret} placeholder="Optional secret for webhook verification" />
    </label>

    <div class="mcp-section">HEADERS <span class="mcp-cnt">{headers.length}</span></div>
    {#each headers as h, i}
      <div class="mcp-kv">
        <input type="text" bind:value={h.key} placeholder="Header name" />
        <input type="text" bind:value={h.value} placeholder="Value" />
        <button class="mcp-rm" onclick={() => removeHeader(i)}>&times;</button>
      </div>
    {/each}
    <button class="mcp-add" onclick={addHeader}>+ Add Header</button>

    <div class="mcp-section">OPTIONS</div>
    <label class="mcp-check"><input type="checkbox" bind:checked={reconnect} /> Auto-reconnect on disconnect</label>

  {:else}
    <!-- ═══ CONTAINER MODE ═══ -->
    <div class="mcp-section">IMAGE</div>
    <div class="mcp-row2">
      <label>Image
        <input type="text" bind:value={image} placeholder="ghcr.io/org/my-mcp-server" />
      </label>
      <label>Tag
        <input type="text" bind:value={tag} placeholder="latest" />
      </label>
    </div>
    {#if image}
      <div class="mcp-image-preview">{image}:{tag || 'latest'}</div>
    {/if}

    <label>Pull policy
      <select bind:value={pullPolicy}>
        <option value="IfNotPresent">If Not Present</option>
        <option value="Always">Always</option>
        <option value="Never">Never</option>
      </select>
    </label>

    <div class="mcp-section">NETWORKING</div>
    <div class="mcp-row2">
      <label>Port
        <input type="number" bind:value={port} placeholder="3001" />
      </label>
      <label>Transport
        <select bind:value={containerTransport}>
          <option value="sse">SSE</option>
          <option value="streamable-http">Streamable HTTP</option>
        </select>
      </label>
    </div>
    <label>Health check path
      <input type="text" bind:value={healthPath} placeholder="/health" />
    </label>

    <div class="mcp-section">ENVIRONMENT <span class="mcp-cnt">{envVars.length}</span></div>
    {#each envVars as env, i}
      <div class="mcp-kv">
        <input type="text" bind:value={env.key} placeholder="ENV_NAME" />
        <input type="text" bind:value={env.value} placeholder="value" />
        <button class="mcp-rm" onclick={() => removeEnv(i)}>&times;</button>
      </div>
    {/each}
    <button class="mcp-add" onclick={addEnv}>+ Add Env Var</button>

    <div class="mcp-section">RESOURCES</div>
    <div class="mcp-row3">
      <label>Replicas
        <input type="number" bind:value={replicas} min="0" max="10" />
      </label>
      <label>CPU limit
        <input type="text" bind:value={cpuLimit} placeholder="250m" />
      </label>
      <label>Memory limit
        <input type="text" bind:value={memLimit} placeholder="256Mi" />
      </label>
    </div>

    <div class="mcp-section">ENDPOINT</div>
    <div class="mcp-ep-modes">
      <button class="mcp-ep-mode" class:active={epMode === 'default'} onclick={() => { epMode = 'default'; emit(); }}>
        Default
      </button>
      <button class="mcp-ep-mode" class:active={epMode === 'new'} onclick={() => { epMode = 'new'; epPath = `/mcp/${resourceName || 'server'}`; emit(); }}>
        New EP
      </button>
      <button class="mcp-ep-mode" class:active={epMode === 'existing'} onclick={() => { epMode = 'existing'; emit(); }}>
        Existing
      </button>
    </div>

    {#if epMode === 'default'}
      <div class="mcp-ep-info">
        Auto-assigned internal endpoint. Accessible within the cluster at <code>http://{resourceName || 'mcp-server'}:{port}/mcp</code>
      </div>
    {:else if epMode === 'new'}
      <label>Endpoint path
        <input type="text" bind:value={epPath} placeholder="/mcp/my-server" />
      </label>
      <div class="mcp-ep-info">
        Creates a trigger at <code>POST /webhook{epPath || '/mcp/server'}</code> — stable URL that survives container rotation.
      </div>
    {:else if epMode === 'existing'}
      <label>Attach to existing trigger
        <select bind:value={epExistingId} onchange={emit}>
          <option value="">-- select trigger --</option>
          {#each existingTriggers as t}
            <option value={t.id}>{t.name}{t.path ? ` (${t.path})` : ''}</option>
          {/each}
        </select>
      </label>
      {#if epExistingId}
        {@const sel = existingTriggers.find(t => t.id === epExistingId)}
        {#if sel}
          <div class="mcp-ep-info">Routes through <code>{sel.name}</code> — container can rotate without changing the endpoint.</div>
        {/if}
      {/if}
    {/if}
  {/if}
</div>

<style>
  .mcp-form { display: flex; flex-direction: column; gap: 8px; }

  /* Mode tabs */
  .mcp-modes { display: flex; gap: 4px; margin-bottom: 4px; }
  .mcp-mode {
    flex: 1; display: flex; align-items: center; justify-content: center; gap: 6px;
    padding: 10px 12px; border: 1.5px solid var(--border); border-radius: 8px;
    background: var(--bg-surface); color: var(--text-3); font-size: 13px; font-weight: 550;
    cursor: pointer; transition: all 0.12s;
  }
  .mcp-mode.active { border-color: var(--spec); background: var(--spec-bg); color: var(--spec-ink); }
  .mcp-mode:hover:not(.active) { background: var(--bg-subtle); color: var(--text-2); }

  .mcp-section { font-size: 10px; font-weight: 700; color: var(--text-3); letter-spacing: 0.06em; margin-top: 12px; margin-bottom: 2px; border-top: 1px solid var(--border); padding-top: 8px; display: flex; align-items: center; gap: 6px; }
  .mcp-section:first-of-type { border-top: none; margin-top: 4px; }
  .mcp-cnt { font-family: var(--font-mono); font-size: 9px; background: var(--bg-subtle); padding: 0 4px; border-radius: 2px; }

  .mcp-form label { display: flex; flex-direction: column; gap: 4px; font-size: 12px; font-weight: 500; color: var(--text-2); }
  .mcp-form input, .mcp-form select {
    padding: 8px 10px; border: 1px solid var(--border); border-radius: var(--r-sm, 4px);
    background: var(--bg-surface); color: var(--text-1); font-size: 13px; font-family: var(--font-mono); outline: none;
  }
  .mcp-form input:focus, .mcp-form select:focus { border-color: var(--spec); }
  .mcp-form input[type="password"] { letter-spacing: 0.1em; }

  .mcp-row2 { display: grid; grid-template-columns: 1fr 1fr; gap: 8px; }
  .mcp-row3 { display: grid; grid-template-columns: 1fr 1fr 1fr; gap: 8px; }

  .mcp-kv { display: flex; gap: 6px; align-items: center; margin-bottom: 4px; }
  .mcp-kv input { flex: 1; }
  .mcp-kv input:first-child { max-width: 40%; font-weight: 550; }
  .mcp-rm { width: 24px; height: 24px; border: none; border-radius: 4px; background: none; color: var(--text-5); cursor: pointer; display: grid; place-items: center; font-size: 14px; flex-shrink: 0; }
  .mcp-rm:hover { color: var(--err); background: var(--err-bg); }

  .mcp-add { border: 1px dashed var(--border); border-radius: 4px; padding: 4px 10px; font-size: 11px; color: var(--text-4); background: none; cursor: pointer; align-self: flex-start; }
  .mcp-add:hover { border-color: var(--spec); color: var(--spec); }

  .mcp-check { flex-direction: row !important; align-items: center; gap: 6px; font-size: 12px; }
  .mcp-check input { accent-color: var(--spec); }

  .mcp-image-preview { font-family: var(--font-mono); font-size: 12px; color: var(--runtime); background: var(--runtime-bg); padding: 5px 10px; border-radius: 4px; margin-top: 2px; }

  /* Endpoint */
  .mcp-ep-modes { display: flex; gap: 3px; }
  .mcp-ep-mode { padding: 5px 12px; border: 1px solid var(--border); border-radius: 5px; background: var(--bg-surface); color: var(--text-3); font-size: 12px; font-weight: 500; cursor: pointer; transition: all 0.12s; }
  .mcp-ep-mode.active { border-color: var(--runtime); background: var(--runtime-bg); color: var(--runtime); }
  .mcp-ep-mode:hover:not(.active) { background: var(--bg-subtle); }
  .mcp-ep-info { font-size: 12px; color: var(--text-3); line-height: 1.5; padding: 8px 10px; background: var(--bg-subtle); border-radius: 4px; margin-top: 4px; }
  .mcp-ep-info code { font-family: var(--font-mono); color: var(--runtime); font-size: 12px; }
</style>
