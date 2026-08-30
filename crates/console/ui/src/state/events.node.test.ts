import { describe as suite, it, expect, beforeEach } from 'vitest';
import { Effect, Stream } from 'effect';
import { eventStream, type EventFrame, type EventSourceLike } from '../core/events.js';
import { describe, forRig, push, recent, reset, streamStatus } from './events.js';
import { onFrame, startLive, stopLive } from '../live.js';
import { notices, reset as resetNotices } from './notices.js';

const frame = (kind: string, extra: Record<string, unknown> = {}, rig = 'toy'): EventFrame => ({
  rig, cursor: 1, record: { at: 1, actor: 'worker', bead: 'ep-1.1', kind, ...extra },
});

beforeEach(() => { reset(); resetNotices(); stopLive(); });

suite('events store', () => {
  it('keeps a bounded ring and filters per rig', () => {
    for (let i = 0; i < 205; i++) push(frame('claimed', {}, i % 2 === 0 ? 'toy' : 'api'));
    expect(recent.get().length).toBe(200);
    expect(forRig('toy').get().every((f) => f.rig === 'toy')).toBe(true);
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
    stopLive();
    expect(streamStatus.get()).toBe('off');
    expect(created).toBe(1);
  });
});
