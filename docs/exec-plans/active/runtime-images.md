# Exec plan: runtime images

- **Status:** active · **Owner:** human steers, agents execute · **Started:** 2026-08-29
- **Beads epic:** `fac-cvv`
- **Related:** `docs/design-docs/rig-sandbox.md`, `docs/DEPLOYMENT.md`, debt item "per-project toolchain baked into one image"

## Goal

Split the single rig image into a small **base** (factory binaries, `bd`, `git`, the three harness
CLIs, agent ergonomics tools, sandbox user/entrypoint) and **runtime images** that extend it with
one language toolchain each. A rig picks a runtime; a project may bring its own image `FROM` the
base or a runtime; when nothing is declared, the base is used. Every runtime is proven by a
conformance test that runs verify commands exactly as the Verifier does.

## Non-goals

- Relaxing the sandbox for any runtime (still non-root, `cap_drop: ALL`, internal network, allowlisted egress).
- Letting an agent choose the image. The image is a security boundary; humans (or the registry) set it.
- Multi-architecture builds in v1 (x86_64 only; arm64 is a follow-up).

## Design

```
ghcr.io/the-zoop-troop/rig-base          factory, stewardd, bd, git, make, claude/opencode/codex,
                                          rg fd jq yq shellcheck shfmt sqlite3 gh just git-lfs, uid 10001, entrypoint
   ├─ rig-rust      rustup (pinned), nextest, llvm-cov                cache: ~/.cargo/registry, target
   ├─ rig-python    uv (Python versions, venvs, pyproject/lock), ruff, pytest   cache: ~/.cache/uv
   ├─ rig-node      Node LTS via corepack, pnpm (+npm/yarn)          cache: ~/.local/share/pnpm/store
   ├─ rig-go        Go toolchain, golangci-lint                      cache: ~/go/pkg/mod, ~/.cache/go-build
   ├─ rig-jvm  (v2) Temurin JDK LTS, Maven, Gradle                   cache: ~/.m2, ~/.gradle
   ├─ rig-c-cpp(v2) gcc/clang, cmake, ninja, ccache, conan           cache: ~/.ccache, ~/.conan2
   ├─ rig-web-e2e(v2) rig-node + Playwright browsers                 sidecar-free browser testing
   ├─ rig-ruby (v2)  Ruby via ruby-build, bundler, Rails deps (libpq, node for assets)   cache: ~/.bundle, vendor/bundle
   ├─ rig-php  (v2)  PHP LTS + composer, common extensions (pdo, mbstring, intl)         cache: ~/.composer/cache
   ├─ rig-elixir(v2) Erlang/OTP + Elixir, hex, rebar3, mix                               cache: ~/.hex, ~/.mix, _build
   ├─ rig-dotnet (on demand)
   └─ rig-polyglot(v2) rust+python+node for monorepos (opt-in, large)
project-provided: .factory/Dockerfile  FROM ghcr.io/the-zoop-troop/rig-<runtime>:<tag>  (+ project deps)
```

- **Selection** (`RIG_IMAGE` in compose): project `.factory/Dockerfile` if present → else the rig's
  declared `RIG_RUNTIME` → else `rig-base`. Resolved by `factory rig` / the compose env, logged by
  `doctor`.
- **Egress per runtime**: each runtime ships `allowlist.fragment` (pypi/npm/proxy.golang.org/maven…);
  the egress config is the concatenation of base + selected runtime + project fragment.
- **Caches**: named volumes per rig per runtime (table above) so worktree-per-task stays fast.
- **Skills/plugins/MCP are repo-carried, not image-baked**: harness skills travel with the project
  repo (picked up in the worktree) or the factory's `skills/` submodules; MCP servers are declared
  per rig in `.factory/mcp.json`, hosts allowlisted, passed through by the adapters.
- **Service sidecars**: a runtime may declare compose profiles (`postgres`, `redis`, `browser`) that
  the Verifier can start for a task's verify commands (v2).
- **Pinning**: every toolchain version pinned with checksums where installers support them;
  images tagged `<runtime>-<yyyymmdd>` and `<runtime>-latest`; base rebuild triggers all runtimes.

## Testability

1. `docker/runtimes/<name>/conformance.sh` runs inside the image: pinned versions present;
   `factory doctor` green; `sample/` project builds and tests; its verify commands run under
   `/bin/sh` from the repo root with the repo root on `PATH` — exactly the Verifier's contract.
2. `.github/workflows/runtimes.yml`: matrix build of base + every runtime, conformance per image,
   publish to GHCR on `main`.
3. Nightly end-to-end: the unattended plan → closed-epic acceptance against each runtime's sample
   project with the OpenCode worker.
4. `factory doctor` gains a `runtime` check that reads `.factory/runtime.toml` and reports missing
   tools *with the fix*, so an agent files a bead instead of looping on `command not found`.

## Epics (in order)

1. **base-split** — `docker/base/Dockerfile` (no Rust toolchain; ergonomics tools added),
   `docker/runtimes/rust/Dockerfile` reproducing today's image; `RIG_IMAGE`/`RIG_RUNTIME`
   selection in compose and entrypoint; `doctor runtime` check; docs.
2. **runtimes-v1** — `python` (uv), `node` (pnpm), `go`; each with `sample/`, `conformance.sh`,
   `allowlist.fragment`, cache volume; `.factory/runtime.toml` schema.
3. **ci-and-registry** — `runtimes.yml` matrix, GHCR publishing with date tags, base→runtime
   rebuild trigger, nightly e2e per runtime.
4. **byo-and-mcp** — project `.factory/Dockerfile` support, project allowlist fragment, per-rig
   `.factory/mcp.json` passed through to all three harnesses, skills mount reported by `doctor`.
5. **runtimes-v2** — `jvm`, `c-cpp`, `web-e2e` (Playwright), `ruby` (Rails), `php`, `elixir`,
   `polyglot`; service sidecars (`postgres`, `redis`) as compose profiles the Verifier can start.

## Acceptance

- `docker compose build` with `RIG_RUNTIME=python` yields an image where `uv run pytest` passes on
  the sample and `factory doctor` is green; same for `node` and `go`.
- A project with `.factory/Dockerfile` `FROM rig-node` builds and runs an unattended epic.
- The runtimes CI matrix is green and images are on GHCR.
- No runtime weakens the sandbox: `CapEff` is zero and `example.com` is refused in every image.

## Decision log

- 2026-08-29 — Skills, plugins and MCP are repo-carried; images carry toolchains and OS tools only.
- 2026-08-29 — The image is chosen by humans/registry, never by an agent.
- 2026-08-29 — v1 runtimes: rust, python (uv), node (pnpm), go. v2: jvm, c-cpp, web-e2e, ruby (Rails), php, elixir, polyglot (user).

## Progress

- [x] base-split (`factory-rig:base` on debian-slim + ergonomics tools; `factory-rig:rust` layer; `docker/build.sh` assembles egress allowlist and runs conformance; compose `RIG_IMAGE`; `doctor` runtime check from `.factory/runtime.toml`) — 2026-08-29
- [x] runtimes-v1 (python/uv, node/pnpm, go — samples, conformance, allowlist fragments, cache paths; all green locally) — 2026-08-29
- [x] ci-and-registry (`runtimes.yml`: matrix base/rust/python/node/go, conformance + sandbox invariants, GHCR publish with date tags, weekly rebuild) — 2026-08-29
- [x] byo-and-mcp (`.factory/Dockerfile` + `.factory/allowlist` via build.sh; `.factory/mcp.json` → Claude/OpenCode/Codex; skills + MCP reported by doctor) — 2026-08-29
- [ ] runtimes-v2
