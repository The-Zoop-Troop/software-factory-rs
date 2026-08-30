// Gantt layout for the throughput page: pure functions from an EpicMetrics report to rows of
// stage segments in seconds from the epic's origin. No DOM here.
import type { Attempt, EpicMetrics, TaskMetrics } from '../core/schema.js';

export type Stage = 'queue_wait' | 'session' | 'verify_wait' | 'verify' | 'integrate_wait' | 'integrate';

export interface Segment {
  readonly stage: Stage;
  /** Seconds from the origin. */
  readonly start: number;
  readonly end: number;
  readonly attempt: number;
  readonly landed: boolean;
}

export interface Row {
  readonly task: string;
  readonly segments: ReadonlyArray<Segment>;
}

export interface Layout {
  readonly origin: number;
  readonly span: number;
  readonly rows: ReadonlyArray<Row>;
}

/** When a task became ready: its planning time, or the last landing among its needs. */
const readyAt = (t: TaskMetrics, landedAt: ReadonlyMap<string, number>): number | null => {
  const times = t.needs.map((n) => landedAt.get(n)).filter((x): x is number => x !== undefined);
  if (t.planned !== null) times.push(t.planned);
  return times.length === 0 ? null : Math.max(...times);
};

const segmentsOf = (a: Attempt, i: number, origin: number): Segment[] => {
  const out: Segment[] = [];
  const seg = (stage: Stage, s: number | null, e: number | null): void => {
    if (s !== null && e !== null && e >= s) out.push({ stage, start: s - origin, end: e - origin, attempt: i, landed: a.landed });
  };
  seg('session', a.claimed, a.submitted);
  seg('verify_wait', a.submitted, a.verify_started);
  seg('verify', a.verify_started ?? a.submitted, a.verified);
  seg('integrate_wait', a.verified, a.integrate_started);
  seg('integrate', a.integrate_started ?? a.verified, a.integrated);
  return out;
};

/** Rows in task order; every attempt's stages as segments; queue wait when a ready time exists. */
export const layout = (m: EpicMetrics): Layout => {
  const stamps = m.tasks.flatMap((t) => [t.planned, ...t.attempts.map((a) => a.claimed)]).filter((x): x is number => x !== null);
  const origin = stamps.length === 0 ? 0 : Math.min(...stamps);
  const landedAt = new Map<string, number>();
  for (const t of m.tasks) {
    const l = t.attempts.find((a) => a.landed)?.integrated;
    if (l !== null && l !== undefined) landedAt.set(t.task, l);
  }
  let span = 0;
  const rows = m.tasks.map((t) => {
    const segments: Segment[] = [];
    const ready = readyAt(t, landedAt);
    const first = t.attempts[0];
    if (ready !== null && first !== undefined && first.claimed > ready) {
      segments.push({ stage: 'queue_wait', start: ready - origin, end: first.claimed - origin, attempt: 0, landed: true });
    }
    t.attempts.forEach((a, i) => segments.push(...segmentsOf(a, i, origin)));
    for (const s of segments) span = Math.max(span, s.end);
    return { task: t.task, segments };
  });
  return { origin, span: Math.max(span, 1), rows };
};

export const STAGE_LABEL: Readonly<Record<Stage, string>> = {
  queue_wait: 'waiting for a worker',
  session: 'session',
  verify_wait: 'waiting for the verifier',
  verify: 'verify',
  integrate_wait: 'waiting for the integrator',
  integrate: 'integrate',
};

export const mmss = (secs: number): string => `${String(Math.floor(secs / 60))}:${String(Math.floor(secs % 60)).padStart(2, '0')}`;
