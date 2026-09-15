<script lang="ts">
  import { Handle, Position } from '@xyflow/svelte';
  let { data }: { data: any } = $props();
  const extensions: any[] = data.extensions ?? [];
  const mcps: any[] = data.mcpServers ?? [];
  const toolFlows: Record<string, any> = data.toolFlows ?? {};
  const extCount = extensions.length;
  const intgCount = mcps.length + Object.keys(toolFlows).length;
  let openSection = $state<string | null>(null);
  function toggle(id: string, e: MouseEvent) { e.stopPropagation(); openSection = openSection === id ? null : id; }
  // ── Derive ALL section data from actual YAML ──
  const agentCfg = data.agentConfig?.config ?? data.agentConfig ?? {};

  // CONFIG: K/V pairs from call.with.config
  const configItems: Array<{name:string;type:string}> = [];
  if (agentCfg && typeof agentCfg === 'object') {
    for (const [k, v] of Object.entries(agentCfg)) {
      if (k.startsWith('_') || k === 'config') continue;
      const t = v === null ? 'null' : Array.isArray(v) ? 'array' : typeof v;
      configItems.push({ name: k, type: t });
    }
  }

  // EXTENSIONS: from use.extensions in YAML
  const extItems = extensions.map((ext: any, i: number) => {
    const name = typeof ext === 'object' ? (Object.keys(ext)[0] ?? `ext-${i}`) : `ext-${i}`;
    const def = typeof ext === 'object' ? (ext[name] ?? {}) : {};
    return { name, type: def.extend ?? 'all' };
  });

  // EVENTS: emit tasks from the do block (if any)
  const eventItems: Array<{name:string;type:string}> = [];
  // TODO: scan do[] for emit tasks

  // INPUT: from top-level input.schema (if defined)
  const inputItems: Array<{name:string;type:string}> = [];
  const inputSchema = data.agentConfig?.input?.schema?.properties;
  if (inputSchema && typeof inputSchema === 'object') {
    for (const [k, v] of Object.entries(inputSchema)) {
      inputItems.push({ name: k, type: (v as any)?.type ?? 'any' });
    }
  }

  // OUTPUT: from top-level output (if defined)
  const outputItems: Array<{name:string;type:string}> = [];
  const outputSchema = data.agentConfig?.output?.schema?.properties;
  if (outputSchema && typeof outputSchema === 'object') {
    for (const [k, v] of Object.entries(outputSchema)) {
      outputItems.push({ name: k, type: (v as any)?.type ?? 'any' });
    }
  }

  // ERRORS: from use.errors (if defined)
  const errorItems: Array<{name:string;type:string}> = [];
  const useErrors = data.agentConfig?.use?.errors;
  if (useErrors && typeof useErrors === 'object') {
    for (const [k, v] of Object.entries(useErrors)) {
      errorItems.push({ name: k, type: 'error' });
    }
  }

  // Presence flags — derived from YAML
  const hasConfig = configItems.length > 0;
  const hasExt = extItems.length > 0;
  const hasEvents = eventItems.length > 0;
  const hasInput = inputItems.length > 0;
  const hasOutput = outputItems.length > 0;
  const hasError = errorItems.length > 0;
  const hasIntg = mcps.length > 0 || Object.keys(toolFlows).length > 0;
  interface Sec { id: string; label: string; items: Array<{name:string;type:string}>; add: string; dashed?: boolean; color?: string; inYaml: boolean }
  const secs: Sec[] = [
    { id: 'input',  label: 'Input',      items: inputItems,  add: '+ Add Input',     color: '#0891b2', inYaml: hasInput },
    { id: 'config', label: 'Config',     items: configItems, add: '+ Add Field',     inYaml: hasConfig },
    { id: 'ext',    label: 'Extensions', items: extItems,    add: '+ Add Hook',      dashed: true, inYaml: hasExt },
    { id: 'events', label: 'Events',     items: eventItems,  add: '+ Add Event',     inYaml: hasEvents },
    { id: 'output', label: 'Output',     items: outputItems, add: '+ Add Output',    color: '#059669', inYaml: hasOutput },
    { id: 'error',  label: 'Errors',     items: errorItems,  add: '+ Add Error Type', color: '#dc2626', inYaml: hasError },
  ];
  let activeSec = $derived(secs.find(s => s.id === openSection));
</script>

