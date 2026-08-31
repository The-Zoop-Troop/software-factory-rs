import { describe, it, expect } from 'vitest';
import { Schema } from 'effect';
import { RigDetail } from '../core/schema.js';
import { ago, factRows, fmtDuration, fmtTokens, postureLabel, totals } from './detail.js';

const detail = Schema.decodeUnknownSync(RigDetail)({
  rig: 'toy',
  facts: { repo_url: 'https://github.com/x/y.git', runtime: 'node', harness: 'claude', main: 'feat/z' },
  posture: 'available',
  ledger_ms: 12,
  events: { count: 41, last_at: 1_700_000_000 },
  budget: { max_tokens: 5_000_000, max_usd_micros: 12_500_000 },
  rollup: { epics: 3, tasks_landed: 10, tasks_planned: 12, first_pass: 8, tokens: 1_234_000, work_seconds: 7200, retry_tax_seconds: 300 },
});

describe('rig detail mappers', () => {
  it('labels every posture', () => {
    expect(postureLabel('available')).toEqual({ label: 'ledger live', tone: 'ok' });
    expect(postureLabel('stopped').tone).toBe('warn');
    expect(postureLabel('never-ran').label).toBe('never ran');
  });

  it('renders coarse relative time', () => {
    const now = 1_700_000_000_000;
    expect(ago(now, 1_700_000_000)).toBe('just now');
    expect(ago(now, 1_700_000_000 - 300)).toBe('5m ago');
    expect(ago(now, 1_700_000_000 - 7200)).toBe('2h ago');
    expect(ago(now, 1_700_000_000 - 3 * 86_400)).toBe('3d ago');
  });

  it('formats tokens and durations at human scale', () => {
    expect(fmtTokens(950)).toBe('950');
    expect(fmtTokens(42_000)).toBe('42k');
    expect(fmtTokens(1_234_000)).toBe('1.2M');
    expect(fmtDuration(45)).toBe('45s');
    expect(fmtDuration(300)).toBe('5m');
    expect(fmtDuration(7200)).toBe('2.0h');
  });

  it('builds fact rows in order, links http repos, and merges the budget', () => {
    const rows = factRows(detail, 1_700_000_060_000);
    expect(rows.map((r) => r.label)).toEqual(['Repo', 'Branch', 'Runtime', 'Harness', 'Budget', 'Ledger', 'Last activity']);
    expect(rows[0]?.href).toBe('https://github.com/x/y.git');
    expect(rows[4]?.value).toBe('≤ 5.0M tokens · ≤ $12.50');
    expect(rows[6]?.value).toBe('1m ago');
  });

  it('skips absent facts instead of rendering blanks', () => {
    const bare = { ...detail, facts: null, ledger_ms: null, events: { count: 0, last_at: null }, budget: { max_tokens: null, max_usd_micros: null } };
    expect(factRows(bare, 0)).toEqual([]);
    const ssh = { ...detail, facts: { ...detail.facts, repo_url: 'git@github.com:x/y.git' } } as typeof detail;
    expect(factRows(ssh, 0)[0]?.href).toBeUndefined();
  });

  it('sums the lifetime totals strip', () => {
    const t = totals(detail);
    expect(t.find((x) => x.label === 'tasks landed')?.value).toBe('10/12');
    expect(t.find((x) => x.label === 'first pass')?.value).toBe('80%');
    expect(t.find((x) => x.label === 'retry tax')?.value).toBe('5m');
    expect(t.find((x) => x.label === 'events')?.value).toBe('41');
    const idle = totals({ ...detail, rollup: { ...detail.rollup, tasks_landed: 0, retry_tax_seconds: 0 } });
    expect(idle.some((x) => x.label === 'first pass')).toBe(false);
    expect(idle.some((x) => x.label === 'retry tax')).toBe(false);
  });
});
