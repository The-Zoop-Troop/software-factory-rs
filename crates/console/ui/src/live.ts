// The live loop: keep one event stream open while connected; on each frame, refresh the rig it
// belongs to (debounced) and surface the ones a human wants to hear about.
import { Effect, Fiber, Stream } from 'effect';
import { refreshRig } from './actions.js';
import { eventStream, type EventSourceFactory } from './core/events.js';
import { RigName } from './core/schema.js';
import { describe, push, streamStatus } from './state/events.js';
import { notify } from './state/notices.js';
import { Schema } from 'effect';

let fiber: Fiber.RuntimeFiber<void> | undefined;
const pending = new Map<string, ReturnType<typeof setTimeout>>();

const scheduleRefresh = (rig: string): void => {
  const existing = pending.get(rig);
  if (existing !== undefined) clearTimeout(existing);
  pending.set(
    rig,
    globalThis.setTimeout(() => {
      pending.delete(rig);
      const name = Schema.decodeUnknownEither(RigName)(rig);
      if (name._tag === 'Right') void refreshRig(name.right);
    }, 400),
  );
};

export const onFrame = (frame: Parameters<typeof push>[0]): void => {
  push(frame);
  if (frame.replay) return;
  scheduleRefresh(frame.rig);
  const line = describe(frame);
  if (line !== null && line.quiet !== true) notify(line.tone, line.title, frame.rig);
};

const defaultFactory: EventSourceFactory = (url) => new EventSource(url);

/** Start streaming `/events` for the connected session. Idempotent; stops the previous run. */
export const startLive = (baseUrl: string, token: string, factory: EventSourceFactory = defaultFactory): void => {
  stopLive();
  streamStatus.set('connecting');
  const url = `${baseUrl}/events?token=${encodeURIComponent(token)}&backlog=25`;
  const program = eventStream(url, factory).pipe(
    Stream.tap((frame) => Effect.sync(() => { streamStatus.set('live'); onFrame(frame); })),
    Stream.runDrain,
    Effect.catchAll(() => Effect.sync(() => { streamStatus.set('reconnecting'); })),
  );
  fiber = Effect.runFork(program);
};

export const stopLive = (): void => {
  if (fiber !== undefined) {
    Effect.runFork(Fiber.interrupt(fiber));
    fiber = undefined;
  }
  for (const t of pending.values()) clearTimeout(t);
  pending.clear();
  streamStatus.set('off');
};
