<script lang="ts">
  /// What other orgs expose to you, at the end of an app's page: Shared and
  /// Training from the platform orgs, Community from any org. You can read and
  /// run these, and clone flows, resources, triggers and canvases into the
  /// selected project — never change the originals.
  import FoldSection from './FoldSection.svelte';
  import EntityCard from './EntityCard.svelte';
  import { mcpCall, canvasApi, shareApi } from '$lib/services/api';
  import { exposedProjects, getProject, getProjects, roleIn, type ExposedKind } from '$lib/stores/context.svelte.ts';
  import { contextRev_, bumpContext } from '$lib/stores/apps.svelte.ts';
  import { navigateTo } from '$lib/stores/panels.svelte.ts';

  type Kind = 'flow' | 'resource' | 'trigger' | 'canvas' | 'template' | 'agent';
  let { kind, noun }: { kind: Kind; noun: string } = $props();

  type Item = { key: string; name: string; namespace: string; sub: string; cloneKind?: 'flow' | 'resource' | 'trigger'; cloneRef?: string; templateId?: string };
  let items = $state<Item[]>([]);
  let message = $state('');

  const LABEL: Record<ExposedKind, { title: string; hint: string }> = {
    shared: { title: 'Shared', hint: 'Shared by the platform — use, clone or wire' },
    training: { title: 'Training', hint: 'From the platform — examples to learn from and clone' },
    community: { title: 'Community', hint: "Exposed by other orgs — read, clone, run; not editable" },
  };

  const target = $derived(getProject());
  /// Clone into the selected project, when you may change it.
  const canClone = $derived.by(() => {
    const p = getProjects().find((x) => x.name === target);
    const r = p ? roleIn(p) : null;
    return !!p && !p.platform && (p.info ? (p.info.can_admin || ['owner', 'developer'].includes(r ?? '')) : true);
  });

  async function load() {
    const exposed = exposedProjects();
    const out: Item[] = [];
    const rows = (d: any, k: string) => (Array.isArray(d) ? d : d?.[k] ?? []);
    for (const p of exposed) {
      const ns = p.name;
      if (kind === 'flow') {
        const r = await mcpCall('list_flows', { namespace: ns });
        for (const w of r.ok ? rows(r.data, 'workflows') : []) out.push({ key: `${ns}/${w.name}`, name: w.name, namespace: ns, sub: `v${w.version}`, cloneKind: 'flow', cloneRef: w.name });
      } else if (kind === 'resource') {
        const r = await mcpCall('list_resources', { namespace: ns });
        for (const x of r.ok ? rows(r.data, 'resources') : []) if (x.type !== 'canvas' && x.type !== 'project') out.push({ key: `${ns}/${x.name}`, name: x.name, namespace: ns, sub: x.type, cloneKind: 'resource', cloneRef: x.name });
      } else if (kind === 'canvas') {
        const r = await mcpCall('list_resources', { namespace: ns, type_ref: 'canvas' });
        for (const x of r.ok ? rows(r.data, 'resources') : []) out.push({ key: `${ns}/${x.name}`, name: x.name, namespace: ns, sub: 'canvas', cloneKind: 'resource', cloneRef: x.name });
      }
    }
    if (kind === 'trigger') {
      const r = await mcpCall('list_triggers', {});
      const names = new Set(exposed.map((p) => p.name));
      for (const t of r.ok ? rows(r.data, 'triggers') : []) if (names.has(t.namespace)) out.push({ key: t.id, name: t.name, namespace: t.namespace, sub: t.config?.type ?? '', cloneKind: 'trigger', cloneRef: t.id });
    } else if (kind === 'agent') {
      const r = await mcpCall('list_agents', {});
      const names = new Set(exposed.map((p) => p.name));
      for (const a of r.ok ? rows(r.data, 'agents') : []) if (names.has(a.namespace)) out.push({ key: a.id, name: a.name, namespace: a.namespace, sub: `from ${a.template_id ?? 'no template'}` });
    } else if (kind === 'template') {
      const r = await mcpCall('list_agent_templates', {});
      const names = new Set(exposed.map((p) => p.name));
      for (const t of r.ok ? rows(r.data, 'templates') : []) {
        const ns = (t.spec_yaml ?? '').match(/^\s+namespace:\s*['"]?([\w.-]+)/m)?.[1];
        if (ns && names.has(ns)) out.push({ key: t.id, name: t.name, namespace: ns, sub: `v${t.version ?? '1.0.0'}`, templateId: t.id });
      }
    }
    items = out;
  }

  let loadedRev = -1;
  $effect(() => {
    const rev = contextRev_();
    if (rev === loadedRev) return;
    loadedRev = rev;
    void load();
  });

  async function clone(it: Item) {
    if (!it.cloneKind || !it.cloneRef) return;
    message = '';
    const res = await shareApi.copy({ kind: it.cloneKind, from: it.namespace, ...(it.cloneKind === 'trigger' ? { id: it.cloneRef } : { name: it.cloneRef }), to: target });
    message = res.ok ? `Cloned ${it.name} into ${target}.` : (res.error ?? 'Clone failed');
    if (res.ok) bumpContext();
  }

  /// One card per release (name + version): passed on from system to a3t, the
  /// same template is exposed twice; keep the copy your own org released.
  const deduped = $derived.by(() => {
    const mine = getProjects().find((x) => x.name === target)?.info?.owner;
    const ownerOf = (ns: string) => getProjects().find((x) => x.name === ns)?.info?.owner;
    const best = new Map<string, Item>();
    for (const it of items) {
      const k = `${it.name}@${it.sub}`;
      const cur = best.get(k);
      if (!cur || (ownerOf(it.namespace) === mine && ownerOf(cur.namespace) !== mine)) best.set(k, it);
    }
    return [...best.values()];
  });

  const byKind = (k: ExposedKind) => {
    const ns = new Set(exposedProjects(k).map((p) => p.name));
    return deduped.filter((i) => ns.has(i.namespace));
  };
</script>

{#if message}<div class="ex-msg">{message}</div>{/if}
{#each ['shared', 'training', 'community'] as k (k)}
  {@const list = byKind(k as ExposedKind)}
  {@const owners = [...new Set((list.length ? list.map((i) => getProjects().find((p) => p.name === i.namespace)) : exposedProjects(k as ExposedKind)).map((p) => p?.info?.owner).filter(Boolean))]}
  {@const title = k === 'shared' && owners.length ? `${noun} from ${owners.map((o) => o === 'a3t' ? 'A3T' : o === 'system' ? 'System' : o).join(' & ')}` : `${LABEL[k as ExposedKind].title} ${noun}`}
  <FoldSection {title} count={list.length} hint={LABEL[k as ExposedKind].hint} defaultOpen={k !== 'community'}>
    <div class="ex-grid">
      {#each list as it (it.key)}
        <EntityCard
          icon={it.name.slice(0, 2).toUpperCase()}
          color={k === 'community' ? '#0f766e' : '#5530a8'}
          name={it.name}
          description={it.sub}
          tags={[k]}
          meta=""
          project={it.namespace}
          actionLabel={it.templateId ? '+ Create agent' : it.cloneKind ? (canClone ? `Clone into ${target}` : 'Select a project of yours to clone') : ''}
          onclick={() => {}}
          onaction={it.templateId ? () => navigateTo('agents', 'Agents', { createFromTemplate: it.templateId }) : it.cloneKind && canClone ? () => clone(it) : undefined}
        />
      {/each}
    </div>
  </FoldSection>
{/each}

<style>
  .ex-grid { display: grid; grid-template-columns: repeat(auto-fill, minmax(300px, 1fr)); gap: 12px; }
  .ex-msg { padding: 8px 12px; border-radius: var(--r-sm); background: var(--spec-bg); color: var(--spec); font-size: 12.5px; margin-bottom: 12px; }
</style>
