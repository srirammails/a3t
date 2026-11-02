// Panel state management with URL sync
// Each navigation updates both panel state and browser URL.
// On load, URL is read to restore state.

import { contextPrefix, splitContextPath, contextFromUrl } from './context.svelte.ts';

export interface PanelDef {
  id: string;
  type: string;
  title: string;
  icon?: string;
  props: Record<string, any>;
}

let panels = $state<PanelDef[]>([]);
let counter = $state(0);
let initialized = false;

// ── URL mapping ──

const TYPE_TO_PATH: Record<string, string> = {
  dashboard: '/dashboard',
  chat: '/chat',
  projects: '/projects',
  templates: '/templates',
  agents: '/agents',
  canvas: '/canvas',
  triggers: '/triggers',
  resources: '/resources',
  knowledge: '/knowledge',
  memory: '/memory',
  runtime: '/runtime',
  reconciliation: '/controller',
  executions: '/executions',
  logs: '/logs',
  'flow-editor': '/studio',
  'aql-console': '/aql-console',
  'db-console': '/db-console',
  inputs: '/inputs',
  'early-access': '/early-access',
  settings: '/settings',
  security: '/security',
};

const PATH_TO_TYPE: Record<string, { type: string; title: string }> = {
  '/dashboard': { type: 'dashboard', title: 'Dashboard' },
  '/chat': { type: 'chat', title: 'AI Chat' },
  '/projects': { type: 'projects', title: 'Projects' },
  '/templates': { type: 'templates', title: 'Templates' },
  '/agents': { type: 'agents', title: 'Agents' },
  '/canvas': { type: 'canvas', title: 'Canvas' },
  '/triggers': { type: 'triggers', title: 'Triggers' },
  '/resources': { type: 'resources', title: 'Resources' },
  '/knowledge': { type: 'knowledge', title: 'Knowledge' },
  '/memory': { type: 'memory', title: 'Memory' },
  '/runtime': { type: 'runtime', title: 'Runtimes' },
  '/controller': { type: 'reconciliation', title: 'Controller' },
  '/executions': { type: 'executions', title: 'Executions' },
  '/logs': { type: 'logs', title: 'Logs' },
  '/studio': { type: 'flow-editor', title: 'Studio' },
  // Old links.
  '/flow-editor': { type: 'flow-editor', title: 'Studio' },
  '/aql-console': { type: 'aql-console', title: 'AQL Console' },
  '/db-console': { type: 'db-console', title: 'Query Console' },
  '/inputs': { type: 'inputs', title: 'Inputs' },
  '/early-access': { type: 'early-access', title: 'Early Access Requests' },
  '/settings': { type: 'settings', title: 'Settings' },
  '/security': { type: 'security', title: 'Security' },
};

function updateUrl(type: string, props: Record<string, any> = {}) {
  if (typeof window === 'undefined') return;
  const basePath = TYPE_TO_PATH[type] ?? `/${type}`;
  const itemId = props.selectedId ?? props.id;
  const url = contextPrefix() + (itemId ? `${basePath}/${encodeURIComponent(itemId)}` : basePath);

  if (window.location.pathname !== url) {
    window.history.pushState({ type, props }, '', url);
  }
}

// ── Public API ──

export function navigateTo(type: string, title: string, props: Record<string, any> = {}) {
  counter++;
  panels = [{ id: `panel-${counter}`, type, title, icon: type[0]?.toUpperCase(), props }];
  updateUrl(type, props);
}

export function navigateToDetail(type: string, title: string, itemId: string, props: Record<string, any> = {}) {
  counter++;
  const fullProps = { ...props, selectedId: itemId };
  panels = [{ id: `panel-${counter}`, type, title, icon: type[0]?.toUpperCase(), props: fullProps }];
  updateUrl(type, fullProps);
}

/// Point the CURRENT panel at a different item, without replacing it.
///
/// `navigateToDetail` builds a new panel, which remounts the page — wrong when a
/// page changes its own selection (the chat starting a new conversation). Writing
/// `history` directly instead, as the chat used to, left `props.selectedId` stale
/// so the shell no longer knew which item was open and the context list could not
/// highlight it. This keeps the panel identity and updates both props and URL.
export function setActiveSelection(itemId: string) {
  const active = panels[panels.length - 1];
  if (!active) return;
  if (active.props?.selectedId === itemId) return;
  const props = { ...active.props, selectedId: itemId };
  // Same panel `id`, so the rendered page is not torn down and rebuilt.
  panels = [...panels.slice(0, -1), { ...active, props }];
  updateUrl(active.type, props);
}

export function pushPanel(type: string, title: string, props: Record<string, any> = {}) {
  counter++;
  panels = [...panels, { id: `panel-${counter}`, type, title, icon: type[0]?.toUpperCase(), props }];
}

export function popPanel() {
  if (panels.length > 0) {
    panels = panels.slice(0, -1);
    // Update URL to current panel
    const active = panels[panels.length - 1];
    if (active) updateUrl(active.type, active.props);
  }
}

export function replacePanel(type: string, title: string, props: Record<string, any> = {}) {
  if (panels.length > 0) {
    counter++;
    panels = [...panels.slice(0, -1), { id: `panel-${counter}`, type, title, icon: type[0]?.toUpperCase(), props }];
    updateUrl(type, props);
  }
}

export function clearPanels() {
  panels = [];
}

export function getPanels(): PanelDef[] {
  return panels;
}

export function getActivePanel(): PanelDef | null {
  return panels.length > 0 ? panels[panels.length - 1] : null;
}

// ── Initialize from URL on first load ──

function parseCurrentUrl(): { type: string; title: string; props: Record<string, any> } | null {
  if (typeof window === 'undefined') return null;
  // `/p/<project>/chat/…` — the project is the context store's; the rest is ours.
  const path = splitContextPath(window.location.pathname).rest;
  const segments = path.split('/').filter(Boolean);

  // /templates/a3t-chat-agent → type=templates, id=a3t-chat-agent
  if (segments.length >= 2) {
    const route = PATH_TO_TYPE[`/${segments[0]}`];
    if (route) {
      return { ...route, props: { selectedId: decodeURIComponent(segments[1]) } };
    }
  }

  // /templates → type=templates
  const route = PATH_TO_TYPE[path];
  if (route) {
    return { ...route, props: {} };
  }
  return null;
}

export function initFromUrl(): { type: string; title: string; props: Record<string, any> } | null {
  if (typeof window === 'undefined') return null;
  if (initialized) return null;
  initialized = true;

  const route = parseCurrentUrl();
  if (route) {
    navigateTo(route.type, route.title, route.props);
    return route;
  }
  return null;
}

export function setupPopState() {
  if (typeof window === 'undefined') return;

  window.addEventListener('popstate', () => {
    contextFromUrl();
    const route = parseCurrentUrl();
    if (route) {
      counter++;
      panels = [{ id: `panel-${counter}`, type: route.type, title: route.title, icon: route.type[0]?.toUpperCase(), props: route.props }];
    }
  });
}
