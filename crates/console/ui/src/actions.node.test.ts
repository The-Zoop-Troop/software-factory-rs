import { describe, it, expect, beforeEach } from 'vitest';
import { Schema } from 'effect';
import { refreshAll, refreshRig, resolveItem, stopEpic, submitPlan } from './actions.js';
import { connectFake, connected, disconnect, run, withApi } from './core/runtime.js';
import type { FakeWorld } from './core/api.js';
import { RigName, Task } from './core/schema.js';
import { reset as resetNotices, notices } from './state/notices.js';
import { reset as resetRigs, rigs, tasksByRig } from './state/rigs.js';
import { connection, lastError, reset as resetSession } from './state/session.js';

const rig = Schema.decodeSync(RigName)('toy');
const incident = Schema.decodeSync(Task)({ id: 'inc-1', contextId: 'ep-1', status: { state: 'TASK_STATE_INPUT_REQUIRED', timestamp: 't' }, metadata: { factory: { kind: 'incident' } } });

beforeEach(() => { resetRigs(); resetNotices(); resetSession(); disconnect(); });

describe('actions over the fake console', () => {
  it('refresh loads rigs and tasks and goes online', async () => {
    connectFake({ token: 'ok', rigs: [rig], tasks: { toy: [incident] } }, 'ok');
    expect(connected()).toBe(true);
    expect(await refreshAll()).toBe(true);
    expect(rigs.get()).toEqual(['toy']);
    expect(tasksByRig.get()['toy']?.length).toBe(1);
    expect(connection.get()).toBe('online');
    expect(lastError.get()).toBeNull();
  });

  it('plan, resolve, stop notify and refresh', async () => {
    connectFake({ token: 'ok', rigs: [rig], tasks: { toy: [incident] } }, 'ok');
    expect(await submitPlan(rig, 'build a thing')).toBe(true);
    expect(notices.get()[0]?.title).toMatch(/Plan queued as/);
    expect(tasksByRig.get()['toy']?.length).toBe(2);
    expect(await resolveItem(rig, 'inc-1', 'done')).toBe(true);
    expect(await stopEpic(rig, 'toy-2')).toBe(true);
    expect(notices.get().map((n) => n.tone)).toEqual(['info', 'success', 'warning']);
    expect(await refreshRig(rig)).toBe(true);
  });

  it('explains failures and never throws', async () => {
    connectFake({ token: 'ok', rigs: [rig], tasks: {} }, 'wrong');
    expect(await refreshAll()).toBe(false);
    expect(lastError.get()?.title).toBe('Not signed in');
    expect(await stopEpic(rig, 'nope')).toBe(false);
    disconnect();
    expect(await refreshRig(rig)).toBe(false);
    await expect(run(withApi((api) => api.rigs()))).rejects.toThrow('not connected');
  });
});

describe('identity and options', () => {
  it('loads whoami once and gates actions by scope', async () => {
    const { identity, can, whyNot } = await import('./state/session.js');
    connectFake({ token: 'ok', rigs: [rig], tasks: {}, scopes: ['watch'] }, 'ok');
    expect(whyNot('toy', 'plan')).toContain('Checking');
    await refreshAll();
    expect(identity.get()?.client).toBe('fake');
    expect(can('toy', 'watch')).toBe(true);
    expect(can('toy', 'plan')).toBe(false);
    expect(can('other', 'watch')).toBe(false);
    expect(whyNot('toy', 'plan')).toContain('`plan`');
    identity.set({ client: 'x', grants: [{ rig: 'toy', scopes: ['admin'] }] });
    expect(can('toy', 'resolve')).toBe(true);
  });

  it('applies attention options with a pending marker', async () => {
    const { applyOption, pending } = await import('./actions.js');
    const world: FakeWorld = { token: 'ok', rigs: [rig], tasks: { toy: [incident] } };
    connectFake(world, 'ok');
    expect(await applyOption(rig, 'inc-1', 'retry_with_guidance', 'use sh')).toBe(true);
    expect(pending.get().has('inc-1')).toBe(false);
    expect(world.applied?.[0]).toEqual({ id: 'inc-1', option: 'retry_with_guidance', note: 'use sh' });
    expect(notices.get().at(-1)?.title).toContain('retry with guidance');
    expect(await applyOption(rig, 'zz', 'stop_epic', '')).toBe(false);
  });
});
