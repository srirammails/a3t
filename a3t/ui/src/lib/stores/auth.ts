// Auth store — JWT-based session management backed by FlowR /auth/* endpoints

import { writable, derived, get } from 'svelte/store';
import * as api from '$lib/services/api';

export interface User {
  id: string;
  email: string;
  name: string;
  role: UserRole;
  org_id?: string;
  avatar?: string;
}

export type UserRole = 'superadmin' | 'securityadmin' | 'admin' | 'operator' | 'viewer';

export interface Session {
  user: User;
  access_token: string;
  refresh_token: string;
  expires_in: number;
}

const STORAGE_KEY = 'a3t_session';

// Core session store
const sessionStore = writable<Session | null>(null);

// Derived stores
export const isAuthenticated = derived(sessionStore, ($s) => !!$s);
export const currentUser = derived(sessionStore, ($s) => $s?.user ?? null);
export const userRole = derived(sessionStore, ($s) => $s?.user?.role ?? null);
export const accessToken = derived(sessionStore, ($s) => $s?.access_token ?? null);

// Initialize from localStorage + sync tokens to API client
export function initAuth(): boolean {
  if (typeof window === 'undefined') return false;
  try {
    const stored = localStorage.getItem(STORAGE_KEY);
    if (!stored) return false;
    const session: Session = JSON.parse(stored);
    sessionStore.set(session);
    // Sync tokens to API client
    api.setTokens(session.access_token, session.refresh_token);
    // Handle token expiry — redirect to login
    api.onTokenExpired(() => signOut());
    return true;
  } catch {
    localStorage.removeItem(STORAGE_KEY);
    return false;
  }
}

// Save session to store + localStorage + API client
function saveSession(session: Session) {
  sessionStore.set(session);
  api.setTokens(session.access_token, session.refresh_token);
  api.onTokenExpired(() => signOut());
  if (typeof window !== 'undefined') {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(session));
  }
}

// ── Login — calls POST /auth/login ──

export async function login(email: string, password: string): Promise<{ ok: boolean; error?: string }> {
  if (!email || !password) return { ok: false, error: 'Email and password required' };

  const res = await api.login(email, password);

  if (res.ok && res.data) {
    const session: Session = {
      user: {
        id: res.data.user.id,
        email: res.data.user.email,
        name: res.data.user.name,
        role: res.data.user.role as UserRole,
        org_id: res.data.user.org_id,
      },
      access_token: res.data.access_token,
      refresh_token: res.data.refresh_token,
      expires_in: res.data.expires_in,
    };
    saveSession(session);
    return { ok: true };
  }

  return { ok: false, error: res.error || 'Login failed' };
}

// ── Signup — calls POST /auth/signup ──

export async function signup(name: string, email: string, password: string): Promise<{ ok: boolean; error?: string }> {
  if (!name || !email || !password) return { ok: false, error: 'All fields required' };
  if (password.length < 8) return { ok: false, error: 'Password must be at least 8 characters' };
  if (!email.includes('@')) return { ok: false, error: 'Invalid email address' };

  const res = await api.signup(name, email, password);

  if (res.ok && res.data) {
    const session: Session = {
      user: {
        id: res.data.user.id,
        email: res.data.user.email,
        name: res.data.user.name,
        role: res.data.user.role as UserRole,
        org_id: res.data.user.org_id,
      },
      access_token: res.data.access_token,
      refresh_token: res.data.refresh_token,
      expires_in: res.data.expires_in,
    };
    saveSession(session);
    return { ok: true };
  }

  return { ok: false, error: res.error || 'Signup failed' };
}

// ── Sign out ──

export function signOut() {
  sessionStore.set(null);
  api.clearTokens();
  if (typeof window !== 'undefined') {
    localStorage.removeItem(STORAGE_KEY);
  }
}

// Get current session (non-reactive)
export function getSession(): Session | null {
  return get(sessionStore);
}

// ── Role hierarchy check ──

const ROLE_HIERARCHY: Record<UserRole, number> = {
  superadmin: 200,
  // SecurityAdmin is the InfoSec seam, not part of the author chain. Ranked for
  // hasRole() checks; its actual capabilities come from ROLE_PERMISSIONS below.
  securityadmin: 90,
  admin: 100,
  operator: 60,
  viewer: 20,
};

export function hasRole(requiredRole: UserRole): boolean {
  const role = get(userRole);
  if (!role) return false;
  return ROLE_HIERARCHY[role] >= ROLE_HIERARCHY[requiredRole];
}

// ── Permission helpers (simple role-based for now) ──

export type Action = 'create' | 'read' | 'update' | 'delete' | 'execute' | 'deploy';
export type ResourceType = 'template' | 'agent' | 'canvas' | 'resource' | 'flow' | 'memory' | 'settings';

const ROLE_PERMISSIONS: Record<UserRole, Record<ResourceType, Action[]>> = {
  superadmin: {
    template: ['create', 'read', 'update', 'delete', 'deploy'],
    agent: ['create', 'read', 'update', 'delete', 'execute'],
    canvas: ['create', 'read', 'update', 'delete', 'deploy'],
    resource: ['create', 'read', 'update', 'delete'],
    flow: ['create', 'read', 'update', 'delete', 'execute'],
    memory: ['create', 'read', 'update', 'delete'],
    settings: ['create', 'read', 'update', 'delete'],
  },
  admin: {
    template: ['create', 'read', 'update', 'delete', 'deploy'],
    agent: ['create', 'read', 'update', 'delete', 'execute'],
    canvas: ['create', 'read', 'update', 'delete', 'deploy'],
    resource: ['create', 'read', 'update', 'delete'],
    flow: ['create', 'read', 'update', 'delete', 'execute'],
    memory: ['read'],
    settings: ['read', 'update'],
  },
  // InfoSec seam: read-only over the platform (so they can navigate + review),
  // plus policy/settings. Cedar is the authority for actual policy edits.
  securityadmin: {
    template: ['read'],
    agent: ['read'],
    canvas: ['read'],
    resource: ['read'],
    flow: ['read'],
    memory: ['read'],
    settings: ['read', 'update'],
  },
  operator: {
    template: ['read', 'deploy'],
    agent: ['create', 'read', 'update', 'delete', 'execute'],
    canvas: ['read', 'deploy'],
    resource: ['read'],
    flow: ['read', 'execute'],
    memory: ['read'],
    settings: ['read'],
  },
  viewer: {
    template: ['read'],
    agent: ['read'],
    canvas: ['read'],
    resource: ['read'],
    flow: ['read'],
    memory: ['read'],
    settings: [],
  },
};

export function can(action: Action, resource: ResourceType) {
  return derived(userRole, ($role) => {
    if (!$role) return false;
    return ROLE_PERMISSIONS[$role]?.[resource]?.includes(action) ?? false;
  });
}

export function canAny(actions: Action[], resource: ResourceType) {
  return derived(userRole, ($role) => {
    if (!$role) return false;
    const allowed = ROLE_PERMISSIONS[$role]?.[resource] ?? [];
    return actions.some(a => allowed.includes(a));
  });
}

export function checkPermission(action: Action, resource: ResourceType): boolean {
  const role = get(userRole);
  if (!role) return false;
  return ROLE_PERMISSIONS[role]?.[resource]?.includes(action) ?? false;
}

// ── Menu filtering by role ──

import { getMenusForRole } from '$lib/core/auth/menu';

/** Reactive store: sidebar menu groups filtered for the current user's role */
export const menuGroups = derived(userRole, ($role) =>
  $role ? getMenusForRole($role) : []
);
