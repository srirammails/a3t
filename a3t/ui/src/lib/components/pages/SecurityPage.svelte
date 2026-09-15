<script lang="ts">
  // Security — a single nav entry with an in-page rail (like Resources) that
  // switches between the Policies and Identity surfaces. Both are EE/admin
  // features; each sub-page handles its own edition/role gating.
  import PoliciesPage from './PoliciesPage.svelte';
  import IdentityPage from './IdentityPage.svelte';

  const SECTIONS = [
    {
      key: 'identity', label: 'Identity',
      icon: '<rect x="1.5" y="3" width="13" height="10" rx="1.5"/><circle cx="5.5" cy="7.5" r="1.5"/><path d="M3.5 11c.3-1.2 1.1-1.8 2-1.8s1.7.6 2 1.8"/><path d="M9.5 6.5h3.5M9.5 9h3"/>',
    },
    {
      key: 'policies', label: 'Policies',
      icon: '<path d="M8 1l5.5 2.2v4c0 3.5-2.3 6.4-5.5 7.6C4.8 13.6 2.5 10.7 2.5 7.2v-4L8 1z"/><path d="M5.8 8l1.6 1.6L10.4 6.5"/>',
    },
  ];

  let active = $state<'policies' | 'identity'>('identity');
</script>

<div class="security">
  <nav class="sec-rail">
    <div class="rail-title">Security</div>
    {#each SECTIONS as s}
      <button class="sec-item" class:active={active === s.key} onclick={() => (active = s.key as typeof active)}>
        <svg class="sec-ico" width="16" height="16" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5">
          {@html s.icon}
        </svg>
        <span class="sec-label">{s.label}</span>
      </button>
    {/each}
  </nav>

  <div class="sec-body">
    {#if active === 'policies'}
      <PoliciesPage />
    {:else}
      <IdentityPage />
    {/if}
  </div>
</div>

<style>
  .security { display: grid; grid-template-columns: 200px 1fr; height: 100%; overflow: hidden; }
  .sec-rail {
    border-right: 1px solid var(--border);
    background: var(--bg-surface);
    padding: 14px 10px;
    display: flex; flex-direction: column; gap: 4px;
    overflow-y: auto;
  }
  .rail-title {
    font-size: 11px; text-transform: uppercase; letter-spacing: 0.06em;
    color: var(--text-4); padding: 4px 10px 8px;
  }
  .sec-item {
    display: flex; align-items: center; gap: 10px;
    padding: 9px 11px; border-radius: 7px;
    border: 1px solid transparent; background: transparent;
    color: var(--text-2); font-size: 13.5px; cursor: pointer; text-align: left;
  }
  .sec-item:hover { background: var(--bg-subtle); }
  .sec-item.active { background: var(--bg-subtle); border-color: var(--border); color: var(--text-1); font-weight: 600; }
  .sec-ico { flex: none; }
  .sec-body { overflow: hidden; min-width: 0; }
</style>
