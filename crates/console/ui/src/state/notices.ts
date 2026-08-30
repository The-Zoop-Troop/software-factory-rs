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

export const notify = (tone: Tone, title: string, detail?: string): number => {
  const id = next++;
  const notice: Notice = detail === undefined ? { id, tone, title } : { id, tone, title, detail };
  notices.set([...notices.get(), notice]);
  return id;
};

export const dismiss = (id: number): void => {
  notices.set(notices.get().filter((n) => n.id !== id));
};

export const reset = (): void => {
  notices.set([]);
  next = 1;
};
