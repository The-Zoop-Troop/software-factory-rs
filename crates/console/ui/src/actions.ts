// Actions the UI performs: Effects against the API that update signals and report outcomes.
// Components call these and get a Promise<boolean> (ok?) back; failures are already explained.
import { Effect } from 'effect';
import { explain, type ApiError } from './core/errors.js';
import type { AttentionOption, RigName } from './core/schema.js';
import { ConsoleApi } from './core/api.js';
import { run, withApi } from './core/runtime.js';
import { notify } from './state/notices.js';
import { historyByRig, markUnavailable, rigs, setHistory, setTasks, taskById } from './state/rigs.js';
import { setBeadDetail, setConsumers, setDetail, setEpicMetrics } from './state/detail.js';
import { setEpicHistory } from './state/events.js';
import { connection, identity, lastError } from './state/session.js';
import { signal } from '@lit-labs/signals';

/** Ids with an action in flight, so every control can show its own pending state. */
export const pending = signal<ReadonlySet<string>>(new Set());
const mark = (id: string, on: boolean): Effect.Effect<void> =>
  Effect.sync(() => {
    const next = new Set(pending.get());
    if (on) next.add(id); else next.delete(id);
    pending.set(next);
  });
const tracked = <A, E, R>(id: string, effect: Effect.Effect<A, E, R>): Effect.Effect<A, E, R> =>
  Effect.acquireUseRelease(mark(id, true), () => effect, () => mark(id, false));

const report = (err: ApiError): Effect.Effect<boolean> =>
  Effect.sync(() => {
    lastError.set(explain(err));
    if (err._tag === 'Unreachable') connection.set('offline');
    return false as const;
  });

const attempt = <A>(effect: Effect.Effect<A, ApiError, ConsoleApi>): Promise<boolean> =>
  run(effect.pipe(Effect.as(true as const), Effect.catchAll(report))).catch(() => false);

const loadRig = (rig: RigName) =>
  withApi((api) => api.tasks(rig)).pipe(Effect.tap((ts) => Effect.sync(() => { setTasks(rig, ts); markUnavailable(rig, null); })));

/** A rig that cannot be read (stopped, no ledger yet) is marked, not fatal to the overview. */
const loadRigLenient = (rig: RigName) =>
  loadRig(rig).pipe(
    Effect.catchTags({
      Unreachable: (e) => Effect.sync(() => { markUnavailable(rig, e.detail); setTasks(rig, []); }),
      Malformed: (e) => Effect.sync(() => { markUnavailable(rig, e.detail); setTasks(rig, []); }),
    }),
    Effect.asVoid,
  );

/** Who the token is, loaded once per session. */
const loadIdentity = (): Effect.Effect<void, ApiError, ConsoleApi> =>
  identity.get() === null
    ? withApi((api) => api.whoami()).pipe(Effect.map((me) => { identity.set(me); }))
    : Effect.void;

/** Load rigs and every rig's tasks; also who we are, once. */
export const refreshAll = (): Promise<boolean> => {
  connection.set(connection.get() === 'online' ? 'online' : 'connecting');
  return attempt(
    loadIdentity().pipe(
      Effect.flatMap(() => withApi((api) => api.rigs())),
      Effect.tap((r) => Effect.sync(() => {
        rigs.set(r.names);
        // The console already said these cannot answer: mark them, do not ask them.
        for (const [name, why] of Object.entries(r.unavailable)) { markUnavailable(name, why); setTasks(name as RigName, []); }
      })),
      Effect.flatMap((r) => Effect.forEach(r.names.filter((n) => !(n in r.unavailable)), loadRigLenient, { concurrency: 4, discard: true })),
      Effect.tap(() => Effect.sync(() => { connection.set('online'); lastError.set(null); lastRefreshAt.set(Date.now()); })),
    ),
  );
};

/** One in-flight ListTasks per rig; a request that arrives meanwhile runs once more after it. */
const inFlight = new Map<string, { promise: Promise<boolean>; again: boolean }>();
export const refreshRig = (rig: RigName): Promise<boolean> => {
  const current = inFlight.get(rig);
  if (current !== undefined) { current.again = true; return current.promise; }
  const entry = { again: false, promise: Promise.resolve(false) };
  entry.promise = attempt(loadRig(rig).pipe(Effect.asVoid)).then(async (ok) => {
    inFlight.delete(rig);
    return entry.again ? refreshRig(rig) : ok;
  });
  inFlight.set(rig, entry);
  return entry.promise;
};
/** How many loads are running right now (tests). */
export const inFlightCount = (): number => inFlight.size;

