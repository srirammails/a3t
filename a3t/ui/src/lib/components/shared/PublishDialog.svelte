<script lang="ts">
  /// Publish an item into one of your org's exposed projects — a release others
  /// can read, clone and run, never change. Your working copy stays where it is;
  /// publish again to update the release.
  import { publishing, closePublish } from '$lib/stores/publish.svelte.ts';
  import { publishTargets } from '$lib/stores/context.svelte.ts';
  import { bumpContext } from '$lib/stores/apps.svelte.ts';
  import { shareApi, execFlow } from '$lib/services/api';
  import { currentUser } from '$lib/stores/auth';

  const item = $derived(publishing());
  const targets = $derived(publishTargets().filter((p) => p.name !== item?.from));
  let to = $state('');
  let busy = $state(false);
  let message = $state('');
  let ok = $state(false);

  $effect(() => { if (item) { to = targets[0]?.name ?? ''; message = ''; ok = false; } });

  function audienceOf(name: string): string {
    const ex = publishTargets().find((p) => p.name === name)?.info?.policy?.exposure;
    if (!ex) return '';
    const who = ex.audience === 'public' ? 'every org' : `org${(ex.orgs ?? []).length === 1 ? '' : 's'} ${(ex.orgs ?? []).join(', ')}`;
    return `${ex.kind ?? 'community'} · ${who}`;
  }

  async function publish() {
    if (!item || !to) return;
    busy = true; message = '';
    let res: { ok: boolean; error?: string };
    if (item.kind === 'template' && item.template) {
      // A template is published as a copy whose spec lives in the target project.
      const spec = item.template.spec_yaml.replace(/^(\s+namespace:\s*)['"]?[\w.-]+['"]?/m, `$1${to}`);
      res = await execFlow('a3t-templates-create', 'a3t', {
        org_id: 'org-default', user_id: $currentUser?.id ?? '', timestamp: new Date().toISOString(),
        data: { id: `tpl-${to}-${item.template.name}-${Date.now().toString(36)}`, name: item.template.name, version: item.template.version,
                source: 'community', spec_yaml: spec, tags: item.template.tags ?? '', vertical: item.template.vertical ?? '', published: 'true',
                // Publishing again updates the release in that project.
                replace: true },
      });
      // The CRUD flow answers a refusal as { error } rather than failing.
      if (res.ok && (res as any).data?.error) res = { ok: false, error: (res as any).data.error };
    } else {
      res = await shareApi.copy({ kind: item.kind as 'flow' | 'resource' | 'trigger', from: item.from,
        ...(item.kind === 'trigger' ? { id: item.ref } : { name: item.ref }), to });
    }
    busy = false;
    ok = res.ok;
    message = res.ok ? `Published ${item.label} to ${to}.` : (res.error ?? 'Publish failed');
    if (res.ok) bumpContext();
  }
</script>

{#if item}
  <div class="pd-overlay" role="presentation" onclick={closePublish}></div>
  <div class="pd" role="dialog" aria-label="Publish">
    <h3>Publish {item.label}</h3>
    <p class="pd-note">One release in an exposed project. Others use or wire it by reference — they get each new release — or clone their own copy. Your copy in <span class="mono">{item.from}</span> stays yours; publish again to update the release.</p>
    {#if targets.length === 0}
      <p class="pd-warn">None of your projects is exposed yet. In Projects → Policy, set a project's Exposure to selected orgs or all orgs, then publish into it.</p>
    {:else}
      <label>Into
        <select bind:value={to}>
          {#each targets as t (t.name)}<option value={t.name}>{t.name} — {audienceOf(t.name)}</option>{/each}
        </select>
      </label>
      {#if item.kind === 'resource'}<p class="pd-note">A secret it refers to is used by others' runs but never shown to them.</p>{/if}
      {#if item.kind === 'trigger'}<p class="pd-note">The released trigger answers under <span class="mono">/{to}/…</span>.</p>{/if}
    {/if}
    {#if message}<div class="pd-msg" class:ok>{message}</div>{/if}
    <div class="pd-row">
      <button class="pd-btn" onclick={closePublish}>{ok ? 'Done' : 'Cancel'}</button>
      {#if targets.length && !ok}<button class="pd-btn primary" onclick={publish} disabled={busy || !to}>{busy ? 'Publishing…' : 'Publish'}</button>{/if}
    </div>
  </div>
{/if}

<style>
  .pd-overlay { position: fixed; inset: 0; background: rgba(0,0,0,0.15); z-index: 200; }
  .pd { position: fixed; top: 18vh; left: 50%; transform: translateX(-50%); width: min(480px, calc(100vw - 32px)); z-index: 201;
        background: var(--bg-surface); border: 1px solid var(--border); border-radius: var(--r); box-shadow: var(--shadow-lg);
        padding: 20px 22px; display: flex; flex-direction: column; gap: 12px; }
  .pd h3 { margin: 0; font-family: var(--font-serif); font-size: 18px; font-weight: 500; }
  .pd label { display: flex; flex-direction: column; gap: 5px; font-size: 12px; color: var(--text-3); }
  .pd select { padding: 7px 9px; border: 1px solid var(--border); border-radius: var(--r-sm); background: var(--bg-page); color: var(--text-1); font-size: 13px; }
  .pd-note { margin: 0; font-size: 12px; color: var(--text-3); line-height: 1.5; }
  .pd-warn { margin: 0; font-size: 12.5px; color: var(--warn, #b45309); }
  .pd-msg { padding: 8px 12px; border-radius: var(--r-sm); background: var(--err-bg, #fde8e8); color: var(--err, #b42318); font-size: 12.5px; }
  .pd-msg.ok { background: var(--spec-bg); color: var(--spec); }
  .pd-row { display: flex; justify-content: flex-end; gap: 8px; }
  .pd-btn { padding: 7px 14px; border-radius: var(--r-sm); border: 1px solid var(--border); background: var(--bg-surface); font-size: 12.5px; cursor: pointer; }
  .pd-btn.primary { background: var(--spec); border-color: var(--spec); color: white; }
  .pd-btn:disabled { opacity: 0.5; cursor: default; }
  .mono { font-family: var(--font-mono); }
</style>
