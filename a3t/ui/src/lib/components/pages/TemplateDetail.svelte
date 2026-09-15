<script lang="ts">
  import { mockTemplates } from '$lib/mock/data';
  let { name }: { name: string } = $props();
  let template = $derived(mockTemplates.find(t => t.name === name) || mockTemplates[0]);

  const specYaml = $derived(`# AgentTemplate · ${template.name} · v${template.activeVersion}
apiVersion: workflow.a3t.io/v1alpha1
kind: AgentTemplate
metadata:
  name: ${template.name}
  labels:
    version: v${template.activeVersion}

spec:
  document:
    dsl: "1.0.0"
    name: ${template.name}
    version: "${template.activeVersion}.0"

  use:
    resources:
      llm: {{ model_endpoint }}
      mcp:
        - tools-registry-mcp
        - ctx-scorer-mcp
      memory: adb-primary

  durability: checkpoint
  budget: 80ms

  do:
    - classify_url:
        call: mcp
        with: { server: ctx-scorer-mcp }
    - win_history:
        call: aql
        checkpoint: true
    - compute_bid:
        call: llm
        checkpoint: true
    - emit_bid:
        emit:
          topic: {{ output_topic }}`);
</script>

<!-- Screen header -->
<div class="screen-header">
  <div class="breadcrumb">
    Templates <span class="sep">/</span> {template.name.split('-').join(' / ')} <span class="sep">/</span> <span style="color: var(--text-2);">{template.name.split('-').map(w => w[0].toUpperCase() + w.slice(1)).join(' ')}</span>
  </div>
  <div class="screen-title-row">
    <div class="title-block">
      <div class="title-icon" style="background: {template.color}">{template.icon}</div>
      <div>
        <h1 class="screen-title">{template.name.split('-').map(w => w[0].toUpperCase() + w.slice(1)).join(' ')}</h1>
        <div class="meta-line">
          <span class="kind-badge spec"><span class="kb-k">kind:</span><span class="kb-v">AgentTemplate</span></span>
          <span>v{template.activeVersion}</span>
          <span>&middot; {template.usedBy} agents instantiated</span>
        </div>
      </div>
    </div>
    <div class="screen-actions">
      <button class="btn ghost sm">Lint</button>
      <button class="btn ghost">View in registry</button>
      <button class="btn">Tag v{template.activeVersion}</button>
      <button class="btn spec">Publish</button>
    </div>
  </div>
</div>

<div class="layer-banner template">
  <div class="layer-banner-ico">T</div>
  <div class="layer-banner-text">
    <p><b>This template is a CNCF Serverless Workflow YAML spec.</b> It compiles to a versioned, portable artifact in the template registry. Templates are immutable once tagged — agents reference them by version tag.</p>
  </div>
  <div class="layer-banner-mapping">
    AgentTemplate <span class="arrow">&rarr;</span> CNCF Serverless Workflow <span class="arrow">&rarr;</span> OCI-signed blob
  </div>
</div>

