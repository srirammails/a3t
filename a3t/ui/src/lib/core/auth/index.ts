// Auth — RBAC barrel export
export type { UserRole, Action, ResourceType, Permission, MenuItem, MenuGroup } from './types';
export { ROLE_HIERARCHY, hasRoleLevel } from './types';
export { can, canAny, getAllowedActions } from './abilities';
export { MENU_CONFIG, getMenusForRole } from './menu';
