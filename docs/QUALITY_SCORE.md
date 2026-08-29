# Quality score

- **Status:** accepted · **Verified:** 2026-08-29 (`cargo xtask coverage`: 91.6% lines, gate 85). Grades decay to `?` if not re-verified within 30 days.

| Area | Tests | Coverage | Docs | Lints | Grade | Notes |
|---|---|---|---|---|---|---|
| `domain` | 32 | 97% | state-machine.md | deny tier | A | `cargo mutants`: 79/82 viable caught (96%) |
| `app` | 27 | high | design-docs | deny tier | A- | fakes in `testing.rs` |
| `infra` | 19 + 5 integration + 3 live | 85%+ | harness-port.md, references | deny tier | A- | real `git`/`bd` tests; fake `claude`/`codex`/`opencode` binaries in `tests/fakebin` |
| `factory` / `stewardd` bins | 9 | cli ~70% / run 98% | generated/cli.md | allow list | B+ | doctor/watch/inbox; dispatch arms need adapter injection (debt) |
| rig (`docker/`, compose) | acceptance script + `factory doctor` | n/a | DEPLOYMENT, SECURITY, rig-sandbox.md | `compose config` in CI | B+ | per-project toolchain layer is debt |
| docs | `lint-docs`, `gen-docs --check`, `quality --check` | n/a | this tree | CI + weekly gardening | A- | freshness enforced at 30 days |

## Measured

<!-- quality:begin -->
| Measure | Value | Measured |
|---|---|---|
| Line coverage (excl. xtask) | 90.83% | 2026-08-29 |
| Tests (nextest) | 104 tests run: 104 passed, 2 skipped | 2026-08-29 |
<!-- quality:end -->
