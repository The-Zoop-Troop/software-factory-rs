// Rig detail: host facts, posture, and lifetime totals from `GET /rigs/<rig>/detail`.
import { signal } from '@lit-labs/signals';
import type { BeadDetail, EpicMetrics, RigDetail } from '../core/schema.js';

export const detailByRig = signal<Readonly<Record<string, RigDetail>>>({});
export const setDetail = (rig: string, d: RigDetail): void => {
  detailByRig.set({ ...detailByRig.get(), [rig]: d });
};
/** Bead detail per `rig/bead`, loaded when a drawer opens. */
export const beadDetails = signal<Readonly<Record<string, BeadDetail>>>({});
export const setBeadDetail = (key: string, d: BeadDetail): void => {
  beadDetails.set({ ...beadDetails.get(), [key]: d });
};

/** Throughput reports per `rig/epic`, for the drawer's attempt strip. */
export const metricsByEpic = signal<Readonly<Record<string, EpicMetrics>>>({});
export const setEpicMetrics = (key: string, m: EpicMetrics): void => {
  metricsByEpic.set({ ...metricsByEpic.get(), [key]: m });
};

/** Bumped when a `task_update` frame lands for `rig/task`; drawers refetch on it. */
export const taskTouched = signal<Readonly<Record<string, number>>>({});
export const touchTask = (rig: string, id: string): void => {
  taskTouched.set({ ...taskTouched.get(), [`${rig}/${id}`]: Date.now() });
};

export const resetDetail = (): void => {
  detailByRig.set({});
  beadDetails.set({});
  metricsByEpic.set({});
  taskTouched.set({});
};

export interface PostureBadge {
  readonly label: string;
  readonly tone: 'ok' | 'warn' | '';
}

export const postureLabel = (p: RigDetail['posture']): PostureBadge => {
  switch (p) {
    case 'available':
      return { label: 'ledger live', tone: 'ok' };
    case 'stopped':
      return { label: 'stopped — history only', tone: 'warn' };
    case 'never-ran':
      return { label: 'never ran', tone: '' };
  }
};

/** "just now" / "5m ago" / "3h ago" / "2d ago" — coarse on purpose. */
export const ago = (nowMs: number, unixSecs: number): string => {
  const s = Math.max(0, Math.floor(nowMs / 1000) - unixSecs);
  if (s < 60) return 'just now';
  if (s < 3600) return `${String(Math.floor(s / 60))}m ago`;
  if (s < 172_800) return `${String(Math.floor(s / 3600))}h ago`;
  return `${String(Math.floor(s / 86_400))}d ago`;
};

export const fmtTokens = (n: number): string =>
  n >= 1_000_000 ? `${(n / 1_000_000).toFixed(1)}M` : n >= 1000 ? `${String(Math.round(n / 1000))}k` : String(n);

export const fmtDuration = (secs: number): string =>
  secs >= 3600 ? `${(secs / 3600).toFixed(1)}h` : secs >= 60 ? `${String(Math.round(secs / 60))}m` : `${String(secs)}s`;

export interface FactRow {
  readonly label: string;
  readonly value: string;
  readonly href?: string;
  readonly mono?: boolean;
}

const budgetText = (b: RigDetail['budget']): string | null => {
  const parts: string[] = [];
  if (b.max_tokens !== null) parts.push(`≤ ${fmtTokens(b.max_tokens)} tokens`);
  if (b.max_usd_micros !== null) parts.push(`≤ $${(b.max_usd_micros / 1_000_000).toFixed(2)}`);
  return parts.length === 0 ? null : parts.join(' · ');
};

/** The facts card rows, in display order; absent facts are skipped, never shown blank. */
export const factRows = (d: RigDetail, nowMs: number): ReadonlyArray<FactRow> => {
  const rows: FactRow[] = [];
  const f = d.facts;
  if (f?.repo_url != null) {
    const href = f.repo_url.startsWith('http') ? f.repo_url : undefined;
    rows.push(href === undefined ? { label: 'Repo', value: f.repo_url, mono: true } : { label: 'Repo', value: f.repo_url, href, mono: true });
  }
  if (f?.main != null) rows.push({ label: 'Branch', value: f.main, mono: true });
  if (f?.runtime != null) rows.push({ label: 'Runtime', value: f.runtime });
  if (f?.harness != null) rows.push({ label: 'Harness', value: f.harness });
  const budget = budgetText(d.budget);
  if (budget !== null) rows.push({ label: 'Budget', value: budget });
  if (d.ledger_ms !== null) rows.push({ label: 'Ledger', value: `answered in ${String(d.ledger_ms)} ms` });
  if (d.events.last_at !== null) rows.push({ label: 'Last activity', value: ago(nowMs, d.events.last_at) });
  return rows;
};

export interface Total {
  readonly label: string;
  readonly value: string;
}

/** The lifetime totals strip under the facts card. */
export const totals = (d: RigDetail): ReadonlyArray<Total> => {
  const r = d.rollup;
  const pct = r.tasks_landed === 0 ? null : Math.round((r.first_pass / r.tasks_landed) * 100);
  return [
    { label: 'epics', value: String(r.epics) },
    { label: 'tasks landed', value: `${String(r.tasks_landed)}/${String(r.tasks_planned)}` },
    ...(pct === null ? [] : [{ label: 'first pass', value: `${String(pct)}%` }]),
    { label: 'tokens', value: fmtTokens(r.tokens) },
    { label: 'work', value: fmtDuration(r.work_seconds) },
    ...(r.retry_tax_seconds === 0 ? [] : [{ label: 'retry tax', value: fmtDuration(r.retry_tax_seconds) }]),
    { label: 'events', value: String(d.events.count) },
  ];
};