<div class="n">
  <!-- Left ear: Input (placeholder — grayed if not in YAML) -->
  <div class="ear left empty" role="button" tabindex="0" onclick={(e) => toggle('input', e)}>
    <svg width="10" height="10" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round"><path d="M3 8h10M10 5l3 3-3 3"/></svg>
    <Handle type="target" position={Position.Left} id="in" class="hdot hdot-l" />
  </div>

  <!-- Main box -->
  <div class="box">
    <div class="icon">
      <svg width="18" height="18" viewBox="0 0 24 24" fill="none">
        <rect x="3" y="6" width="18" height="14" rx="3" stroke="white" stroke-width="1.5"/>
        <circle cx="9" cy="13" r="1.5" fill="white"/><circle cx="15" cy="13" r="1.5" fill="white"/>
        <rect x="8" y="2" width="8" height="5" rx="2" stroke="white" stroke-width="1.5"/>
      </svg>
    </div>
    <div class="txt">
      <span class="name">{data.label ?? 'agent'}</span>
      <span class="sub">{data.mode ?? 'session'} agent</span>
    </div>
  </div>

  <!-- Right ears: Output + Error (placeholders — grayed if not in YAML) -->
  <div class="right-ears">
    <div class="ear right empty" role="button" tabindex="0" onclick={(e) => toggle('output', e)}>
      <svg width="10" height="10" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round"><path d="M3 8h10M10 5l3 3-3 3"/></svg>
      <Handle type="source" position={Position.Right} id="out" class="hdot hdot-r" />
    </div>
    <div class="ear right empty" role="button" tabindex="0" onclick={(e) => toggle('error', e)}>
      <svg width="9" height="9" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5"><path d="M8 2l6 11H2L8 2z"/><path d="M8 7v2.5M8 11v.5"/></svg>
    </div>
  </div>

  <!-- Bottom ears -->
  <div class="bottom-ears">
    <div class="bear" class:on={openSection==='config'} class:empty={!hasConfig} role="button" tabindex="0" onclick={(e) => toggle('config', e)}>
      <svg width="9" height="9" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5"><circle cx="8" cy="8" r="2.5"/><path d="M8 1v2M8 13v2M1 8h2M13 8h2M3.05 3.05l1.41 1.41M11.54 11.54l1.41 1.41M3.05 12.95l1.41-1.41M11.54 4.46l1.41-1.41"/></svg>
      <span class="cnt">{configItems.length}</span>
    </div>
    <div class="bear" class:on={openSection==='ext'} class:empty={!hasExt} role="button" tabindex="0" onclick={(e) => toggle('ext', e)}>
      <svg width="9" height="9" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5"><rect x="2" y="2" width="12" height="12" rx="2"/><path d="M6 8h4M8 6v4"/></svg>
      <span class="cnt">{extCount}</span>
      <Handle type="source" position={Position.Bottom} id="bp-ext" class="hdot hdot-b" />
    </div>
    <div class="bear" class:on={openSection==='events'} class:empty={!hasEvents} role="button" tabindex="0" onclick={(e) => toggle('events', e)}>
      <svg width="9" height="9" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5"><circle cx="8" cy="8" r="6"/><circle cx="8" cy="8" r="2.5"/></svg>
      <span class="cnt">{eventItems.length}</span>
      <Handle type="source" position={Position.Bottom} id="bp-events" class="hdot hdot-b" />
    </div>
  </div>

  <!-- Panel -->
  {#if activeSec}
    <div class="panel" class:dashed={activeSec.dashed}>
      <div class="ph" style={activeSec.color ? `border-left-color:${activeSec.color}` : ''}>
        <span class="pht">{activeSec.label}</span>
        <button class="phx" onclick={(e) => {e.stopPropagation(); openSection=null}}>×</button>
      </div>
      <div class="pb">
        {#each activeSec.items as item}
          <div class="pr"><span class="pn">{item.name}</span><span class="pt" class:fn={item.type==='function'} class:err={activeSec.color==='#dc2626'}>{item.type}</span></div>
        {/each}
        {#if (activeSec as any).more > 0}<div class="pm">+{(activeSec as any).more} more</div>{/if}
        <button class="pa" onclick={(e) => {
          e.stopPropagation();
          if (typeof window !== 'undefined') {
            window.dispatchEvent(new CustomEvent('agent-add', { detail: activeSec.id }));
          }
        }}>{activeSec.add}</button>
      </div>
    </div>
  {/if}
</div>

<style>
  .n {
    --c: #5530a8;
    display: grid; grid-template-columns: auto 1fr auto;
    grid-template-rows: auto auto auto; gap: 0; align-items: center;
    filter: drop-shadow(0 2px 6px rgba(85,48,168,0.06));
  }

  /* Ears */
  .ear {
    background: linear-gradient(180deg, #f5f2fd 0%, #eee8fb 100%);
    border: 1.5px solid #d4c4f0;
    color: var(--c); cursor: pointer; position: relative;
    display: flex; align-items: center; justify-content: center;
    z-index: 0; transition: background 0.12s;
  }
  .ear.left { border-radius: 8px 0 0 8px; border-right: none; grid-row: 1; grid-column: 1; align-self: center; padding: 6px 5px; }
  .ear.right { border-radius: 0 6px 6px 0; border-left: none; padding: 4px 4px; }

  .right-ears {
    grid-row: 1; grid-column: 3; align-self: center;
    display: flex; flex-direction: column; gap: 2px;
  }
  .ear.empty { opacity: 0.35; }
  .ear.empty:hover { opacity: 0.6; }
  .ear:hover { background: #ede7ff; }
  .ear span { cursor: pointer; padding: 2px; border-radius: 4px; transition: background 0.1s; }
  .ear span:hover { background: rgba(85,48,168,0.08); }

  /* Connector handle dots — positioned at ear edges */
  :global(.hdot) {
    width: 5px !important; height: 5px !important;
    background: #c4b8e0 !important; border: none !important;
    min-width: 0 !important; min-height: 0 !important;
  }
  :global(.hdot:hover) { background: var(--c) !important; }
  :global(.hdot-l) { left: -3px !important; top: 50% !important; transform: translateY(-50%) !important; }
  :global(.hdot-r) { right: -3px !important; top: 50% !important; transform: translateY(-50%) !important; }
  :global(.hdot-b) { bottom: -3px !important; left: 50% !important; transform: translateX(-50%) !important; top: auto !important; }
  :global(.hdot-err) { background: #e8b4b4 !important; }
  :global(.hdot-err:hover) { background: #dc2626 !important; }

  /* Main box */
  .box {
    display: flex; align-items: center; gap: 10px;
    padding: 12px 14px;
    background: linear-gradient(180deg, #ffffff 0%, #faf8ff 100%);
    border: 2px solid var(--c);
    border-radius: 12px;
    z-index: 1; grid-row: 1; grid-column: 2;
    min-width: 160px;
  }
  .icon {
    width: 30px; height: 30px; border-radius: 8px;
    background: linear-gradient(135deg, #7044d4, var(--c));
    box-shadow: 0 1px 4px rgba(85,48,168,0.2);
    display: grid; place-items: center; flex-shrink: 0;
  }
  .txt { display: flex; flex-direction: column; }
  .name { font-size: 12.5px; font-weight: 600; color: #1a1a1a; letter-spacing: -0.01em; }
  .sub { font-size: 8.5px; color: var(--c); font-family: var(--font-mono); opacity: 0.8; margin-top: 1px; }

  /* Bottom ears */
  .bottom-ears {
    grid-row: 2; grid-column: 2;
    display: flex; gap: 2px;
    justify-content: center;
    margin-top: -2px;
  }

  .bear {
    display: flex; align-items: center; gap: 1px;
    padding: 2px 4px;
    background: linear-gradient(180deg, #f5f2fd 0%, #eee8fb 100%);
    border: 1.5px solid #d4c4f0; border-top: none;
    border-radius: 0 0 8px 8px;
    color: var(--c); cursor: pointer; position: relative;
    transition: background 0.12s;
  }
  .bear:hover { background: #ede7ff; }
  .bear.on { background: white; border-color: var(--c); }
  .bear.empty { opacity: 0.3; }
  .bear.empty:hover { opacity: 0.5; }

  .cnt {
    font-size: 8px; font-weight: 700;
    background: white; color: var(--c);
    padding: 0 3px; border-radius: 2px;
  }

  /* Panel */
  .panel {
    grid-row: 3; grid-column: 2;
    border: 1.5px solid #d4c4f0; border-radius: 0 0 8px 8px;
    background: #fff; margin: -1px 0 0;
    box-shadow: 0 2px 6px rgba(85,48,168,0.04);
  }
  .panel.dashed { border-style: dashed; }

  .ph {
    display: flex; align-items: center;
    padding: 4px 8px; background: linear-gradient(180deg, #f8f6ff 0%, #f3f0ff 100%);
    border-bottom: 1px solid #e8e2f6; border-left: 2px solid var(--c);
  }
  .pht { font-size: 9.5px; font-weight: 600; color: #1a1a1a; flex: 1; letter-spacing: 0.01em; }
  .phx { font-size: 11px; color: #bbb; cursor: pointer; border: none; background: none; line-height: 1; border-radius: 3px; padding: 0 2px; }
  .phx:hover { color: #666; background: rgba(0,0,0,0.04); }

  .pb { padding: 2px 8px 4px; }
  .pr { display: flex; align-items: center; justify-content: space-between; padding: 3px 0; border-bottom: 1px solid rgba(85,48,168,0.03); }
  .pr:last-child { border-bottom: none; }
  .pn { font-size: 9.5px; color: #444; }
  .pt {
    font-family: var(--font-mono); font-size: 7px; font-weight: 600;
    padding: 1px 5px; border-radius: 3px;
    background: linear-gradient(180deg, #f4f4f4 0%, #ebebeb 100%);
    color: #777; border: 0.5px solid #ddd;
  }
  .pt.fn { background: linear-gradient(180deg, #f3eefb 0%, #ede7ff 100%); color: var(--c); border-color: #d4c4f0; }
  .pt.err { background: linear-gradient(180deg, #fff5f5 0%, #fef2f2 100%); color: #dc2626; border-color: #fecaca; }
  .pm { font-size: 8px; color: var(--c); font-style: italic; opacity: 0.5; padding-top: 2px; }
  .pa { font-size: 8px; color: var(--c); cursor: pointer; opacity: 0.35; padding: 2px 0; transition: opacity 0.1s; }
  .pa:hover { opacity: 0.8; }
</style>
