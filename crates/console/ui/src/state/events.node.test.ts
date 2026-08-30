import { describe as suite, it, expect, beforeEach } from 'vitest';
import { Effect, Stream } from 'effect';
import { eventStream, type EventFrame, type EventSourceLike } from '../core/events.js';
import { describe, forEpic, forRig, latestProgress, push, recent, recordDate, reset, setEpicHistory, streamStatus } from './events.js';
import { onFrame, startLive, stopLive } from '../live.js';
import { notices, reset as resetNotices } from './notices.js';

const frame = (kind: string, extra: Record<string, unknown> = {}, rig = 'toy'): EventFrame => ({
  rig, cursor: 1, replay: false, record: { at: 1, actor: 'worker', bead: 'ep-1.1', kind, ...extra },
});

beforeEach(() => { reset(); resetNotices(); stopLive(); });

suite('events store', () => {
  it('merges loaded history under the live ring for an epic, de-duplicated and in time order', () => {
    setEpicHistory('toy', 'ep-1', [
      { at: 10, actor: 'planner', bead: 'ep-1.1', kind: 'task_planned' },
      { at: 20, actor: 'worker', bead: 'ep-1.1', kind: 'claimed' },
      { at: 15, actor: 'stewardd', bead: 'zz-9', kind: 'claimed' },
    ]);
    push({ rig: 'toy', cursor: 7, replay: false, record: { at: 20, actor: 'worker', bead: 'ep-1.1', kind: 'claimed' } });
    push({ rig: 'toy', cursor: 8, replay: false, record: { at: 30, actor: 'worker', bead: 'ep-1.1', kind: 'submitted' } });
    const kinds = forEpic('toy', 'ep-1').map((f) => `${String(f.record.at)}:${f.record.kind}`);
    expect(kinds).toEqual(['10:task_planned', '20:claimed', '30:submitted']);
    reset();
    expect(forEpic('toy', 'ep-1')).toEqual([]);
  });

  it('turns any at into a valid date', () => {
    expect(recordDate(1788093449).getTime()).toBe(1788093449000);
    expect(recordDate('1788093449').getTime()).toBe(1788093449000);
    expect(recordDate('2026-08-30T12:00:00Z').getTime()).toBe(Date.parse('2026-08-30T12:00:00Z'));
    expect(Number.isNaN(recordDate('t').getTime())).toBe(false);
  });

  it('refreshes a rig only after state-changing events', async () => {
    const { REFRESH_KINDS } = await import('../live.js');
    expect(REFRESH_KINDS.has('claimed') && REFRESH_KINDS.has('integrated') && REFRESH_KINDS.has('remote')).toBe(true);
    expect(REFRESH_KINDS.has('progress') || REFRESH_KINDS.has('sweep_done') || REFRESH_KINDS.has('verify_started')).toBe(false);
  });

  it('shows progress in the feed and the epic page but never as a toast', () => {
    const f = frame('progress', { files: 3, insertions: 40, deletions: 2 });
    expect(describe(f)).toEqual({ title: 'ep-1.1 working: 3 files, +40/-2', tone: 'info', quiet: true });
    onFrame(f);
    expect(notices.get().length).toBe(0);
    expect(latestProgress([frame('progress', { files: 1, insertions: 1, deletions: 0 }), f]).get('ep-1.1')?.title).toBe('3 files · +40/-2');
  });

  it('keeps a bounded ring and filters per rig', () => {
    for (let i = 0; i < 205; i++) push(frame('claimed', {}, i % 2 === 0 ? 'toy' : 'api'));
    expect(recent.get().length).toBe(200);
    expect(forRig('toy').every((f) => f.rig === 'toy')).toBe(true);
  });

  it('describes the events a human cares about and hides noise', () => {
    expect(describe(frame('claimed', { holder: 'w-1' }))?.title).toBe('ep-1.1 claimed by w-1');
    expect(describe(frame('verified', { passed: true }))?.tone).toBe('success');
    expect(describe(frame('verified', { passed: false }))?.tone).toBe('warning');
    expect(describe(frame('integrated', { landed: 'abc' }))?.title).toContain('landed');
    expect(describe(frame('integrated', { landed: null }))?.tone).toBe('warning');
    expect(describe(frame('escalated'))?.tone).toBe('danger');
    expect(describe(frame('epic_closed'))?.tone).toBe('success');
    expect(describe(frame('remote', { action: 'planned', detail: 'ep-2 (3 tasks)' }))?.title).toContain('epic created');
    expect(describe(frame('remote', { action: 'plan_failed', detail: 'x' }))?.tone).toBe('danger');
    expect(describe(frame('remote', { action: 'plan_started', detail: 'x' }))?.tone).toBe('info');
    expect(describe(frame('remote', { action: 'alert', detail: 'x' }))?.title).toContain('alert');
    expect(describe(frame('remote', { action: 'cancel' }))?.tone).toBe('warning');
    expect(describe(frame('remote', { action: 'refused' }))).toBeNull();
    expect(describe(frame('sweep_done'))).toBeNull();
    expect(describe(frame('released', { detail: 'no diff' }))?.title).toContain('released');
    expect(describe(frame('submitted'))?.title).toContain('submitted');
    expect(describe(frame('lease_reaped'))?.title).toContain('lease');
  });
});

class FakeSource implements EventSourceLike {
  listeners = new Map<string, (e: { data: string }) => void>();
  closed = false;
  addEventListener(type: string, cb: (e: { data: string }) => void): void { this.listeners.set(type, cb); }
  close(): void { this.closed = true; }
  emit(type: string, data: string): void { this.listeners.get(type)?.({ data }); }
}

suite('event stream', () => {
  it('decodes frames and closes the source when done', async () => {
    const src = new FakeSource();
    const stream = eventStream('http://x/events', () => src);
    const taken = Effect.runPromise(Stream.runCollect(Stream.take(stream, 2)));
    await new Promise((r) => setTimeout(r, 10));
    src.emit('factory', JSON.stringify(frame('claimed')));
    src.emit('factory', JSON.stringify(frame('verified', { passed: true })));
    const frames = [...(await taken)];
    expect(frames.map((f) => f.record.kind)).toEqual(['claimed', 'verified']);
    expect(src.closed).toBe(true);
  });

  it('live loop pushes frames, notifies, and can be stopped', async () => {
    let created = 0;
    const src = new FakeSource();
    startLive('http://x', 'tok', (url) => { created++; expect(url).toContain('token=tok'); return src; });
    expect(streamStatus.get()).toBe('connecting');
    await new Promise((r) => setTimeout(r, 10));
    src.emit('factory', JSON.stringify(frame('escalated')));
    await new Promise((r) => setTimeout(r, 20));
    expect(streamStatus.get()).toBe('live');
    expect(recent.get().length).toBe(1);
    expect(notices.get()[0]?.tone).toBe('danger');
    onFrame(frame('sweep_done'));
    expect(notices.get().length).toBe(1);
    onFrame({ ...frame('escalated'), replay: true });
    expect(notices.get().length).toBe(1);
    expect(recent.get().length).toBe(3);
    stopLive();
    expect(streamStatus.get()).toBe('off');
    expect(created).toBe(1);
  });
});
