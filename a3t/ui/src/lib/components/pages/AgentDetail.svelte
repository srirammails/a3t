<script lang="ts">
  import { mockAgents, mockLogs } from '$lib/mock/data';
  let { id }: { id: string } = $props();
  let agent = $derived(mockAgents.find(a => a.agent_id === id) || mockAgents[0]);
</script>

<div class="screen-header">
  <div class="breadcrumb">
    Agents <span class="sep">/</span> rtb <span class="sep">/</span> <span style="color: var(--text-2);">{agent.agent_id}</span> <span class="sep">/</span> process
  </div>
  <div class="screen-title-row">
    <div class="title-block">
      <div>
        <h1 class="screen-title">{agent.agent_id}</h1>
        <div class="meta-line">
          <span class="kind-badge runtime"><span class="kb-k">kind:</span><span class="kb-v">Agent</span></span>
          <span>from template {agent.template}</span>
          <span style="color: var(--text-5);">&middot;</span>
          <span>pod {agent.pod}</span>
          <span style="color: var(--text-5);">&middot;</span>
          <span>node {agent.node}</span>
        </div>
      </div>
    </div>
    <div class="screen-actions">
      <button class="btn ghost sm">kubectl describe</button>
      <button class="btn ghost">Logs</button>
      <button class="btn">Exec shell</button>
      <button class="btn runtime">
        <svg width="12" height="12" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.8"><path d="M3 3l3 5-3 5M8 13h5"/></svg>
        Attach debugger
      </button>
    </div>
  </div>
</div>

