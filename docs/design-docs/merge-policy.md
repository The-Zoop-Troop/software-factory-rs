# Merge policy

- **Status:** accepted · **Verified:** `app::integrator` tests 2026-08-29

**Inside the factory.** The Integrator lands one merge bead at a time: rebase onto `main` → run project checks (`--check`, repeatable) → fast-forward with compare-and-swap → push (if a remote is configured). A conflict or failing check reopens the task with the output (an attempt); infrastructure failure leaves it mergeable for retry. Batch-then-bisect is Phase 1.

**For this repository.** Short-lived branches; PRs merge when CI is green; flaky failures are re-run, not debated; agent review before human review; humans may review but are not required to. Corrections are cheap, waiting is expensive.

## Landing onto what the remote has

Before rebasing a task branch the Integrator fetches the integration branch from the remote and
fast-forwards its local copy when it is strictly behind (an operator's hand commit, another
rig's push). If the two have diverged it rejects the landing with `Diverged` — the task is
reopened with the reason and nothing is pushed; an operator reconciles by hand. Learned on the
guide project's portal rig, where one hand commit made every landing fail three times a poll.
