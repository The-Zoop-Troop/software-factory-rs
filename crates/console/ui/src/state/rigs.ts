// Rigs and their tasks: the shared live model every page renders from.
import { signal, computed } from '@lit-labs/signals';
import type { RigName, Task } from '../core/schema.js';

export const rigs = signal<ReadonlyArray<RigName>>([]);
export const tasksByRig = signal<Readonly<Record<string, ReadonlyArray<Task>>>>({});
export const currentRig = signal<RigName | null>(null);

export const isEpic = (t: Task): boolean => t.metadata.factory.kind === 'epic';
export const isRequest = (t: Task): boolean => t.metadata.factory.kind === 'plan_request';
export const needsHuman = (t: Task): boolean => t.status.state === 'TASK_STATE_INPUT_REQUIRED';
export const isTerminal = (t: Task): boolean =>
  t.status.state === 'TASK_STATE_COMPLETED' ||
  t.status.state === 'TASK_STATE_FAILED' ||
  t.status.state === 'TASK_STATE_CANCELED' ||
  t.status.state === 'TASK_STATE_REJECTED';

export interface RigSummary {
  readonly rig: RigName;
  readonly epics: number;
  readonly working: number;
  readonly attention: number;
  readonly done: number;
}

export const summaries = computed<ReadonlyArray<RigSummary>>(() =>
  rigs.get().map((rig) => {
    const tasks = tasksByRig.get()[rig] ?? [];
    const epics = tasks.filter(isEpic);
    return {
      rig,
      epics: epics.length,
      working: epics.filter((t) => t.status.state === 'TASK_STATE_WORKING').length,
      attention: tasks.filter(needsHuman).length,
      done: epics.filter(isTerminal).length,
    };
  }),
);

export const attentionCount = computed(() =>
  Object.values(tasksByRig.get()).reduce((n, ts) => n + ts.filter(needsHuman).length, 0),
);

export const setTasks = (rig: RigName, tasks: ReadonlyArray<Task>): void => {
  tasksByRig.set({ ...tasksByRig.get(), [rig]: tasks });
};

export const reset = (): void => {
  rigs.set([]);
  tasksByRig.set({});
  currentRig.set(null);
};

/** Everything that needs a human, across rigs, newest rig first. */
export const attentionItems = computed<ReadonlyArray<{ readonly rig: string; readonly task: Task }>>(() =>
  Object.entries(tasksByRig.get()).flatMap(([rig, tasks]) =>
    tasks.filter((t) => needsHuman(t) && !isEpic(t) && !isRequest(t)).map((task) => ({ rig, task })),
  ),
);

/** A task by id on a rig, if loaded. */
export const taskById = (rig: string, id: string): Task | undefined => (tasksByRig.get()[rig] ?? []).find((t) => t.id === id);
