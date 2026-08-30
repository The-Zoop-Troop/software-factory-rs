// Recent events per rig (a ring buffer), stream status, and what the UI derives from them.
import { signal, computed } from '@lit-labs/signals';
import type { EventFrame } from '../core/events.js';

export type StreamStatus = 'off' | 'connecting' | 'live' | 'reconnecting';

const KEEP = 200;

export const streamStatus = signal<StreamStatus>('off');
export const recent = signal<ReadonlyArray<EventFrame>>([]);
export const lastEventAt = signal<number | null>(null);

export const push = (frame: EventFrame): void => {
  const next = [...recent.get(), frame];
  recent.set(next.length > KEEP ? next.slice(next.length - KEEP) : next);
  lastEventAt.set(Date.now());
};

/** Events for one rig (a plain read; callers inside `render()` are tracked through `recent`). */
export const forRig = (rig: string): ReadonlyArray<EventFrame> => recent.get().filter((f) => f.rig === rig);

export const str = (v: unknown): string => (typeof v === 'string' ? v : v === undefined || v === null ? '' : JSON.stringify(v));
export const num = (v: unknown): string => (typeof v === 'number' ? String(v) : '0');

export interface Line { readonly title: string; readonly tone: 'info' | 'success' | 'warning' | 'danger'; readonly quiet?: boolean }

/** Human line for an event, or null when it is noise (steward sweeps, heartbeats). `quiet` lines stay off the toasts. */
export const describe = (f: EventFrame): Line | null => {
  const r = f.record;
  const bead = typeof r.bead === 'string' ? r.bead : '';
  switch (r.kind) {
    case 'claimed': return { title: `${bead} claimed by ${(str(r['holder']) || r.actor)}`, tone: 'info' };
    case 'submitted': return { title: `${bead} submitted for verification`, tone: 'info' };
    case 'released': return { title: `${bead} released: ${str(r['detail'])}`, tone: 'warning' };
    case 'verified': return r['passed'] === true ? { title: `${bead} verified`, tone: 'success' } : { title: `${bead} failed verification`, tone: 'warning' };
    case 'integrated': return r['landed'] ? { title: `${bead} landed on main`, tone: 'success' } : { title: `${bead} could not be integrated`, tone: 'warning' };
    case 'escalated': return { title: `${bead} needs you`, tone: 'danger' };
    case 'verify_started': return { title: `${bead} checks running`, tone: 'info', quiet: true };
    case 'integrate_started': return { title: `${bead} landing`, tone: 'info', quiet: true };
    case 'task_planned': return { title: `${bead} planned`, tone: 'info', quiet: true };
    case 'progress': return { title: `${bead} working: ${num(r['files'])} files, +${num(r['insertions'])}/-${num(r['deletions'])}`, tone: 'info', quiet: true };
    case 'verify_blocked': return { title: `${bead}: the rig could not run its checks (${str(r['detail'])})`, tone: 'danger' };
    case 'lease_reaped': return { title: `${bead} lease expired; reopened`, tone: 'warning' };
    case 'epic_closed': return { title: `${bead} epic complete`, tone: 'success' };
    case 'remote': {
      const action = str(r['action']);
      const detail = str(r['detail']);
      if (action === 'plan_started') return { title: `planning: ${detail}`, tone: 'info' };
      if (action === 'planned') return { title: `epic created: ${detail}`, tone: 'success' };
      if (action === 'plan_failed') return { title: `planning failed: ${detail}`, tone: 'danger' };
      if (action === 'alert') return { title: `alert sent: ${detail}`, tone: 'info' };
      if (action === 'cancel') return { title: `${bead} stopped`, tone: 'warning' };
      return null;
    }
    case 'transition':
    case 'sweep_done':
    case 'merge_bead_repaired':
    case 'error':
    default:
      return null;
  }
};

export const reset = (): void => {
  streamStatus.set('off');
  recent.set([]);
  lastEventAt.set(null);
};

/** Alert deliveries the console recorded (webhook / chat), newest first. */
export const alerts = computed(() =>
  recent
    .get()
    .filter((f) => f.record.kind === 'remote' && (f.record['action'] === 'alert' || f.record['action'] === 'alert-failed'))
    .slice()
    .reverse(),
);

/** Events under an epic (the epic itself and its children `epic.N`). */
export const forEpic = (rig: string, epic: string): ReadonlyArray<EventFrame> =>
  recent.get().filter((f) => f.rig === rig && typeof f.record.bead === 'string' && (f.record.bead === epic || f.record.bead.startsWith(`${epic}.`)));

/** The most recent progress sample per bead among `frames` (what a running session has changed so far). */
export const latestProgress = (frames: ReadonlyArray<EventFrame>): ReadonlyMap<string, Line> => {
  const out = new Map<string, Line>();
  for (const f of frames) {
    const r = f.record;
    const bead = r.bead;
    if (r.kind !== 'progress' || typeof bead !== 'string') continue;
    out.set(bead, { title: `${num(r['files'])} files · +${num(r['insertions'])}/-${num(r['deletions'])}`, tone: 'info' });
  }
  return out;
};

/** A record's `at` as a Date: unix seconds (number or numeric string) or an ISO string; never invalid. */
export const recordDate = (at: unknown): Date => {
  const n = typeof at === 'number' ? at : typeof at === 'string' && /^\d+$/.test(at) ? Number(at) : NaN;
  const d = Number.isFinite(n) ? new Date(n * 1000) : new Date(typeof at === 'string' ? at : NaN);
  return Number.isNaN(d.getTime()) ? new Date(0) : d;
};