<div class="screen-body">
  <div class="agent-split">
    <div class="agent-left">
      <!-- Process card -->
      <div class="process-card">
        <div class="process-indicator">&beta;</div>
        <div class="process-details">
          <div class="process-name">
            <span>{agent.agent_id}</span>
            <span class="badge ok"><span class="dot"></span>Running</span>
            <span class="badge mut mono">PID {agent.pid}</span>
          </div>
          <div class="process-facts">
            <span>uptime <b>{agent.uptime}</b></span>
            <span>restarts <b>{agent.restarts}</b></span>
            <span>image <b>{agent.image}</b></span>
            <span>replicas <b>{agent.replicas}</b></span>
          </div>
          <div class="process-facts" style="margin-top: 2px;">
            <span>cpu <b>{agent.cpu}</b></span>
            <span>mem <b>{agent.memory}</b></span>
            <span>ip <b>{agent.ip}</b></span>
          </div>
        </div>
      </div>

      <!-- Lifecycle -->
      <div class="lifecycle-bar">
        <span class="lb-label">Lifecycle</span>
        <button class="lifecycle-btn pause">
          <svg width="11" height="11" viewBox="0 0 16 16" fill="currentColor"><rect x="4" y="2" width="3" height="12" rx="1"/><rect x="9" y="2" width="3" height="12" rx="1"/></svg>
          Pause all instances
        </button>
        <button class="lifecycle-btn step">
          <svg width="11" height="11" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="2"><path d="M3 3l5 5-5 5M10 3v10"/></svg>
          Step one
        </button>
        <button class="lifecycle-btn">
          <svg width="11" height="11" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5"><circle cx="8" cy="8" r="6"/><path d="M8 4v4l3 1.5"/></svg>
          Restart gracefully
        </button>
        <button class="lifecycle-btn stop" style="margin-left: auto;">
          <svg width="11" height="11" viewBox="0 0 16 16" fill="currentColor"><rect x="3" y="3" width="10" height="10" rx="1"/></svg>
          Stop &amp; evict
        </button>
      </div>

      <!-- Metrics strip -->
      <div class="metrics-strip">
        <div class="ms-cell">
          <div class="ms-label">Inflight</div>
          <div class="ms-value">{agent.metrics.inflight}</div>
          <svg class="ms-spark" viewBox="0 0 120 20"><polyline points="0,12 15,10 30,14 45,8 60,11 75,6 90,9 105,5 120,7" fill="none" stroke="var(--ok)" stroke-width="1.2"/></svg>
        </div>
        <div class="ms-cell">
          <div class="ms-label">Completed &middot; 1h</div>
          <div class="ms-value">{(agent.metrics.completed_1h / 1000).toFixed(0)}<small>k</small></div>
          <svg class="ms-spark" viewBox="0 0 120 20"><polyline points="0,14 15,12 30,13 45,8 60,9 75,10 90,6 105,7 120,4" fill="none" stroke="var(--ephemeral)" stroke-width="1.2"/></svg>
        </div>
        <div class="ms-cell">
          <div class="ms-label">p99 latency</div>
          <div class="ms-value">{agent.metrics.p99_latency_ms}<small>ms</small></div>
          <svg class="ms-spark" viewBox="0 0 120 20"><polyline points="0,6 15,8 30,7 45,9 60,5 75,11 90,8 105,7 120,10" fill="none" stroke="var(--warn)" stroke-width="1.2"/></svg>
        </div>
        <div class="ms-cell">
          <div class="ms-label">Errors &middot; 1h</div>
          <div class="ms-value">{agent.metrics.errors_1h}</div>
          <svg class="ms-spark" viewBox="0 0 120 20"><polyline points="0,16 15,17 30,15 45,16 60,13 75,14 90,11 105,13 120,10" fill="none" stroke="var(--err)" stroke-width="1.2"/></svg>
        </div>
      </div>

      <!-- Logs -->
      <div class="card">
        <div class="card-header">
          <div>
            <div class="card-title">Container logs</div>
            <div class="card-sub">stdout &middot; last 60s &middot; streaming</div>
          </div>
          <div style="margin-left: auto; display: flex; gap: 6px; align-items: center;">
            <span class="badge ok"><span class="dot"></span>live</span>
            <button class="btn ghost sm">Filter</button>
            <button class="btn ghost sm">Open full</button>
          </div>
        </div>
        <div class="log-console">
          {#each mockLogs as log}
            <div class="log-line">
              <span class="log-ts">{log.ts}</span>
              <span class="log-lvl {log.level.toLowerCase()}">{log.level}</span>
              <span class="log-msg">{log.msg}</span>
            </div>
          {/each}
        </div>
      </div>
    </div>

    <!-- Right aside -->
    <div class="agent-aside">
      <div class="aside-section">
        <div class="aside-title">Process facts</div>
        <dl class="aside-kv">
          <dt>kind</dt><dd>Agent</dd>
          <dt>mode</dt><dd>{agent.mode}</dd>
          <dt>pod</dt><dd>{agent.pod}</dd>
          <dt>node</dt><dd>{agent.node}</dd>
          <dt>ip</dt><dd>{agent.ip}</dd>
          <dt>image</dt><dd>{agent.image}</dd>
          <dt>pid</dt><dd>{agent.pid}</dd>
        </dl>
      </div>
      <div class="aside-section">
        <div class="aside-title">Instantiated from</div>
        <dl class="aside-kv">
          <dt>template</dt><dd>{agent.template}</dd>
          <dt>tools</dt><dd>{agent.tools_count} bound</dd>
          <dt>memory</dt><dd>{agent.has_memory ? 'enabled' : 'disabled'}</dd>
          <dt>history</dt><dd>{(agent.history_len / 1000).toFixed(0)}k turns</dd>
        </dl>
      </div>
      {#if agent.canvas}
        <div class="aside-section">
          <div class="aside-title">Deployed in canvas</div>
          <dl class="aside-kv">
            <dt>canvas</dt><dd>{agent.canvas}</dd>
          </dl>
        </div>
      {/if}
    </div>
  </div>
</div>

<style>
  .screen-header { padding: 22px 40px 14px; border-bottom: 1px solid var(--border); background: var(--bg-page); }
  .breadcrumb { font-size: 12px; color: var(--text-3); margin-bottom: 10px; font-family: var(--font-mono); }
  .breadcrumb .sep { color: var(--text-5); }
  .screen-title-row { display: flex; align-items: flex-end; gap: 18px; }
  .title-block { flex: 1; }
  .screen-title { font-family: var(--font-serif); font-size: 28px; font-weight: 500; letter-spacing: -0.02em; }
  .meta-line { display: flex; align-items: center; gap: 10px; margin-top: 6px; font-family: var(--font-mono); font-size: 11px; color: var(--text-3); }
  .screen-actions { display: flex; gap: 8px; align-items: center; }

  .screen-body { flex: 1; overflow: auto; padding: 22px 40px 28px; }

  .agent-split { display: grid; grid-template-columns: 1fr 320px; gap: 22px; }
  .agent-left { display: flex; flex-direction: column; gap: 18px; }

  /* Process card */
  .process-card { background: var(--bg-surface); border: 1px solid var(--border); border-radius: var(--r); padding: 20px 24px; box-shadow: var(--shadow-sm); display: grid; grid-template-columns: auto 1fr; gap: 20px; align-items: center; }
  .process-indicator { width: 64px; height: 64px; border-radius: var(--r); background: var(--runtime-bg); color: var(--runtime-ink); display: grid; place-items: center; font-family: var(--font-serif); font-size: 28px; font-weight: 500; position: relative; }
  .process-indicator::after { content: ''; position: absolute; bottom: 6px; right: 6px; width: 12px; height: 12px; border-radius: 50%; background: var(--ok); box-shadow: 0 0 0 3px var(--bg-surface); animation: run-pulse 2s infinite; }
  @keyframes run-pulse { 0%, 100% { opacity: 1; } 50% { opacity: 0.5; } }
  .process-details { display: flex; flex-direction: column; gap: 6px; }
  .process-name { font-family: var(--font-serif); font-size: 22px; font-weight: 500; letter-spacing: -0.015em; display: flex; align-items: center; gap: 12px; }
  .process-facts { display: flex; gap: 16px; flex-wrap: wrap; font-family: var(--font-mono); font-size: 11.5px; color: var(--text-3); }
  .process-facts b { font-weight: 500; color: var(--text-1); }

  /* Lifecycle */
  .lifecycle-bar { background: var(--bg-surface); border: 1px solid var(--border); border-radius: var(--r); padding: 14px 18px; display: flex; align-items: center; gap: 12px; box-shadow: var(--shadow-sm); }
  .lb-label { font-size: 12px; color: var(--text-3); text-transform: uppercase; letter-spacing: 0.05em; font-weight: 600; }
  .lifecycle-btn { padding: 7px 14px; border-radius: var(--r-sm); font-size: 12px; font-weight: 500; border: 1px solid var(--border); background: var(--bg-surface); color: var(--text-2); cursor: pointer; display: inline-flex; align-items: center; gap: 6px; }
  .lifecycle-btn:hover { background: var(--bg-hover); color: var(--text-1); }
  .lifecycle-btn.pause { color: var(--checkpoint-ink); }
  .lifecycle-btn.pause:hover { border-color: var(--checkpoint); background: var(--checkpoint-bg); }
  .lifecycle-btn.stop { color: var(--err); }
  .lifecycle-btn.stop:hover { border-color: var(--err); background: var(--err-bg); }
  .lifecycle-btn.step { color: var(--ephemeral-ink); }
  .lifecycle-btn.step:hover { border-color: var(--ephemeral); background: var(--ephemeral-bg); }

  /* Metrics */
  .metrics-strip { background: var(--bg-surface); border: 1px solid var(--border); border-radius: var(--r); overflow: hidden; box-shadow: var(--shadow-sm); display: grid; grid-template-columns: repeat(4, 1fr); }
  .ms-cell { padding: 16px 20px; border-right: 1px solid var(--divider); }
  .ms-cell:last-child { border-right: 0; }
  .ms-label { font-size: 11px; color: var(--text-3); margin-bottom: 6px; }
  .ms-value { font-family: var(--font-serif); font-size: 22px; font-weight: 500; letter-spacing: -0.01em; line-height: 1; }
  .ms-value small { font-family: var(--font-sans); font-size: 11px; color: var(--text-3); margin-left: 3px; }
  .ms-spark { height: 20px; margin-top: 8px; }

  /* Card + logs */
  .card { background: var(--bg-surface); border: 1px solid var(--border); border-radius: var(--r); overflow: hidden; box-shadow: var(--shadow-sm); }
  .card-header { padding: 14px 20px; border-bottom: 1px solid var(--border); display: flex; align-items: center; gap: 12px; }
  .card-title { font-family: var(--font-serif); font-size: 16px; font-weight: 500; letter-spacing: -0.01em; }
  .card-sub { font-family: var(--font-mono); font-size: 11px; color: var(--text-3); }

  .log-console { background: #19181c; color: #d4d1cc; font-family: var(--font-mono); font-size: 11.5px; line-height: 1.6; padding: 14px 18px; overflow-x: auto; max-height: 280px; overflow-y: auto; }
  .log-line { display: grid; grid-template-columns: 90px 60px 1fr; gap: 10px; padding: 2px 0; }
  .log-ts { color: #7a756c; }
  .log-lvl { font-weight: 600; }
  .log-lvl.info { color: #7aa2ff; }
  .log-lvl.warn { color: #f6a93b; }
  .log-lvl.error { color: #ff6b6b; }
  .log-lvl.debug { color: #5a5e68; }

  /* Aside */
  .agent-aside { display: flex; flex-direction: column; gap: 18px; }
  .aside-section { background: var(--bg-surface); border: 1px solid var(--border); border-radius: var(--r); padding: 16px 18px; box-shadow: var(--shadow-sm); }
  .aside-title { font-size: 11px; color: var(--text-3); text-transform: uppercase; letter-spacing: 0.05em; font-weight: 600; margin-bottom: 12px; }
  .aside-kv { display: grid; grid-template-columns: auto 1fr; gap: 6px 14px; font-size: 12.5px; }
  .aside-kv dt { color: var(--text-3); font-size: 11.5px; }
  .aside-kv dd { color: var(--text-1); text-align: right; font-family: var(--font-mono); font-size: 11.5px; word-break: break-all; }
</style>
