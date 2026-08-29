# Core beliefs

- **Status:** accepted · **Verified:** by construction; each belief names its enforcement

1. **The repository is the agent's whole world.** If a decision, contract, or fact is not in the repo, it does not exist. Chat, docs elsewhere, and heads are not sources of truth. *Enforced:* exec plans with decision logs; `xtask lint-docs`.
2. **The ledger is the truth for work.** Beads holds every task, dependency, lease, budget, and note. Agent context is cache. *Enforced:* every state change goes through `apply_event`; no other path writes task state.
3. **Humans steer; agents execute; verification decides.** A model's claim of success carries no weight. *Enforced:* only the Verifier advances a task; only the Integrator lands it.
4. **Invariants over instructions.** Prefer a lint with a remediation message to a paragraph of guidance. When guidance fails twice, promote it to code. *Enforced:* deny-tier clippy, `clippy.toml` path bans, structural tests, taste lints.
5. **Small, legible, boring.** Boring dependencies, explicit boundaries, files that fit in a context window. *Enforced:* 600-line cap; layering test.
6. **Pay debt continuously.** Drift is garbage-collected on a schedule, not in heroic sprints. *Enforced:* `QUALITY_SCORE.md` grades decay unless re-verified; gardening run files beads.
7. **Errors are data on a railway.** Every failure is a typed value with a payload the next agent can branch on; control flow is a total state machine. *Enforced:* `xtask lint-fp`, deny-tier clippy, mutation testing on `domain`; see `railway.md`.
8. **Throughput changes merge philosophy.** Short-lived branches, minimal blocking gates, retries over long waits — because corrections are cheap and waiting is expensive. *See* `merge-policy.md`.
