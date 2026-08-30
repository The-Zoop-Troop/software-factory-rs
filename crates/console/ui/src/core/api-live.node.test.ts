import { describe, it, expect, afterEach } from 'vitest';
import { Effect, Schema } from 'effect';
import { ConsoleApi, ConsoleApiLive } from './api.js';
import { RigName } from './schema.js';

const rig = Schema.decodeSync(RigName)('toy');
const task = { id: 'ep-1', contextId: 'ep-1', status: { state: 'TASK_STATE_WORKING', timestamp: 't' }, metadata: { factory: { kind: 'epic' } } };
const originalFetch = globalThis.fetch;

interface Seen { url: string; init: RequestInit | undefined }
const stub = (replies: ReadonlyArray<{ status: number; body: unknown }>): Seen[] => {
  const seen: Seen[] = [];
  let i = 0;
  globalThis.fetch = ((url: string, init?: RequestInit) => {
    seen.push({ url, init });
    const r = replies[Math.min(i++, replies.length - 1)] ?? { status: 500, body: null };
    return Promise.resolve(new Response(r.body === null ? '' : JSON.stringify(r.body), { status: r.status, headers: { 'content-type': 'application/json' } }));
  }) as typeof fetch;
  return seen;
};
const bodyOf = (s: Seen | undefined): string => (typeof s?.init?.body === 'string' ? s.init.body : '');
afterEach(() => { globalThis.fetch = originalFetch; });

const call = <A, E>(f: (api: ConsoleApi['Type']) => Effect.Effect<A, E>) =>
  Effect.runPromise(Effect.flatMap(ConsoleApi, f).pipe(Effect.provide(ConsoleApiLive({ baseUrl: 'http://c', token: 'tok' }))));
const fail = <A, E>(f: (api: ConsoleApi['Type']) => Effect.Effect<A, E>) =>
  Effect.runPromise(Effect.flip(Effect.flatMap(ConsoleApi, f)).pipe(Effect.provide(ConsoleApiLive({ baseUrl: 'http://c', token: 'tok' }))));

describe('live api over HTTP', () => {
  it('speaks json-rpc with the bearer token', async () => {
    const seen = stub([
      { status: 200, body: { rigs: ['toy'] } },
      { status: 200, body: { name: 'factory rig toy', version: '1', skills: [] } },
      { status: 200, body: { jsonrpc: '2.0', id: 1, result: { tasks: [task] } } },
      { status: 200, body: { jsonrpc: '2.0', id: 1, result: task } },
      { status: 200, body: { jsonrpc: '2.0', id: 1, result: { task } } },
      { status: 200, body: { jsonrpc: '2.0', id: 1, result: { task } } },
      { status: 200, body: { jsonrpc: '2.0', id: 1, result: task } },
    ]);
    expect((await call((a) => a.rigs())).names).toEqual(['toy']);
    expect((await call((a) => a.card(rig))).name).toBe('factory rig toy');
    expect((await call((a) => a.tasks(rig))).length).toBe(1);
    expect((await call((a) => a.task(rig, 'ep-1'))).id).toBe('ep-1');
    expect((await call((a) => a.plan(rig, 'go'))).id).toBe('ep-1');
    expect((await call((a) => a.resolve(rig, 'inc-1', 'ok'))).id).toBe('ep-1');
    expect((await call((a) => a.stop(rig, 'ep-1'))).id).toBe('ep-1');
    expect(seen[0]?.url).toBe('http://c/rigs');
    expect((seen[2]?.init?.headers as Record<string, string>)['authorization']).toBe('Bearer tok');
    expect(bodyOf(seen[2])).toContain('"ListTasks"');
    expect(bodyOf(seen[5])).toContain('"taskId":"inc-1"');
  });

  it('maps refusals, rpc errors, malformed and unreachable', async () => {
    stub([
      { status: 401, body: { error: 'missing or unknown bearer token' } },
      { status: 200, body: { jsonrpc: '2.0', id: 1, error: { code: -32040, message: 'no' } } },
      { status: 200, body: { jsonrpc: '2.0', id: 1, result: { nope: 1 } } },
    ]);
    expect((await fail((a) => a.rigs()))._tag).toBe('Unauthorized');
    expect((await fail((a) => a.tasks(rig)))._tag).toBe('Forbidden');
    expect((await fail((a) => a.task(rig, 'x')))._tag).toBe('Malformed');
    globalThis.fetch = () => Promise.reject(new TypeError('network down'));
    expect((await fail((a) => a.rigs()))._tag).toBe('Unreachable');
    globalThis.fetch = () => Promise.resolve(new Response('not json', { status: 200 }));
    expect((await fail((a) => a.rigs()))._tag).toBe('Malformed');
  });
});