/** When the last full refresh finished (ms since epoch), for the backstop timer. */
export const lastRefreshAt = signal<number>(0);

const historyOf = (rig: RigName) => historyByRig.get()[rig] ?? [];

/** One bead in depth for the drawer. Best-effort: a missing bead just leaves it empty. */
export const loadBeadDetail = (rig: RigName, id: string): Promise<boolean> =>
  run(
    withApi((api) => api.beadDetail(rig, id)).pipe(
      Effect.map((d) => { setBeadDetail(`${rig}/${id}`, d); return true as const; }),
      Effect.catchAll(() => Effect.succeed(false as const)),
    ),
  ).catch(() => false);

/** The epic's throughput report, cached for the drawer's attempt strip. */
export const loadEpicMetrics = (rig: RigName, epic: string): Promise<boolean> =>
  run(
    withApi((api) => api.metrics(rig, epic)).pipe(
      Effect.map((m) => { if (m !== null) setEpicMetrics(`${rig}/${epic}`, m); return true as const; }),
      Effect.catchAll(() => Effect.succeed(false as const)),
    ),
  ).catch(() => false);

/** Who builds on this epic, across rigs. Best-effort. */
export const loadEpicConsumers = (rig: RigName, epic: string): Promise<boolean> =>
  run(
    withApi((api) => api.consumers(rig, epic)).pipe(
      Effect.map((c) => { setConsumers(`${rig}/${epic}`, c); return true as const; }),
      Effect.catchAll(() => Effect.succeed(false as const)),
    ),
  ).catch(() => false);

/** Rig facts + lifetime rollup. Best-effort: a rig without detail keeps its page, quietly. */
export const loadRigDetail = (rig: RigName): Promise<boolean> =>
  run(
    withApi((api) => api.detail(rig)).pipe(
      Effect.map((d) => { setDetail(rig, d); return true as const; }),
      Effect.catchAll(() => Effect.succeed(false as const)),
    ),
  ).catch(() => false);

/** Closed epics of a rig (the Completed section). */
export const loadHistory = (rig: RigName): Promise<boolean> =>
  attempt(withApi((api) => api.history(rig)).pipe(Effect.map((ts) => { setHistory(rig, ts); })));

/** A closed (or any) epic's page: the task itself if it is not loaded yet, and its full log. */
export const loadEpicHistory = (rig: RigName, id: string): Promise<boolean> =>
  attempt(
    Effect.all([
      taskById(rig, id) === undefined
        ? withApi((api) => api.task(rig, id)).pipe(Effect.map((t) => { setHistory(rig, [...(historyOf(rig)).filter((x) => x.id !== t.id), t]); }))
        : Effect.void,
      withApi((api) => api.epicEvents(rig, id)).pipe(Effect.map((events) => { setEpicHistory(rig, id, events); })),
    ], { discard: true }),
  );

export const submitPlan = (rig: RigName, text: string, needs: ReadonlyArray<{ readonly rig: string; readonly epic: string }> = []): Promise<boolean> =>
  attempt(
    withApi((api) => api.plan(rig, text, needs)).pipe(
      Effect.tap((task) => Effect.sync(() => { notify('info', `Plan queued as ${task.id}`, 'The rig\'s planner is on it; the card updates when the epic exists.'); })),
      Effect.flatMap(() => loadRig(rig)),
    ),
  );

export const resolveItem = (rig: RigName, id: string, note: string): Promise<boolean> =>
  attempt(
    tracked(id, withApi((api) => api.resolve(rig, id, note))).pipe(
      Effect.tap(() => Effect.sync(() => { notify('success', `Resolved ${id}`); })),
      Effect.flatMap(() => loadRig(rig)),
    ),
  );

export const applyOption = (rig: RigName, id: string, option: AttentionOption, note: string): Promise<boolean> =>
  attempt(
    tracked(id, withApi((api) => api.applyOption(rig, id, option, note))).pipe(
      Effect.tap(() => Effect.sync(() => { notify(option === 'stop_epic' ? 'warning' : 'success', `${option.replace(/_/g, ' ')} applied to ${id}`); })),
      Effect.flatMap(() => loadRig(rig)),
    ),
  );

export const stopEpic = (rig: RigName, id: string): Promise<boolean> =>
  attempt(
    tracked(id, withApi((api) => api.stop(rig, id))).pipe(
      Effect.tap(() => Effect.sync(() => { notify('warning', `Stopped ${id}`, 'Open tasks were closed.'); })),
      Effect.flatMap(() => loadRig(rig)),
    ),
  );
