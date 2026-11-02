# Railway-oriented control flow

- **Status:** accepted · **Verified:** 2026-11-02 against the code — atomicity test (`transition::tests`), Steward repair test, Integrator rollback test, real-git rollback test; `xtask lint-fp` blocking in CI

The factory's own control flow is a typed railway. This is a **product** principle, not only a coding standard: it is what makes failures legible to the next agent, the console, and the gardening loop.

## The model

- **One state machine, total.** `Task::apply(Event) -> Result<Transition, IllegalTransition>` is defined over every (state, event) pair. Adding a state or an event must break the build until every pair is decided.
- **Facts in, effects out.** Roles observe facts (a verify run exited 1; a rebase conflicted; a lease expired) and hand them to the domain. The domain decides; a single imperative shell (`app::transition::run_effect`) executes the effects. Roles change task state only this way, with one exception: an operator's retry from `incident` (`app::console::resolve`) writes `open` directly.
- **Persist, then effect.** The new state is written before effects run, so a crash leaves a *detectable* gap (a mergeable task with no merge bead) rather than an undoable one. Atomicity is tested by forcing failures between steps.
- **Errors are data on the red track.** Every error enum variant carries what a caller can branch on: `Blocked { by: Vec<BeadId> }`, `NotFastForward { branch, to }`, `Conflict { paths }`, `Budget { exceeded }`. A few payloads still carry rendered text as a `detail` string, including `MergeConflict` and `ReleaseLoop`. Incidents, `factory inbox`, and the event log render these; agents act on them without parsing strings.
- **Parse, don't validate — including model output.** Everything from outside — `bd`/`git` output, harness JSON, CLI arguments, plan text — is decoded exactly once at the boundary (`Raw*` + `TryFrom`) into types that cannot hold an invalid value. Domain code never sees a `String` where a `VerifyCommand`, `Title`, or `NonEmpty<T>` is meant.
- **Capabilities are injected.** Time, randomness, IDs, the ledger, git, and the LLM harness arrive as ports. The domain crate compiles without any of them.
- **Sagas for multi-step effects.** The Integrator's rebase → checks → fast-forward → push is a compensation stack: if push fails after the fast-forward, `main` is rolled back (compare-and-swap) so the retry starts clean; Drop cannot await, so compensation is explicit code.
- **Gaps are repaired, not hidden.** Because state is persisted before effects, a crash mid-effect leaves a visible symptom (a `mergeable` task with no merge bead). The Steward's sweep detects and repairs it idempotently; `app::transition` tests force the failure with a flaky store to prove the gap is detectable.

## Compatibility rule

Stored ledger metadata is a public boundary. `META_VERSION` bumps only when a field's meaning changes; tightening lives in the `Raw*` decoders, which must keep accepting every previously written shape (fixture-tested). A bump would need a migration; `factory migrate` does not exist yet.

## Enforcement

`cargo xtask lint-fp` (the skill's mechanical sweep, with `// fp-allow: <why>` for justified exceptions), deny-tier clippy, `clippy.toml` path bans, `cargo mutants` on `domain`, and the full `skills/rust-fp-skill/references/code-review.md` walk recorded in the active exec plan.
