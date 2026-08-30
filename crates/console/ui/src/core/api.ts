// ConsoleApi: everything the UI can ask the console, as a service. Live over HTTP, Fake in memory.
import { Context, Effect, Layer, Schema } from 'effect';
import type { ApiError } from './errors.js';
import { Unauthorized, Forbidden, TaskNotFound } from './errors.js';
import { decode, fetchJson, rejectIfError } from './interop.js';
import { AgentCard, RigList, RpcReply, Task, TaskList, type RigName, type Task as TaskT } from './schema.js';

export interface ConsoleApiShape {
  readonly rigs: () => Effect.Effect<ReadonlyArray<RigName>, ApiError>;
  readonly card: (rig: RigName) => Effect.Effect<AgentCard, ApiError>;
  readonly tasks: (rig: RigName) => Effect.Effect<ReadonlyArray<TaskT>, ApiError>;
  readonly task: (rig: RigName, id: string) => Effect.Effect<TaskT, ApiError>;
  readonly plan: (rig: RigName, text: string) => Effect.Effect<TaskT, ApiError>;
  readonly resolve: (rig: RigName, id: string, note: string) => Effect.Effect<TaskT, ApiError>;
  readonly stop: (rig: RigName, id: string) => Effect.Effect<TaskT, ApiError>;
}

export class ConsoleApi extends Context.Tag('ConsoleApi')<ConsoleApi, ConsoleApiShape>() {}

export interface Session {
  readonly baseUrl: string;
  readonly token: string;
}

const rpc = (session: Session, rig: RigName, method: string, params: unknown) =>
  fetchJson(`${session.baseUrl}/rigs/${rig}/a2a`, {
    method: 'POST',
    headers: { authorization: `Bearer ${session.token}`, 'content-type': 'application/json' },
    body: JSON.stringify({ jsonrpc: '2.0', id: 1, method, params }),
  }).pipe(
    Effect.flatMap(rejectIfError),
    Effect.flatMap((body) => decode(RpcReply, body)),
    Effect.flatMap((reply) =>
      reply.error !== undefined
        ? Effect.fail(new Forbidden({ detail: reply.error.message }))
        : Effect.succeed(reply.result),
    ),
  );

const taskOf = (v: unknown) =>
  decode(Schema.Union(Schema.Struct({ task: Task }), Task), v).pipe(
    Effect.map((t) => ('task' in t ? t.task : t)),
  );

export const ConsoleApiLive = (session: Session): Layer.Layer<ConsoleApi> =>
  Layer.succeed(ConsoleApi, {
    rigs: () =>
      fetchJson(`${session.baseUrl}/rigs`, { headers: { authorization: `Bearer ${session.token}` } }).pipe(
        Effect.flatMap(rejectIfError),
        Effect.flatMap((b) => decode(RigList, b)),
        Effect.map((r) => r.rigs),
      ),
    card: (rig) =>
      fetchJson(`${session.baseUrl}/rigs/${rig}/.well-known/agent-card.json`, {}).pipe(
        Effect.flatMap(rejectIfError),
        Effect.flatMap((b) => decode(AgentCard, b)),
      ),
    tasks: (rig) =>
      rpc(session, rig, 'ListTasks', null).pipe(
        Effect.flatMap((r) => decode(TaskList, r)),
        Effect.map((t) => t.tasks),
      ),
    task: (rig, id) => rpc(session, rig, 'GetTask', { id }).pipe(Effect.flatMap(taskOf)),
    // Non-blocking: the console returns the queued request as a SUBMITTED task; the event
    // stream and refreshes carry it to COMPLETED (epic created) or FAILED.
    plan: (rig, text) =>
      rpc(session, rig, 'SendMessage', {
        message: { messageId: `m-${String(Date.now())}`, role: 'ROLE_USER', parts: [{ text }] },
        configuration: { returnImmediately: true },
      }).pipe(Effect.flatMap(taskOf)),
    resolve: (rig, id, note) =>
      rpc(session, rig, 'SendMessage', {
        message: { messageId: `m-${String(Date.now())}`, role: 'ROLE_USER', parts: [{ text: note }], taskId: id },
      }).pipe(Effect.flatMap(taskOf)),
    stop: (rig, id) => rpc(session, rig, 'CancelTask', { id }).pipe(Effect.flatMap(taskOf)),
  });

/** An in-memory console for tests and `pnpm dev` without a rig. */
export interface FakeWorld {
  readonly token: string;
  readonly rigs: ReadonlyArray<RigName>;
  tasks: Record<string, ReadonlyArray<TaskT>>;
}

export const ConsoleApiFake = (world: FakeWorld, token: string): Layer.Layer<ConsoleApi> => {
  const auth = <A>(f: () => Effect.Effect<A, ApiError>): Effect.Effect<A, ApiError> =>
    token === world.token ? f() : Effect.fail(new Unauthorized({ detail: 'missing or unknown bearer token' }));
  const find = (rig: RigName, id: string): Effect.Effect<TaskT, TaskNotFound> => {
    const t = (world.tasks[rig] ?? []).find((x) => x.id === id);
    return t === undefined ? Effect.fail(new TaskNotFound({ id })) : Effect.succeed(t);
  };
  const withState = (t: TaskT, state: TaskT['status']['state']): TaskT => ({ ...t, status: { ...t.status, state } });
  return Layer.succeed(ConsoleApi, {
    rigs: () => auth(() => Effect.succeed(world.rigs)),
    card: (rig) =>
      Effect.succeed({ name: `factory rig ${rig}`, version: 'fake', skills: [{ id: 'plan', name: 'Plan' }] }),
    tasks: (rig) => auth(() => Effect.succeed(world.tasks[rig] ?? [])),
    task: (rig, id) => auth(() => find(rig, id)),
    plan: (rig, text) =>
      auth(() => {
        const id = `${rig}-${String((world.tasks[rig] ?? []).length + 1)}` as TaskT['id'];
        const task: TaskT = {
          id,
          contextId: id,
          status: { state: 'TASK_STATE_SUBMITTED', timestamp: 'now' },
          metadata: { factory: { kind: 'plan_request', title: text.split('\n')[0] ?? text, tasks: 0, closed: 0, working: 0, incidents: 0 } },
        };
        world.tasks = { ...world.tasks, [rig]: [...(world.tasks[rig] ?? []), task] };
        return Effect.succeed(task);
      }),
    resolve: (rig, id) => auth(() => find(rig, id).pipe(Effect.map((t) => withState(t, 'TASK_STATE_COMPLETED')))),
    stop: (rig, id) => auth(() => find(rig, id).pipe(Effect.map((t) => withState(t, 'TASK_STATE_CANCELED')))),
  });
};
