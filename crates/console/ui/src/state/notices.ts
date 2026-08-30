// In-page notifications: toasts (transient) and the attention badge (derived elsewhere).
import { signal } from '@lit-labs/signals';

export type Tone = 'info' | 'success' | 'warning' | 'danger';

export interface Notice {
  readonly id: number;
  readonly tone: Tone;
  readonly title: string;
  readonly detail?: string;
}

export const notices = signal<ReadonlyArray<Notice>>([]);
let next = 1;

/** How long a toast stays, and how many may stack; older ones make room. */
export const TOAST_TTL_MS = 6000;
export const MAX_TOASTS = 4;
const timers = new Map<number, ReturnType<typeof setTimeout>>();

export const notify = (tone: Tone, title: string, detail?: string, ttl = TOAST_TTL_MS): number => {
  const id = next++;
  const notice: Notice = detail === undefined ? { id, tone, title } : { id, tone, title, detail };
  const kept = [...notices.get(), notice];
  const overflow = kept.length - MAX_TOASTS;
  for (const old of kept.slice(0, Math.max(0, overflow))) clearTimer(old.id);
  notices.set(overflow > 0 ? kept.slice(overflow) : kept);
  if (ttl > 0) timers.set(id, setTimeout(() => { dismiss(id); }, ttl));
  return id;
};

const clearTimer = (id: number): void => {
  const t = timers.get(id);
  if (t !== undefined) clearTimeout(t);
  timers.delete(id);
};

export const dismiss = (id: number): void => {
  clearTimer(id);
  notices.set(notices.get().filter((n) => n.id !== id));
};

export const reset = (): void => {
  for (const id of timers.keys()) clearTimer(id);
  notices.set([]);
  next = 1;
};
