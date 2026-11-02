# Quality score

- **Status:** accepted · **Verified:** 2026-11-02 (coverage and test counts measured; grades are set by hand). `cargo xtask quality --check` fails when this date or the measured block is over 30 days old and code has changed since.

| Area | Tests | Coverage | Docs | Lints | Grade | Notes |
|---|---|---|---|---|---|---|
| `domain` | 76 | 97.6% | state-machine.md, railway.md | deny tier + lint-fp | A | `cargo mutants` on 2026-08-29: 90/93 viable caught (97%); v1 metadata fixture |
| `app` | 81 | 95.7% | design-docs | deny tier | A- | fakes in `testing.rs` |
| `infra` | 37, plus live tests run with `--ignored` | 84.4% | harness-port.md, references | deny tier | A- | real `git`/`bd` tests; fake `claude`/`codex`/`opencode` binaries in `tests/fakebin` |
| `console` | 36 | 83.9% | remote-control.md, web-console.md | deny tier | B+ | the UI has its own gate, `cargo xtask ui-check` |
| `factory` / `stewardd` bins | 22 | 68.5% / 62.0% | generated/cli.md | allow list | B+ | doctor/watch/inbox; dispatch arms need adapter injection (debt) |
| rig (`docker/`, compose) | acceptance script + `factory doctor` | n/a | DEPLOYMENT, SECURITY, rig-sandbox.md | `compose config` in CI | B | the writer can still reach what checks it; see `docs/design-docs/proven-gates.md` |
| docs | `lint-docs`, `gen-docs --check`, `quality --check` | n/a | this tree | CI + weekly gardening | A- | freshness enforced at 30 days once code has changed |

## Measured

<!-- quality:begin -->
| Measure | Value | Measured |
|---|---|---|
| Line coverage (excl. xtask) | 88.03% | 2026-11-02 |
| Tests (nextest) | 251 tests run: 251 passed, 2 skipped | 2026-11-02 |
<!-- quality:end -->
