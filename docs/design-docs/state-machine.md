# Task state machine

- **Status:** accepted · **Verified:** `cargo test -p domain` (`task::tests`) 2026-08-29; diagram mirrors `domain::task`

```
open ──claim──▶ leased ──submit──▶ in_verify ──pass──▶ mergeable ──merged──▶ closed
                 │  │                  │                   │
   lease expired │  │ release          │ fail              │ merge failed
                 ▼  ▼                  ▼                   ▼
                open (attempt++ on release/fail; note appended)
                 └── attempts/tokens/wall-clock budget exceeded, or ≥3 lease expiries ──▶ incident
```

- `Task::apply(Event) -> Result<Transition, IllegalTransition>` is total over (state, event); illegal pairs are listed exhaustively so a new variant breaks the build.
- A `Transition` carries **effects** (`AppendNote`, `OpenMergeBead`, `OpenIncidentBead`, `CloseTaskBead`, `CloseVerifyBead`) that the imperative shell (`app::transition::run_effect`) executes after persisting the new state — persist first, so a crash leaves a detectable gap rather than an undoable one.
- **Heartbeat** renews a lease from *now*, never from the old expiry. Only the lease holder may heartbeat, submit, or release.
- **Budgets** (`domain::Budget`): attempts are checked first because they mean "the model keeps failing".
