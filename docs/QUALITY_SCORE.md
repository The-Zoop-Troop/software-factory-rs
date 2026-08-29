# Quality score

- **Status:** accepted · **Verified:** 2026-08-29 (`cargo xtask coverage`: 91.6% lines, gate 85). Grades decay to `?` if not re-verified within 30 days.

| Area | Tests | Coverage | Docs | Lints | Grade | Notes |
|---|---|---|---|---|---|---|
| `domain` | 54 unit + 10 property + fixture | 97% | state-machine.md, railway.md | deny tier + lint-fp | A | `cargo mutants`: 90/93 viable caught (97%); v1 metadata fixture |
| `app` | 27 | high | design-docs | deny tier | A- | fakes in `testing.rs` |
| `infra` | 19 + 5 integration + 3 live | 85%+ | harness-port.md, references | deny tier | A- | real `git`/`bd` tests; fake `claude`/`codex`/`opencode` binaries in `tests/fakebin` |
| `factory` / `stewardd` bins | 9 | cli ~70% / run 98% | generated/cli.md | allow list | B+ | doctor/watch/inbox; dispatch arms need adapter injection (debt) |
| rig (`docker/`, compose) | acceptance script + `factory doctor` | n/a | DEPLOYMENT, SECURITY, rig-sandbox.md | `compose config` in CI | B+ | per-project toolchain layer is debt |
| docs | `lint-docs`, `gen-docs --check`, `quality --check` | n/a | this tree | CI + weekly gardening | A- | freshness enforced at 30 days |

## Measured

<!-- quality:begin -->
| Measure | Value | Measured |
|---|---|---|
| Line coverage (excl. xtask) | 89.93% | 2026-08-29 |
| Tests (nextest) | 131 tests run: 131 passed, 2 skipped | 2026-08-29 |
<!-- quality:end -->
