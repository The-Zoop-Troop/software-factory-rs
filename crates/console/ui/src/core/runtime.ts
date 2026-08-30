// The one place Effects are run. Components call `run(effect)` and get a Promise back.
import { Effect, Layer, ManagedRuntime } from 'effect';
import { ConsoleApi, ConsoleApiFake, ConsoleApiLive, type FakeWorld, type Session } from './api.js';

let runtime: ManagedRuntime.ManagedRuntime<ConsoleApi, never> | undefined;

export const connect = (session: Session): void => {
  runtime?.dispose().catch(() => undefined);
  runtime = ManagedRuntime.make(ConsoleApiLive(session));
};

export const connectFake = (world: FakeWorld, token: string): void => {
  runtime?.dispose().catch(() => undefined);
  runtime = ManagedRuntime.make(ConsoleApiFake(world, token));
};

export const disconnect = (): void => {
  runtime?.dispose().catch(() => undefined);
  runtime = undefined;
};

export const connected = (): boolean => runtime !== undefined;

/** Run an effect that needs the API; rejects with the typed error if it fails. */
export const run = <A, E>(effect: Effect.Effect<A, E, ConsoleApi>): Promise<A> =>
  runtime === undefined
    ? Promise.reject(new Error('not connected'))
    : runtime.runPromise(effect);

export const withApi = <A, E>(f: (api: ConsoleApi['Type']) => Effect.Effect<A, E>): Effect.Effect<A, E, ConsoleApi> =>
  Effect.flatMap(ConsoleApi, f);

// Exposed for tests that want to provide a layer without the global runtime.
export const provideFake = (world: FakeWorld, token: string): Layer.Layer<ConsoleApi> => ConsoleApiFake(world, token);
