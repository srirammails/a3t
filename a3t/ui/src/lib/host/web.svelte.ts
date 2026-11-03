/// The Studio's host in the web dash: browser storage and dialogs, Console
/// navigation, and the project switcher as the Studio's scope.
import { untrack } from 'svelte';
import type { StudioHost } from '$lib/studio-core';
import { navigateTo } from '$lib/stores/panels.svelte.ts';
import { getProject, namespacesFor } from '$lib/stores/context.svelte.ts';
import { contextRev_ } from '$lib/stores/apps.svelte.ts';

export const webHost: StudioHost = {
  storage: {
    get(key) {
      try { return JSON.parse(localStorage.getItem(key) ?? 'null'); } catch { return null; }
    },
    set(key, value) {
      try { localStorage.setItem(key, JSON.stringify(value)); } catch { /* private window */ }
    },
  },
  confirm: async (message) => window.confirm(message),
  clipboard: async (text) => { await navigator.clipboard?.writeText(text); },
  openConsole: (section, title) => navigateTo(section, title),
  scope: {
    projects: () => namespacesFor('owned'),
    current: () => getProject() || null,
    /// The switcher bumps a context revision; follow it outside any component.
    /// `fn` runs untracked: whatever it reads (the API client's own state while it
    /// starts a request) must not become a reason to call it again.
    onChange(fn) {
      let first = true;
      return $effect.root(() => {
        $effect(() => {
          contextRev_();
          if (first) { first = false; return; }
          untrack(fn);
        });
      });
    },
  },
};
