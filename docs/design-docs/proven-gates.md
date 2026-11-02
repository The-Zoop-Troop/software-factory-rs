# Proven gates

- **Status:** accepted · **Verified:** 2026-11-02 against the code at tag `PRE_PROVEN_GATES` and a static reading of six rigs' history. Nothing below is implemented yet; the thresholds are judgment until measured.

A gate earns trust by catching planted faults at a known rate. That rate, together with how
reversible a change is, decides how much of the factory runs without a human.

The work is tracked in `docs/exec-plans/active/proven-gates.md`.

## Why

A 90% first-pass rate looked healthy. It mostly measured checks that could not fail.

Six rigs' backups hold 69 verify beads, 68 of which landed and 61 of which passed first time:

| What the verify commands checked | Tasks | Share |
|---|---|---|
| Whole test suite or lint only | 33 | 48% |
| A file or some text exists | 18 | 26% |
| A targeted test | 18 | 26% |

Half of the 30 targeted commands were `cargo test <name filter>`, which exits 0 when nothing
matches. Only about 11 of the 69 tasks had a behaviour check known to fail before the work. This
is a static reading rather than an executed sweep, and it shows the checks are weak, not that the
landed code is bad.

Published work points the same way. METR's March 2026 study of test-passing pull requests found
that roughly half would not have been merged by maintainers.

The writer can also reach what judges it:

- `crates/infra/src/shell.rs:26` puts the repo root first on `PATH`, so a committed `cargo` script
  replaces the real one for the Verifier. A standalone shell test confirmed the mechanism.
- `crates/app/src/verifier.rs:177` reads `[verify] prepare` from the candidate's worktree, so the
  branch under test chooses commands the Verifier runs.
- Nothing detects a Worker weakening an existing test.
- Harness children inherit the role's environment, and `compose.yaml` and
  `docker/entrypoint.sh` give every role the same env file and git token. "Only the Integrator
  pushes" is therefore a convention.
- For the same reason every role holds the ledger password, and each mounts the ledger read-write
  at `/work/rig`, so a Worker session could change its own task's state or edit the event log.
  Removing the password alone would not close this, because the files stay reachable.

The last two come from reading the files, not a test in a running rig.

No run history shows a Worker exploiting any of this.

## Terms

- **Check:** a command with a pass or fail result. A check that names new behaviour must fail on
  the base commit. A **guard** must pass on the base commit and still pass afterwards.
- **Hidden check:** a check the Worker cannot read or edit. It gives coarse feedback only.
- **Gate:** anything that can stop a change: checks, the Verifier, a reviewer, a human.
- **Planted fault:** a known-bad change fed to a gate on purpose: a mutant, a plausible wrong
  patch, or an impossible task.
- **Catch rate:** planted faults stopped divided by planted faults tried, reported with a 95%
  lower bound. Zero misses in 30 still allows a true miss rate near 10%; zero in 100, near 3%.
- **Escape:** a change that passed every gate and a human later judged wrong.
- **Accepted result:** a change that landed and was not reverted. Cost is reported per accepted
  result rather than per attempt.

## What "proven" means

Agreed before any results exist, so the bar cannot drift. A Worker setup is proven when:

1. Every writer-to-checker hole above has a test proving it closed.
2. Across at least 50 planted faults, the gates together miss no more than 5, which gives at least
   80% at the 95% lower bound.
3. In a blind human audit, its diffs are rejected about as often as commits the human had already
   accepted.
4. These hold for that setup. One setup can qualify while another does not.

Being proven does not grant landing rights. The first real work on a real repository is a branch
and a pull request a human reviews. Autonomy beyond that is earned per change class under the
policy in phase 6.

## Stop rules

- **After the baseline:** if about 80% of mutants are caught and the audit finds few escapes, close
  the writer-to-checker holes, make checks able to fail, and stop.
- **During the gate lab:** if catch rates do not track audited escapes, keep the lab only as a
  debugging tool and do not let it set autonomy.

## What stays

The ledger graph, pull-based scheduling with no orchestrator, stateless Workers, one writer per
task, the non-LLM Verifier, Integrator and Steward, the sandbox, the event log and the blocked-note
route. The factory's outer loop is already deterministic; the model never decides what runs next.

## Not doing

A graph framework, a central state machine beyond `Task::apply`, panels of reviewers from several
vendors as a default, or renames. Batch-then-bisect, flaky detection and tree-sitter context wait
until measurements justify them.
