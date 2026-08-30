import { describe, it, expect } from 'vitest';
import { Effect, Schema } from 'effect';
import { ConsoleApi, ConsoleApiFake, type FakeWorld } from './api.js';
import { explain, fromRpc } from './errors.js';
import { RigName, Task, TaskList, messageText } from './schema.js';
import { rejectIfError } from './interop.js';

const rig = Schema.decodeSync(RigName)('toy');
const epic = Schema.decodeSync(Task)({
  id: 'ep-1', contextId: 'ep-1',
  status: { state: 'TASK_STATE_WORKING', timestamp: 't', message: { messageId: 'm', role: 'ROLE_AGENT', parts: [{ text: 'a' }, { data: {} }, { text: 'b' }] } },
  metadata: { factory: { kind: 'epic', title: 'T', tasks: 3, closed: 1 } },
});
const world = (): FakeWorld => ({ token: 'ok', rigs: [rig], tasks: { toy: [epic] } });

describe('schemas', () => {
  it('decode tasks with defaults and reject junk', () => {
    expect(epic.metadata.factory.incidents).toBe(0);
    expect(messageText(epic.status.message)).toBe('a\nb');
    expect(messageText(undefined)).toBe('');
    expect(Schema.decodeUnknownEither(TaskList)({ tasks: [{ id: 1 }] })._tag).toBe('Left');
    expect(Schema.decodeUnknownEither(RigName)('Bad Name')._tag).toBe('Left');
  });
});

describe('errors', () => {
  it('map rpc codes to tags and explanations', () => {
    expect(fromRpc(401, 0, 'x')._tag).toBe('Unauthorized');
    expect(fromRpc(403, -32040, 'x')._tag).toBe('Forbidden');
    expect(fromRpc(404, -32001, 'task `ep-9` not found')).toMatchObject({ _tag: 'TaskNotFound', id: 'ep-9' });
    expect(fromRpc(200, -32002, 'task `ep-1` is terminal')).toMatchObject({ _tag: 'Terminal', id: 'ep-1' });
    expect(fromRpc(200, -32041, 'cap')._tag).toBe('Budget');
    expect(fromRpc(500, -32603, 'rig refused the plan: planner died')._tag).toBe('PlannerFailed');
    expect(fromRpc(502, 0, 'gateway')._tag).toBe('Unreachable');
    for (const e of [fromRpc(401, 0, 'x'), fromRpc(403, 0, 'x'), fromRpc(404, -32001, '`a`'), fromRpc(200, -32002, '`a`'), fromRpc(200, -32041, 'x'), fromRpc(500, -32603, 'plan'), fromRpc(502, 0, 'x')]) {
      expect(explain(e).title.length).toBeGreaterThan(0);
      expect(explain(e).recovery.length).toBeGreaterThan(0);
    }
  });

  it('rejectIfError reads json-rpc bodies and plain errors', async () => {
    expect(await Effect.runPromise(rejectIfError({ status: 200, body: { a: 1 } }))).toEqual({ a: 1 });
    const rpc = await Effect.runPromise(Effect.flip(rejectIfError({ status: 403, body: { error: { code: -32040, message: 'no' } } })));
    expect(rpc._tag).toBe('Forbidden');
    const plain = await Effect.runPromise(Effect.flip(rejectIfError({ status: 401, body: { error: 'missing token' } })));
    expect(plain._tag).toBe('Unauthorized');
    const empty = await Effect.runPromise(Effect.flip(rejectIfError({ status: 500, body: null })));
    expect(empty._tag).toBe('Unreachable');
  });
});

describe('fake api', () => {
  const call = <A, E>(w: FakeWorld, tok: string, f: (api: ConsoleApi['Type']) => Effect.Effect<A, E>) =>
    Effect.runPromise(Effect.flatMap(ConsoleApi, f).pipe(Effect.provide(ConsoleApiFake(w, tok))));

  it('serves rigs, tasks, plan, resolve, stop', async () => {
    const w = world();
    expect(await call(w, 'ok', (a) => a.rigs())).toEqual(['toy']);
    expect((await call(w, 'ok', (a) => a.tasks(rig))).length).toBe(1);
    expect((await call(w, 'ok', (a) => a.card(rig))).name).toContain('toy');
    const planned = await call(w, 'ok', (a) => a.plan(rig, 'build it\nmore'));
    expect(planned.metadata.factory.title).toBe('build it');
    expect((await call(w, 'ok', (a) => a.tasks(rig))).length).toBe(2);
    expect((await call(w, 'ok', (a) => a.resolve(rig, 'ep-1', 'ok'))).status.state).toBe('TASK_STATE_COMPLETED');
    expect((await call(w, 'ok', (a) => a.stop(rig, 'ep-1'))).status.state).toBe('TASK_STATE_CANCELED');
    expect((await call(w, 'ok', (a) => a.task(rig, 'ep-1'))).id).toBe('ep-1');
  });

  it('refuses bad tokens and unknown tasks', async () => {
    const w = world();
    const bad = await Effect.runPromise(Effect.flip(Effect.flatMap(ConsoleApi, (a) => a.rigs()).pipe(Effect.provide(ConsoleApiFake(w, 'nope')))));
    expect(bad._tag).toBe('Unauthorized');
    const missing = await Effect.runPromise(Effect.flip(Effect.flatMap(ConsoleApi, (a) => a.task(rig, 'zz')).pipe(Effect.provide(ConsoleApiFake(w, 'ok')))));
    expect(missing._tag).toBe('TaskNotFound');
  });
});
