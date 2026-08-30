// The error taxonomy of the client. Each maps to a message and, where possible, a recovery.
import { Data } from 'effect';

export class Unauthorized extends Data.TaggedError('Unauthorized')<{ readonly detail: string }> {}
export class Forbidden extends Data.TaggedError('Forbidden')<{ readonly detail: string }> {}
export class TaskNotFound extends Data.TaggedError('TaskNotFound')<{ readonly id: string }> {}
export class Terminal extends Data.TaggedError('Terminal')<{ readonly id: string }> {}
export class Budget extends Data.TaggedError('Budget')<{ readonly detail: string }> {}
export class PlannerFailed extends Data.TaggedError('PlannerFailed')<{ readonly detail: string }> {}
export class Unreachable extends Data.TaggedError('Unreachable')<{ readonly detail: string }> {}
export class Malformed extends Data.TaggedError('Malformed')<{ readonly detail: string }> {}

export type ApiError =
  | Unauthorized
  | Forbidden
  | TaskNotFound
  | Terminal
  | Budget
  | PlannerFailed
  | Unreachable
  | Malformed;

/** JSON-RPC error code → typed error (codes from docs/generated/console-api.md). */
export const fromRpc = (status: number, code: number, message: string): ApiError => {
  if (status === 401) return new Unauthorized({ detail: message });
  if (code === -32040 || status === 403) return new Forbidden({ detail: message });
  if (code === -32001) return new TaskNotFound({ id: message.replace(/^.*`([^`]+)`.*$/, '$1') });
  if (code === -32002) return new Terminal({ id: message.replace(/^.*`([^`]+)`.*$/, '$1') });
  if (code === -32041) return new Budget({ detail: message });
  if (code === -32603 && /planner|plan/i.test(message)) return new PlannerFailed({ detail: message });
  return new Unreachable({ detail: message });
};

/** What to tell the operator, and what they can do about it. */
export const explain = (e: ApiError): { readonly title: string; readonly detail: string; readonly recovery: string } => {
  switch (e._tag) {
    case 'Unauthorized':
      return { title: 'Not signed in', detail: e.detail, recovery: 'Paste a valid console token and connect again.' };
    case 'Forbidden':
      return { title: 'Not allowed', detail: e.detail, recovery: 'This token lacks the scope for that action on this rig.' };
    case 'TaskNotFound':
      return { title: 'Unknown task', detail: `No task ${e.id} on this rig.`, recovery: 'Refresh; it may have been closed.' };
    case 'Terminal':
      return { title: 'Already finished', detail: `Task ${e.id} is terminal.`, recovery: 'Start a new plan instead.' };
    case 'Budget':
      return { title: 'Rig budget reached', detail: e.detail, recovery: 'Raise the cap in the console registry or wait for spend to reset.' };
    case 'PlannerFailed':
      return { title: 'Planner failed', detail: e.detail, recovery: 'Check the rig planner logs and harness credentials, then submit again.' };
    case 'Unreachable':
      return { title: 'Console unreachable', detail: e.detail, recovery: 'Check the connection; the console will reconnect automatically.' };
    case 'Malformed':
      return { title: 'Unexpected reply', detail: e.detail, recovery: 'The console and UI versions may differ; reload.' };
  }
};
