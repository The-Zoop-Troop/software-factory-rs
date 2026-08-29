# Quality score

- **Status:** accepted · **Verified:** 2026-08-29 (`cargo llvm-cov`: 77.4% lines overall). Grades decay to `?` if not re-verified within 30 days.

| Area | Tests | Coverage | Docs | Lints | Grade | Notes |
|---|---|---|---|---|---|---|
| `domain` | 29 | high | state-machine.md | deny tier | A | `task.rs` 856 lines — over the cap (debt) |
| `app` | 27 | high | design-docs | deny tier | A- | fakes in `testing.rs` |
| `infra` | 15 + 3 live | medium | harness-port.md, references | deny tier | B | adapters need real-`bd`/`git` tests |
| `factory` / `stewardd` bins | 0 | 0% | README | allow list | C | move logic out of `main.rs` |
| rig (`docker/`, compose) | acceptance script | n/a | DEPLOYMENT, SECURITY | none | B | needs `factory doctor` |
| docs | lint pending | n/a | this tree | `xtask lint-docs` pending | B- | generated docs missing |
