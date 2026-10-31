/**
 * Menu configuration with role-based access control
 *
 * Access rules:
 *   superadmin — sees all menu items (platform-level access)
 *   admin      — sees all menu items
 *   operator   — Build (read-only templates/canvas), Augment (read), Operate (full), Workbench (flow editor)
 *   viewer     — Build (read-only), Augment (read), Operate (logs only), no Workbench
 */

import type { UserRole, MenuGroup } from './types';

const ALL_ROLES: UserRole[] = ['superadmin', 'securityadmin', 'admin', 'operator', 'viewer'];
const ADMIN_PLUS: UserRole[] = ['superadmin', 'securityadmin', 'admin'];
const OPERATOR_PLUS: UserRole[] = ['superadmin', 'admin', 'operator'];

export const MENU_CONFIG: MenuGroup[] = [
  {
    label: 'Home',
    roles: ALL_ROLES,
    items: [
      // Dashboard is hidden for now. The `dashboard` panel type and its
      // /dashboard route still resolve, so a bookmark keeps working.
      { type: 'chat',       label: 'AI Chat',    tag: null, icon: 'chat',      roles: ALL_ROLES },
      { type: 'projects',   label: 'Projects',   tag: null, icon: 'project',   roles: ALL_ROLES },
    ],
  },
  {
    label: 'Build',
    roles: ALL_ROLES,
    items: [
      { type: 'templates',  label: 'Templates',  tag: 'sw/v1', icon: 'grid',     roles: ALL_ROLES },
      { type: 'agents',     label: 'Agents',     tag: 'pods',  icon: 'agent',    roles: ALL_ROLES },
      { type: 'canvas',     label: 'Canvas',     tag: 'crd',   icon: 'canvas',   roles: ALL_ROLES },
      { type: 'triggers',   label: 'Triggers',   tag: null,    icon: 'trigger',  roles: ALL_ROLES },
      { type: 'resources',  label: 'Resources',  tag: null,    icon: 'cabinet',  roles: ALL_ROLES },
      { type: 'security',   label: 'Security',   tag: 'ee',    icon: 'shield',   roles: ADMIN_PLUS },
    ],
  },
  {
    label: 'Workbench',
    roles: ALL_ROLES,
    items: [
      { type: 'flow-editor',      label: 'Flow Editor',      tag: null, icon: 'flow',     roles: ALL_ROLES },
      { type: 'db-console',       label: 'Query Console',    tag: null, icon: 'database', roles: ALL_ROLES },
      { type: 'knowledge',        label: 'Knowledge',        tag: null, icon: 'kg', roles: ALL_ROLES },
    ],
  },
  {
    label: 'System',
    roles: ALL_ROLES,
    items: [
      { type: 'runtime',     label: 'Runtimes',    tag: null, icon: 'runtime', roles: ALL_ROLES },
      { type: 'executions',  label: 'Executions',  tag: null, icon: 'runs', roles: ALL_ROLES },
      { type: 'logs',        label: 'Logs',         tag: null, icon: 'logs', roles: ALL_ROLES },
    ],
  },
  {
    // Own group so a divider renders between Logs and Settings (each group
    // draws a top divider; see Sidebar.svelte).
    label: 'Settings',
    roles: ALL_ROLES,
    items: [
      { type: 'settings',    label: 'Settings',     tag: null, icon: 'settings', roles: ALL_ROLES },
    ],
  },
];

/**
 * Filter menu groups and items for the given role.
 * Removes empty groups after filtering.
 */
export function getMenusForRole(role: UserRole): MenuGroup[] {
  return MENU_CONFIG
    .filter(group => group.roles.includes(role))
    .map(group => ({
      ...group,
      items: group.items.filter(item => item.roles.includes(role)),
    }))
    .filter(group => group.items.length > 0);
}
