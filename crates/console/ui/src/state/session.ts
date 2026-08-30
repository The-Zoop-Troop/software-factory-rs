// Session: where the console is and who we are. Persisted token is a per-browser convenience.
import { signal, computed } from '@lit-labs/signals';

export type Connection = 'idle' | 'connecting' | 'online' | 'offline';

export const baseUrl = signal<string>(typeof location === 'undefined' ? 'http://127.0.0.1:7700' : location.origin);
export const token = signal<string>('');
export const connection = signal<Connection>('idle');
export const lastError = signal<{ readonly title: string; readonly detail: string; readonly recovery: string } | null>(null);

export const isOnline = computed(() => connection.get() === 'online');

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
  connection.set('idle');
  lastError.set(null);
};
