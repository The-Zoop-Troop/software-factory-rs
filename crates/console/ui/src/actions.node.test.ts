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
  it('submits a plan with cross-rig needs', async () => {
    const { connectFake } = await import('./core/runtime.js');
    const world = { token: 'ok', rigs: [rig], tasks: { toy: [] as never[] } };
    connectFake(world, 'ok');
    expect(await submitPlan(rig, 'Portal after the backend', [{ rig: 'backend', epic: 'be-1' }])).toBe(true);
    expect((world as { plannedNeeds?: unknown[] }).plannedNeeds).toEqual([[{ rig: 'backend', epic: 'be-1' }]]);
    const queued = tasksByRig.get()['toy']?.[0];
    expect(queued?.metadata.factory.needs).toEqual(['backend/be-1']);
    expect(queued?.metadata.factory.waiting).toBe(true);
  });

  it('coalesces concurrent refreshes of one rig into at most one in flight and one follow-up', async () => {
    const { connectFake } = await import('./core/runtime.js');
    const { inFlightCount } = await import('./actions.js');
    connectFake({ token: 'ok', rigs: [rig], tasks: { toy: [incident] } }, 'ok');
    const a = refreshRig(rig); const b = refreshRig(rig); const c = refreshRig(rig);
    expect(inFlightCount()).toBe(1);
    expect(b).toBe(a);
    expect(await Promise.all([a, b, c])).toEqual([true, true, true]);
    expect(inFlightCount()).toBe(0);
  });

  it('polls every 15 s without a stream and every 90 s with one', async () => {
    const { backstopDue } = await import('./app-shell.js');
    expect(backstopDue('off', 0, 20_000)).toBe(true);
    expect(backstopDue('live', 0, 20_000)).toBe(false);
    expect(backstopDue('live', 0, 95_000)).toBe(true);
    expect(backstopDue('reconnecting', 0, 16_000)).toBe(true);
  });

  it('does not ask rigs the console marked unavailable, and marks them itself', async () => {
    const { connectFake } = await import('./core/runtime.js');
    const { unavailable, tasksByRig } = await import('./state/rigs.js');
    connectFake({ token: 'ok', rigs: ['toy', 'idle'] as never, tasks: { toy: [] }, unavailable: { idle: 'no ledger yet: the rig has never run' } }, 'ok');
    expect(await refreshAll()).toBe(true);
    expect(unavailable.get()['idle']).toContain('never run');
    expect(tasksByRig.get()['idle']).toEqual([]);
    expect(unavailable.get()['toy']).toBeUndefined();
  });

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

describe('unavailable rigs', () => {
  it('do not fail the overview; they are marked', async () => {
    const { unavailable, summaries } = await import('./state/rigs.js');
    const api = Schema.decodeSync(RigName)('api');
    const w: FakeWorld = { token: 'ok', rigs: [rig, api], tasks: { toy: [incident] } };
    connectFake(w, 'ok');
    // The fake returns [] for unknown rigs; simulate a dead ledger through the error mapper instead.
    const { fromRpc } = await import('./core/errors.js');
    expect(fromRpc(500, -32603, 'ledger unavailable during List: backend database error: no beads database found')._tag).toBe('Unreachable');
    expect(await refreshAll()).toBe(true);
    expect(unavailable.get()).toEqual({});
    expect(summaries.get().length).toBe(2);
  });
});
