# Quality score

- **Status:** accepted · **Verified:** 2026-08-29 (`cargo xtask coverage`: 91.6% lines, gate 85). Grades decay to `?` if not re-verified within 30 days.

| Area | Tests | Coverage | Docs | Lints | Grade | Notes |
|---|---|---|---|---|---|---|
| `domain` | 32 | 97% | state-machine.md | deny tier | A | tests split into `task_tests.rs`; mutants run pending |
| `app` | 27 | high | design-docs | deny tier | A- | fakes in `testing.rs` |
| `infra` | 19 + 5 integration + 3 live | 85%+ | harness-port.md, references | deny tier | A- | real `git`/`bd` tests; fake `claude`/`codex`/`opencode` binaries in `tests/fakebin` |
| `factory` / `stewardd` bins | 6 | cli 68% / run 98% | README | allow list | B | logic lives in `cli.rs`/`run.rs`; `main.rs` is a shim |
| rig (`docker/`, compose) | acceptance script | n/a | DEPLOYMENT, SECURITY | none | B | needs `factory doctor` |
| docs | lint pending | n/a | this tree | `xtask lint-docs` pending | B- | generated docs missing |
