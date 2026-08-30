// Live events: an SSE connection as an Effect Stream with typed frames and automatic reconnect.
import { Effect, Schedule, Schema, Stream } from 'effect';
import { Malformed, Unreachable } from './errors.js';

export const EventRecord = Schema.Struct({
  at: Schema.Union(Schema.String, Schema.Number),
  actor: Schema.String,
  bead: Schema.optional(Schema.NullOr(Schema.String)),
  kind: Schema.String,
}).pipe(Schema.extend(Schema.Record({ key: Schema.String, value: Schema.Unknown })));
export type EventRecord = typeof EventRecord.Type;

export const EventFrame = Schema.Struct({
  rig: Schema.String,
  cursor: Schema.Number,
  /** Replayed from the backlog on connect: shown in feeds, never announced. */
  replay: Schema.optionalWith(Schema.Boolean, { default: () => false }),
  record: EventRecord,
});
export type EventFrame = typeof EventFrame.Type;

export interface EventSourceLike {
  addEventListener(type: string, cb: (e: { data: string }) => void): void;
  close(): void;
}
export type EventSourceFactory = (url: string) => EventSourceLike;

/**
 * Frames from `url` as a stream. The browser's EventSource cannot send headers, so the token
 * travels as `?token=` (the console accepts it for this endpoint only). Reconnects with backoff.
 */
export const eventStream = (
  url: string,
  factory: EventSourceFactory,
): Stream.Stream<EventFrame, Unreachable | Malformed> =>
  Stream.async<EventFrame, Unreachable | Malformed>((emit) => {
    const es = factory(url);
    es.addEventListener('factory', (e) => {
      const decoded = Schema.decodeUnknownEither(Schema.parseJson(EventFrame))(e.data);
      if (decoded._tag === 'Right') void emit.single(decoded.right);
      else void emit.fail(new Malformed({ detail: decoded.left.message.split('\n')[0] ?? 'bad frame' }));
    });
    es.addEventListener('error', () => {
      void emit.fail(new Unreachable({ detail: 'event stream dropped' }));
    });
    return Effect.sync(() => { es.close(); });
  }).pipe(
    Stream.retry(Schedule.exponential('500 millis').pipe(Schedule.union(Schedule.spaced('10 seconds')))),
  );
