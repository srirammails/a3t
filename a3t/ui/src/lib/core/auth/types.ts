/**
 * Auth Types — RBAC for A3T
 * Mirrors the permission model from the flowr backend (a3t_schema_install.py)
 */

export type UserRole = 'superadmin' | 'securityadmin' | 'admin' | 'operator' | 'viewer';

export type Action = 'create' | 'read' | 'update' | 'delete' | 'execute' | 'deploy';

export type ResourceType =
  | 'template'
  | 'agent'
  | 'canvas'
  | 'resource'
  | 'flow'
  | 'memory'
  | 'knowledge'
  | 'runtime'
  | 'logs'
  | 'settings';

export interface Permission {
  resource: ResourceType;
  actions: Action[];
}

export interface MenuItem {
  type: string;
  label: string;
  tag: string | null;
  icon: string;
  roles: UserRole[];
}

export interface MenuGroup {
  label: string;
  roles: UserRole[];
  items: MenuItem[];
}

// Role hierarchy for quick level checks
export const ROLE_HIERARCHY: Record<UserRole, number> = {
  superadmin: 200,
  admin: 100,
  operator: 60,
  viewer: 20,
};

export function hasRoleLevel(userRole: UserRole, requiredRole: UserRole): boolean {
  return ROLE_HIERARCHY[userRole] >= ROLE_HIERARCHY[requiredRole];
}