<div class="screen-body">
  <!-- Spec meta strip -->
  <div class="spec-meta">
    <div class="sm-field"><span class="sm-label">version</span><span class="sm-value">v{template.activeVersion}</span><span class="badge ok" style="font-size: 10px;"><span class="dot"></span>tagged</span></div>
    <div class="sm-sep"></div>
    <div class="sm-field"><span class="sm-label">agents</span><span class="sm-value">{template.usedBy} instantiated</span></div>
    <div class="sm-sep"></div>
    <div class="sm-field"><span class="sm-label">tags</span><span class="sm-value">{template.tags.length}</span></div>
  </div>

  <div class="template-split">
    <!-- LEFT: YAML spec -->
    <div class="spec-card">
      <div class="spec-card-head">
        <div>
          <div class="spec-card-title">agent-template.yaml</div>
          <div class="spec-card-sub">the source of truth · CNCF Serverless Workflow 1.0</div>
        </div>
        <div style="margin-left: auto; display: flex; gap: 6px;">
          <button class="btn ghost sm">Copy</button>
          <button class="btn ghost sm">Edit</button>
        </div>
      </div>
      <div class="yaml-view"><pre>{specYaml}</pre></div>
    </div>

    <!-- RIGHT: versions + info -->
    <div class="right-col">
      <div class="spec-card">
        <div class="spec-card-head">
          <div>
            <div class="spec-card-title">Versions</div>
            <div class="spec-card-sub">tagged releases in template registry</div>
          </div>
          <button class="btn ghost sm" style="margin-left: auto;">Diff</button>
        </div>
        <div style="padding: 18px 20px;">
          <div class="version-list">
            {#each template.versions.slice().reverse() as v}
              <div class="version-row" class:current={v === template.activeVersion}>
                <span class="ver-tag" class:ver-active={v === template.activeVersion}>{v}</span>
                <div class="ver-row-info">
                  <div class="ver-msg">{v === template.activeVersion ? 'Current active version' : 'Previous version'}</div>
                  <div class="ver-meta">{v === template.activeVersion ? `${template.usedBy} agents on this version` : '0 agents'}</div>
                </div>
                {#if v === template.activeVersion}
                  <span class="badge ok"><span class="dot"></span>current</span>
                {:else}
                  <button class="btn ghost sm">View</button>
                {/if}
              </div>
            {/each}
          </div>
        </div>
      </div>

      <div class="spec-card">
        <div class="spec-card-head">
          <div>
            <div class="spec-card-title">Tags</div>
            <div class="spec-card-sub">capability surface markers</div>
          </div>
        </div>
        <div style="padding: 18px 20px; display: flex; flex-wrap: wrap; gap: 6px;">
          {#each template.tags as tag}
            <span class="badge mut mono">{tag}</span>
          {/each}
        </div>
      </div>
    </div>
  </div>
</div>

<style>
  .screen-header {
    padding: 22px 40px 14px;
    border-bottom: 1px solid var(--border);
    background: var(--bg-page);
  }
  .breadcrumb { font-size: 12px; color: var(--text-3); margin-bottom: 10px; font-family: var(--font-mono); }
  .breadcrumb .sep { color: var(--text-5); }
  .screen-title-row { display: flex; align-items: flex-end; gap: 18px; }
  .title-block { display: flex; align-items: center; gap: 14px; flex: 1; }
  .title-icon {
    width: 44px; height: 44px; border-radius: 10px;
    display: grid; place-items: center;
    font-family: var(--font-serif); font-size: 22px; font-weight: 500; color: white;
  }
  .screen-title { font-family: var(--font-serif); font-size: 28px; font-weight: 500; letter-spacing: -0.02em; line-height: 1.1; }
  .meta-line { display: flex; align-items: center; gap: 10px; margin-top: 6px; font-family: var(--font-mono); font-size: 11px; color: var(--text-3); }
  .screen-actions { display: flex; gap: 8px; align-items: center; }

  .layer-banner {
    display: grid; grid-template-columns: 40px 1fr auto; gap: 16px;
    padding: 12px 40px; border-bottom: 1px solid var(--border); align-items: center;
  }
  .layer-banner.template { background: linear-gradient(90deg, var(--spec-bg) 0%, transparent 60%); }
  .layer-banner-ico { width: 32px; height: 32px; border-radius: 7px; display: grid; place-items: center; color: white; font-family: var(--font-mono); font-size: 12px; font-weight: 600; background: var(--spec); }
  .layer-banner-text p { font-size: 13px; line-height: 1.5; color: var(--spec-ink); }
  .layer-banner-text b { font-weight: 600; }
  .layer-banner-mapping { font-family: var(--font-mono); font-size: 10.5px; color: var(--text-3); display: flex; align-items: center; gap: 6px; padding: 4px 10px; background: rgba(255,255,255,0.7); border-radius: 5px; border: 1px solid rgba(0,0,0,0.05); }
  .layer-banner-mapping .arrow { color: var(--text-4); }

  .screen-body { flex: 1; overflow: auto; padding: 22px 40px 24px; }

  .spec-meta {
    background: var(--bg-surface); border: 1px solid var(--border); border-radius: var(--r);
    padding: 14px 20px; margin-bottom: 20px; display: flex; gap: 24px; align-items: center; box-shadow: var(--shadow-sm);
  }
  .sm-field { display: flex; align-items: center; gap: 8px; font-size: 12px; }
  .sm-label { color: var(--text-3); font-size: 11px; }
  .sm-value { color: var(--text-1); font-family: var(--font-mono); font-weight: 500; }
  .sm-sep { width: 1px; height: 16px; background: var(--border); }

  .template-split { display: grid; grid-template-columns: 1.1fr 1fr; gap: 22px; }

  .spec-card {
    background: var(--bg-surface); border: 1px solid var(--border); border-radius: var(--r);
    overflow: hidden; box-shadow: var(--shadow-sm); display: flex; flex-direction: column;
  }
  .spec-card-head { padding: 14px 20px; border-bottom: 1px solid var(--border); display: flex; align-items: center; gap: 12px; }
  .spec-card-title { font-family: var(--font-serif); font-size: 16px; font-weight: 500; letter-spacing: -0.01em; }
  .spec-card-sub { font-family: var(--font-mono); font-size: 11px; color: var(--text-3); }

  .yaml-view {
    flex: 1; background: var(--bg-ivory); font-family: var(--font-mono); font-size: 12px;
    line-height: 1.75; white-space: pre; padding: 20px 24px; overflow: auto; color: var(--text-1);
  }
  .yaml-view pre { margin: 0; font-family: inherit; }

  .right-col { display: flex; flex-direction: column; gap: 20px; }

  .version-list { display: flex; flex-direction: column; gap: 8px; }
  .version-row {
    background: var(--bg-surface); border: 1px solid var(--border); border-radius: var(--r-sm);
    padding: 12px 16px; display: flex; align-items: center; gap: 14px;
  }
  .version-row.current { border-color: var(--ok); background: linear-gradient(90deg, var(--ok-bg) 0%, var(--bg-surface) 40%); }
  .ver-tag {
    font-family: var(--font-mono); font-size: 12px; font-weight: 600;
    padding: 3px 9px; border-radius: 4px; background: var(--bg-subtle);
  }
  .ver-tag.ver-active { background: var(--ok); color: white; }
  .ver-row-info { flex: 1; }
  .ver-msg { font-size: 13px; color: var(--text-1); font-weight: 500; }
  .ver-meta { font-family: var(--font-mono); font-size: 10.5px; color: var(--text-3); margin-top: 2px; }
</style>
