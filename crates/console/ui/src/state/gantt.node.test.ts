import { describe, it, expect } from 'vitest';
import { layout, mmss } from './gantt.js';
import type { EpicMetrics } from '../core/schema.js';

const attempt = (o: Partial<EpicMetrics['tasks'][number]['attempts'][number]>) => ({
  claimed: 0, submitted: null, verify_started: null, verified: null, passed: null, integrate_started: null, integrated: null, landed: false, ended_by: null, tokens: 0, ...o,
});

const report: EpicMetrics = {
  epic: 'e-1', wall_clock: 210, work: 100, parallelism_pct: 47, critical_path: 80, retry_tax: 40, first_pass: 1, landed: 2, tokens: 13,
  stages: [], concurrency: [[0, 1]],
  tasks: [
    { task: 'e-1.1', planned: 0, needs: [], attempts: [attempt({ claimed: 10, submitted: 70, verify_started: 80, verified: 90, passed: true, integrate_started: 95, integrated: 100, landed: true })] },
    { task: 'e-1.2', planned: 0, needs: ['e-1.1'], attempts: [
      attempt({ claimed: 130, submitted: 170, verified: 171, passed: false, ended_by: 'verify_failed' }),
      attempt({ claimed: 180, submitted: 200, verified: 201, passed: true, integrated: 210, landed: true }),
    ] },
  ],
};

describe('gantt layout', () => {
  it('lays every attempt out by stage from the epic origin, with queue wait from planned or landed needs', () => {
    const l = layout(report);
    expect(l.origin).toBe(0);
    expect(l.span).toBe(210);
    const first = l.rows[0]?.segments.map((s) => `${s.stage}:${String(s.start)}-${String(s.end)}`);
    expect(first).toEqual(['queue_wait:0-10', 'session:10-70', 'verify_wait:70-80', 'verify:80-90', 'integrate_wait:90-95', 'integrate:95-100']);
    const second = l.rows[1]?.segments ?? [];
    expect(second[0]).toMatchObject({ stage: 'queue_wait', start: 100, end: 130 });
    expect(second.filter((s) => !s.landed).map((s) => s.stage)).toEqual(['session', 'verify']);
    expect(second.find((s) => s.attempt === 1 && s.stage === 'integrate')).toMatchObject({ start: 201, end: 210 });
  });
  it('formats seconds as m:ss and copes with an empty report', () => {
    expect(mmss(0)).toBe('0:00');
    expect(mmss(3657)).toBe('60:57');
    expect(layout({ ...report, tasks: [] })).toEqual({ origin: 0, span: 1, rows: [] });
  });
});
