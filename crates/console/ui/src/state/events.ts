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

export const forRig = (rig: string) => computed(() => recent.get().filter((f) => f.rig === rig));

export const str = (v: unknown): string => (typeof v === 'string' ? v : v === undefined || v === null ? '' : JSON.stringify(v));

/** Human line for an event, or null when it is noise (steward sweeps, heartbeats). */
export const describe = (f: EventFrame): { readonly title: string; readonly tone: 'info' | 'success' | 'warning' | 'danger' } | null => {
  const r = f.record;
  const bead = typeof r.bead === 'string' ? r.bead : '';
  switch (r.kind) {
    case 'claimed': return { title: `${bead} claimed by ${(str(r['holder']) || r.actor)}`, tone: 'info' };
    case 'submitted': return { title: `${bead} submitted for verification`, tone: 'info' };
    case 'released': return { title: `${bead} released: ${str(r['detail'])}`, tone: 'warning' };
    case 'verified': return r['passed'] === true ? { title: `${bead} verified`, tone: 'success' } : { title: `${bead} failed verification`, tone: 'warning' };
    case 'integrated': return r['landed'] ? { title: `${bead} landed on main`, tone: 'success' } : { title: `${bead} could not be integrated`, tone: 'warning' };
    case 'escalated': return { title: `${bead} needs you`, tone: 'danger' };
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
export const forEpic = (rig: string, epic: string) =>
  computed(() => recent.get().filter((f) => f.rig === rig && typeof f.record.bead === 'string' && (f.record.bead === epic || f.record.bead.startsWith(`${epic}.`))));
