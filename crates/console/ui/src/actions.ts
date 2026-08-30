// Actions the UI performs: Effects against the API that update signals and report outcomes.
// Components call these and get a Promise<boolean> (ok?) back; failures are already explained.
import { Effect } from 'effect';
import { explain, type ApiError } from './core/errors.js';
import type { RigName } from './core/schema.js';
import { ConsoleApi } from './core/api.js';
import { run, withApi } from './core/runtime.js';
import { notify } from './state/notices.js';
import { rigs, setTasks } from './state/rigs.js';
import { connection, lastError } from './state/session.js';

const report = (err: ApiError): Effect.Effect<boolean> =>
  Effect.sync(() => {
    lastError.set(explain(err));
    if (err._tag === 'Unreachable') connection.set('offline');
    return false as const;
  });

const attempt = <A>(effect: Effect.Effect<A, ApiError, ConsoleApi>): Promise<boolean> =>
  run(effect.pipe(Effect.as(true as const), Effect.catchAll(report))).catch(() => false);

const loadRig = (rig: RigName) => withApi((api) => api.tasks(rig)).pipe(Effect.tap((ts) => Effect.sync(() => { setTasks(rig, ts); })));

/** Load rigs and every rig's tasks. */
export const refreshAll = (): Promise<boolean> => {
  connection.set(connection.get() === 'online' ? 'online' : 'connecting');
  return attempt(
    withApi((api) => api.rigs()).pipe(
      Effect.tap((names) => Effect.sync(() => { rigs.set(names); })),
      Effect.flatMap((names) => Effect.forEach(names, loadRig, { concurrency: 4, discard: true })),
      Effect.tap(() => Effect.sync(() => { connection.set('online'); lastError.set(null); })),
    ),
  );
};

export const refreshRig = (rig: RigName): Promise<boolean> => attempt(loadRig(rig).pipe(Effect.asVoid));

export const submitPlan = (rig: RigName, text: string): Promise<boolean> =>
  attempt(
    withApi((api) => api.plan(rig, text)).pipe(
      Effect.tap((task) => Effect.sync(() => { notify('info', `Plan queued as ${task.id}`, 'The rig\'s planner is on it; the card updates when the epic exists.'); })),
      Effect.flatMap(() => loadRig(rig)),
    ),
  );

export const resolveItem = (rig: RigName, id: string, note: string): Promise<boolean> =>
  attempt(
    withApi((api) => api.resolve(rig, id, note)).pipe(
      Effect.tap(() => Effect.sync(() => { notify('success', `Resolved ${id}`); })),
      Effect.flatMap(() => loadRig(rig)),
    ),
  );

export const stopEpic = (rig: RigName, id: string): Promise<boolean> =>
  attempt(
    withApi((api) => api.stop(rig, id)).pipe(
      Effect.tap(() => Effect.sync(() => { notify('warning', `Stopped ${id}`, 'Open tasks were closed.'); })),
      Effect.flatMap(() => loadRig(rig)),
    ),
  );
