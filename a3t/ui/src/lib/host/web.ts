/// The Studio's host in the web dash.
import type { StudioHost } from '$lib/studio-core';
import { navigateTo } from '$lib/stores/panels.svelte.ts';

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
};
