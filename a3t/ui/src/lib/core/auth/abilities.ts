/**
 * Role-Based Abilities for A3T
 *
 * Permissions map derived from the backend RBAC schema:
 *   superadmin — full platform-level access to all resources
 *   admin      — full access to all resources within org
 *   operator   — read/execute/deploy on most, full CRUD on agents
 *   viewer     — read-only on templates, agents, canvas, resources
 */

import type { UserRole, Action, ResourceType } from './types';

// Permission matrix: role → resource → allowed actions
const ABILITIES: Record<UserRole, Record<ResourceType, Action[]>> = {
  superadmin: {
    template:  ['create', 'read', 'update', 'delete', 'execute', 'deploy'],
    agent:     ['create', 'read', 'update', 'delete', 'execute', 'deploy'],
    canvas:    ['create', 'read', 'update', 'delete', 'execute', 'deploy'],
    resource:  ['create', 'read', 'update', 'delete'],
    flow:      ['create', 'read', 'update', 'delete', 'execute'],
    memory:    ['create', 'read', 'update', 'delete'],
    knowledge: ['create', 'read', 'update', 'delete'],
    runtime:   ['read', 'execute'],
    logs:      ['read'],
    settings:  ['create', 'read', 'update', 'delete'],
  },
  admin: {
    template:  ['create', 'read', 'update', 'delete', 'execute', 'deploy'],
    agent:     ['create', 'read', 'update', 'delete', 'execute', 'deploy'],
    canvas:    ['create', 'read', 'update', 'delete', 'execute', 'deploy'],
    resource:  ['create', 'read', 'update', 'delete'],
    flow:      ['create', 'read', 'update', 'delete', 'execute'],
    memory:    ['read'],
    knowledge: ['create', 'read', 'update', 'delete'],
    runtime:   ['read', 'execute'],
    logs:      ['read'],
    settings:  ['read', 'update'],
  },
  operator: {
    template:  ['read', 'deploy'],
    agent:     ['create', 'read', 'update', 'delete', 'execute'],
    canvas:    ['read', 'deploy'],
    resource:  ['read'],
    flow:      ['read', 'execute'],
    memory:    ['read'],
    knowledge: ['read'],
    runtime:   ['read', 'execute'],
    logs:      ['read'],
    settings:  ['read'],
  },
  viewer: {
    template:  ['read'],
    agent:     ['read'],
    canvas:    ['read'],
    resource:  ['read'],
    flow:      ['read'],
    memory:    ['read'],
    knowledge: ['read'],
    runtime:   ['read'],
    logs:      ['read'],
    settings:  [],
  },
};

/**
 * Check if a role can perform an action on a resource.
 */
export function can(role: UserRole, action: Action, resource: ResourceType): boolean {
  const allowed = ABILITIES[role]?.[resource];
  return allowed ? allowed.includes(action) : false;
}

/**
 * Check if a role can perform any of the given actions on a resource.
 */
export function canAny(role: UserRole, actions: Action[], resource: ResourceType): boolean {
  return actions.some(a => can(role, a, resource));
}

/**
 * Get all allowed actions for a role on a resource.
 */
export function getAllowedActions(role: UserRole, resource: ResourceType): Action[] {
  return ABILITIES[role]?.[resource] ?? [];
}
