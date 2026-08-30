// The only file that touches fetch/promises directly: wraps them into Effects with typed errors.
import { Effect, Schema } from 'effect';
import { Malformed, Unreachable, fromRpc, type ApiError } from './errors.js';

export interface HttpReply {
  readonly status: number;
  readonly body: unknown;
}

export const fetchJson = (
  input: string,
  init: RequestInit,
): Effect.Effect<HttpReply, Unreachable | Malformed> =>
  Effect.tryPromise({
    try: async () => {
      const r = await fetch(input, init);
      const text = await r.text();
      let body: unknown = null;
      if (text.length > 0) {
        body = JSON.parse(text) as unknown;
      }
      return { status: r.status, body };
    },
    catch: (e) =>
      e instanceof SyntaxError
        ? new Malformed({ detail: e.message })
        : new Unreachable({ detail: e instanceof Error ? e.message : String(e) }),
  });

/** Decode a reply body with a schema, or fail as `Malformed`. */
export const decode = <A, I>(schema: Schema.Schema<A, I>, body: unknown): Effect.Effect<A, Malformed> =>
  Schema.decodeUnknown(schema)(body).pipe(
    Effect.mapError((e) => new Malformed({ detail: e.message.split('\n')[0] ?? 'decode failed' })),
  );

/** Non-2xx replies become typed errors using the JSON-RPC error body when present. */
export const rejectIfError = (reply: HttpReply): Effect.Effect<unknown, ApiError> => {
  if (reply.status < 400) return Effect.succeed(reply.body);
  const err = reply.body as { error?: { code?: number; message?: string } | string } | null;
  const inner = err?.error;
  if (inner !== undefined && typeof inner === 'object') {
    return Effect.fail(fromRpc(reply.status, inner.code ?? 0, inner.message ?? 'error'));
  }
  return Effect.fail(fromRpc(reply.status, 0, typeof inner === 'string' ? inner : `HTTP ${String(reply.status)}`));
};
