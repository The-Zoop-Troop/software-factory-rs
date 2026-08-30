// Session: where the console is and who we are. Persisted token is a per-browser convenience.
import { signal, computed } from '@lit-labs/signals';
import type { Scope, Whoami } from '../core/schema.js';

export type Connection = 'idle' | 'connecting' | 'online' | 'offline';

export const baseUrl = signal<string>(typeof location === 'undefined' ? 'http://127.0.0.1:7700' : location.origin);
export const token = signal<string>('');
export const connection = signal<Connection>('idle');
export const lastError = signal<{ readonly title: string; readonly detail: string; readonly recovery: string } | null>(null);

export const isOnline = computed(() => connection.get() === 'online');
export const identity = signal<Whoami | null>(null);

/** May this token do `scope` on `rig`? `admin` implies everything. Unknown until whoami loads. */
export const can = (rig: string, scope: Scope): boolean => {
  const g = identity.get()?.grants.find((x) => x.rig === rig);
  return g !== undefined && (g.scopes.includes(scope) || g.scopes.includes('admin'));
};

/** Why an action is unavailable, for the operator. */
export const whyNot = (rig: string, scope: Scope): string =>
  identity.get() === null ? 'Checking what this token may do…' : `This token has no \`${scope}\` scope on ${rig}.`;

const KEY = 'factory.token';

export const loadToken = (): string => {
  const v = typeof localStorage === 'undefined' ? null : localStorage.getItem(KEY);
  token.set(v ?? '');
  return token.get();
};

export const saveToken = (v: string): void => {
  token.set(v);
  if (typeof localStorage !== 'undefined') localStorage.setItem(KEY, v);
};

export const reset = (): void => {
  token.set('');
  identity.set(null);
  connection.set('idle');
  lastError.set(null);
};
