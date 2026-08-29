# Golden principles

- **Status:** accepted · **Verified:** each rule names its enforcer; "review" means not yet mechanical (file debt). The standard behind these rules is `skills/rust-fp-skill` (pinned submodule); `references/code-review.md` there is the mandatory self-review.

| Rule | Why | Enforcer |
|---|---|---|
| Parse at the boundary, once (`TryFrom`, `#[serde(try_from)]`) | Domain code never sees an invalid value | clippy `as_conversions` deny; review |
| Errors are typed data (`thiserror` enums) **with actionable payloads, never `String`**; `anyhow` only in `main.rs` | Callers and agents branch on the variant, not on prose | `clippy.toml` `disallowed-types`; `xtask lint-fp` |
| Adapters parse external output once into typed variants; no substring matching on `Display` | Retry/branch predicates read fields | `xtask lint-fp` |
| Justified exceptions carry `// fp-allow: <why>` | Every hit is a decision, not a habit | `xtask lint-fp` |
| No `unwrap`/`expect`/`panic` outside binaries and tests | Totality; a factory must not die on a bad row | clippy deny tier |
| Time, randomness, and IDs are injected (`Clock`, parameters) | Determinism; tests never sleep | `clippy.toml` bans `SystemTime::now` |
| Exhaustive matches over domain enums | Adding a variant breaks the build, not production | clippy `wildcard_enum_match_arm` deny |
| Ports, not mocks: hand-written fakes in `app::testing` | Tests document the contract | review |
| Every newtype has a round-trip property test; every aggregate an invariant test | Proofs, not examples | `crates/domain/tests/properties.rs`; `cargo mutants ≥ 90%` |
| Docs are re-verified within 30 days or marked superseded | Stale guidance is worse than none | `xtask quality --check` (weekly `gardening` workflow) |
| `docs/generated` is never hand-edited | It is derived from code | `xtask gen-docs --check` in CI |
| Layering `domain ← app ← infra ← bins` | Cargo enforces the onion | structural test (`xtask lint-taste`) |
| Every `Effect` the domain emits is executed by exactly one place (`transition::run_effect`) | One imperative shell | manual |
| Custom lint messages tell the reader how to fix it | The reader is usually an agent | convention in `xtask` |
| Files ≤ 600 lines | Fits a context window with room for the task | `xtask lint-taste` |
| No `println!` outside binaries; `tracing` with fields | Logs are queryable | `xtask lint-taste` |
| Never `pkill -f <pattern>` in scripts | It matches the calling shell | `xtask lint-taste` |
| Verify commands are POSIX `sh`; repo root is on `PATH` | Models write `. lib.sh`; make it work rather than argue | `ShellRunner` |
| New beads are deferred until their `needs` edges exist | Otherwise a polling worker claims them first | `BdCli::create` |
