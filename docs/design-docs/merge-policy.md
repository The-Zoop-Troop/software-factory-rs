# Merge policy

- **Status:** accepted · **Verified:** 2026-11-02 against `app::integrator`; its tests pass

**Inside the factory.** The Integrator lands one merge bead at a time: rebase onto `main` → run project checks (`--check`, repeatable) → fast-forward with compare-and-swap → push (if a remote is configured). A conflict or failing check reopens the task with the output (an attempt); infrastructure failure leaves it mergeable for retry. Batch-then-bisect is Phase 1.

**For this repository.** Short-lived branches; PRs merge when CI is green; flaky failures are re-run, not debated; agent review before human review; humans may review but are not required to. Corrections are cheap, waiting is expensive.

## Landing onto what the remote has

When a remote is configured (`RIG_REMOTE`), before rebasing a task branch the Integrator fetches
the integration branch and fast-forwards its local copy when it is strictly behind (an operator's
hand commit, another rig's push). If the two have diverged it rejects the landing with `Diverged`
— the task is reopened with the reason and nothing is pushed; an operator reconciles by hand. Each
rejection charges an attempt, so repeated divergence becomes a `MergeConflict` incident. Learned on the
guide project's portal rig, where one hand commit made every landing fail three times a poll.
