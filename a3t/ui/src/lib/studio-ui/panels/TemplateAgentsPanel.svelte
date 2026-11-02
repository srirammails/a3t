<script lang="ts">
  /// The agents built from an agent template (bottom region): what each sets
  /// itself, and its state. Click one to open it.
  import { agentModel, type Doc, type Workspace } from '$lib/studio-core';

  let { doc, workspace }: { doc: Doc | null; workspace: Workspace } = $props();
  const agents = $derived((doc?.data?.agents ?? []) as any[]);
  const spec = $derived((doc?.text ?? '') as string);

  function open(a: any) {
    workspace.open({ project: a.namespace ?? doc!.ref.project, kind: 'agent', name: a.name ?? a.id, id: a.id });
  }
  const show = (v: unknown) => (typeof v === 'string' ? v : JSON.stringify(v));
</script>

{#if !doc}
  <div class="note">Open an agent template.</div>
{:else if !agents.length}
  <div class="note">No agents are built from this template yet.</div>
{:else}
  <table>
    <thead><tr><th>Agent</th><th>Project</th><th>Status</th><th>Mode</th><th>Sets itself</th><th>Missing (required)</th></tr></thead>
    <tbody>
      {#each agents as a (a.id)}
        {@const own = agentModel.agentValues(a)}
        {@const need = agentModel.missing(spec, own)}
        <tr onclick={() => open(a)} title="Open {a.name ?? a.id}">
          <td class="nm">◉ {a.name ?? a.id}</td>
          <td class="mono">{a.namespace ?? '—'}</td>
          <td><span class="pill {a.status}">{a.status ?? '—'}</span></td>
          <td>{a.mode ?? '—'}</td>
          <td class="own">{#each Object.entries(own) as [k, v] (k)}<span class="kv" title={show(v)}>{k}</span>{/each}</td>
          <td class="need">{need.join(', ') || '—'}</td>
        </tr>
      {/each}
    </tbody>
  </table>
{/if}

<style>
  table { border-collapse: collapse; width: 100%; font-size: 12.5px; }
  th, td { text-align: left; padding: 6px 10px; border-bottom: 1px solid var(--border); vertical-align: top; }
  th { font-size: 11px; text-transform: uppercase; letter-spacing: 0.04em; color: var(--text-3); font-weight: 500; position: sticky; top: 0; background: var(--bg-surface); }
  tbody tr { cursor: pointer; }
  tbody tr:hover { background: var(--bg-subtle); }
  .nm { font-weight: 500; white-space: nowrap; }
  .mono { font-family: var(--font-mono); font-size: 11.5px; }
  .pill { font-size: 10.5px; padding: 1px 7px; border-radius: 10px; background: var(--bg-subtle); color: var(--text-2); }
  .pill.active { background: var(--runtime-bg, #e3f4ef); color: var(--runtime, #1a7158); }
  .own { display: flex; flex-wrap: wrap; gap: 4px; }
  .kv { font-family: var(--font-mono); font-size: 10.5px; padding: 1px 6px; border-radius: 3px; background: var(--spec-bg); color: var(--spec-ink, var(--spec)); }
  .need { color: var(--err, #b42318); font-family: var(--font-mono); font-size: 11.5px; }
  .note { padding: 14px; color: var(--text-4); font-size: 12.5px; }
</style>
