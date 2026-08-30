// Boundary schemas: every byte from the console API is decoded here, once.
import { Schema } from 'effect';

export const RigName = Schema.String.pipe(Schema.pattern(/^[a-z0-9][a-z0-9_-]*$/), Schema.brand('RigName'));
export type RigName = typeof RigName.Type;

export const TaskId = Schema.NonEmptyString.pipe(Schema.brand('TaskId'));
export type TaskId = typeof TaskId.Type;

export const TaskState = Schema.Literal(
  'TASK_STATE_SUBMITTED',
  'TASK_STATE_WORKING',
  'TASK_STATE_INPUT_REQUIRED',
  'TASK_STATE_COMPLETED',
  'TASK_STATE_FAILED',
  'TASK_STATE_CANCELED',
  'TASK_STATE_REJECTED',
);
export type TaskState = typeof TaskState.Type;

export const Part = Schema.Union(
  Schema.Struct({ text: Schema.String }),
  Schema.Struct({ data: Schema.Unknown }),
);

export const Message = Schema.Struct({
  messageId: Schema.String,
  role: Schema.String,
  parts: Schema.Array(Part),
  taskId: Schema.optional(Schema.String),
  contextId: Schema.optional(Schema.String),
});
export type Message = typeof Message.Type;

export const FactoryMeta = Schema.Struct({
  kind: Schema.String,
  title: Schema.optionalWith(Schema.String, { default: () => '' }),
  tasks: Schema.optionalWith(Schema.Number, { default: () => 0 }),
  closed: Schema.optionalWith(Schema.Number, { default: () => 0 }),
  working: Schema.optionalWith(Schema.Number, { default: () => 0 }),
  incidents: Schema.optionalWith(Schema.Number, { default: () => 0 }),
  epic: Schema.optional(Schema.NullOr(Schema.String)),
  failure: Schema.optional(Schema.NullOr(Schema.String)),
});

export const Task = Schema.Struct({
  id: TaskId,
  contextId: Schema.String,
  status: Schema.Struct({
    state: TaskState,
    message: Schema.optional(Message),
    timestamp: Schema.String,
  }),
  metadata: Schema.Struct({ factory: FactoryMeta }),
});
export type Task = typeof Task.Type;

export const TaskList = Schema.Struct({ tasks: Schema.Array(Task) });

export const RigList = Schema.Struct({ rigs: Schema.Array(RigName) });

export const AgentCard = Schema.Struct({
  name: Schema.String,
  version: Schema.String,
  skills: Schema.Array(Schema.Struct({ id: Schema.String, name: Schema.String })),
});
export type AgentCard = typeof AgentCard.Type;

export const RpcError = Schema.Struct({
  code: Schema.Number,
  message: Schema.String,
});

export const RpcReply = Schema.Struct({
  result: Schema.optional(Schema.Unknown),
  error: Schema.optional(RpcError),
});

/** Text of a message: its text parts joined. */
export const messageText = (m: Message | undefined): string =>
  m === undefined ? '' : m.parts.flatMap((p) => ('text' in p ? [p.text] : [])).join('\n');

export const Scope = Schema.Literal('watch', 'plan', 'resolve', 'admin');
export type Scope = typeof Scope.Type;

export const Whoami = Schema.Struct({
  client: Schema.String,
  grants: Schema.Array(Schema.Struct({ rig: Schema.String, scopes: Schema.Array(Scope) })),
});
export type Whoami = typeof Whoami.Type;

export const AttentionOption = Schema.Literal('retry_fresh', 'retry_with_guidance', 'stop_epic', 'replan', 'answer');
export type AttentionOption = typeof AttentionOption.Type;

export const Counter = Schema.Struct({ used: Schema.Number, limit: Schema.Number });

export const Attention = Schema.Struct({
  kind: Schema.String,
  id: Schema.String,
  taskId: Schema.optional(Schema.NullOr(Schema.String)),
  epicId: Schema.optional(Schema.NullOr(Schema.String)),
  reason: Schema.Struct({ kind: Schema.String, summary: Schema.String, detail: Schema.String }),
  attempts: Schema.optional(Schema.NullOr(Counter)),
  tokens: Schema.optional(Schema.NullOr(Counter)),
  branch: Schema.optional(Schema.NullOr(Schema.String)),
  lastVerify: Schema.optional(Schema.NullOr(Schema.String)),
  guidance: Schema.Array(Schema.String),
  options: Schema.Array(Schema.Struct({
    id: AttentionOption,
    label: Schema.String,
    description: Schema.String,
    needsNote: Schema.Boolean,
    destructive: Schema.Boolean,
  })),
});
export type Attention = typeof Attention.Type;

/** The structured attention item carried by an INPUT_REQUIRED message, if any. */
export const attentionOf = (m: Message | undefined): Attention | undefined => {
  if (m === undefined) return undefined;
  for (const p of m.parts) {
    if ('data' in p) {
      const d = Schema.decodeUnknownEither(Attention)(p.data);
      if (d._tag === 'Right') return d.right;
    }
  }
  return undefined;
};
