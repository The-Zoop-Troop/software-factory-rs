# Runtime images
- **Status:** accepted · **Verified:** 2026-08-29 (`docker/build.sh rust --conformance`)

| Image | Adds to `factory-rig:base` | Caches under `/work/cache` | Egress fragment |
|---|---|---|---|
| `factory-rig:base` | nothing — factory binaries, `bd`, git, make, harness CLIs, `rg fd jq yq shellcheck shfmt sqlite3 gh just git-lfs` | — | `docker/egress/allowlist.base` |
| `factory-rig:rust` | rustup 1.97.1 (rustfmt, clippy, llvm-tools), cargo-nextest, cargo-llvm-cov, gcc/pkg-config/libssl | `rustup/`, `cargo/` | crates.io, static.rust-lang.org |
| `factory-rig:python` | uv 0.12.7 (Python 3.13 managed, venvs, pyproject/lock), ruff, pytest, build-essential | `uv/`, `uv-python/` | pypi.org, files.pythonhosted.org |
| `factory-rig:node` | Node 24 LTS, corepack, pnpm 11 (npm/yarn via corepack) | `pnpm/`, `npm/`, `corepack/` | registry.npmjs.org, nodejs.org |
| `factory-rig:go` | Go 1.27, golangci-lint 2.13, gcc | `go/`, `go-build/` | proxy.golang.org, sum.golang.org, go.dev |
| `factory-rig:jvm` | OpenJDK 17 (bookworm), Maven 3.9.16, Gradle via project wrapper | `m2/`, `gradle/` | repo1.maven.org, plugins.gradle.org, services.gradle.org |
| `factory-rig:c-cpp` | gcc/g++, clang, cmake, ninja, ccache, gdb, conan 2 | `ccache/`, `conan2/` | center.conan.io, pypi.org |
| `factory-rig:ruby` | Ruby 3.1 (bookworm), bundler, libpq/sqlite/yaml headers (Rails-ready) | `bundle/`, `gems/` | rubygems.org |
| `factory-rig:php` | PHP 8.2 CLI + xml/mbstring/intl/curl/zip/sqlite/pgsql, Composer 2.10 | `composer/` | packagist.org, getcomposer.org |
| `factory-rig:elixir` | Erlang/OTP + Elixir (bookworm), hex, rebar3 | `mix/`, `hex/` | hex.pm, repo.hex.pm, github.com |
| `factory-rig:web-e2e` | `node` + Playwright 1.62 with Chromium and its system deps | node caches + `ms-playwright/` | node + playwright.azureedge.net, cdn.playwright.dev |
| `factory-rig:polyglot` | `rust` + uv/Python 3.13 + Node 24/pnpm on one image (monorepos; large) | union of rust/python/node | union of rust/python/node |

Language versions in the apt-sourced images (jvm, ruby, php, elixir) follow Debian bookworm; a project
needing newer ones adds them in `.factory/Dockerfile`. `web-e2e` and `polyglot` are layered on `node` /
`rust`, which `docker/build.sh` builds first (parent read from the runtime Dockerfile's `ARG BASE`).

Verifier sidecars: `docker compose --profile postgres up -d postgres` / `--profile redis up -d redis`
put a throwaway Postgres 17 (`postgres:5432`, user/password/db `factory`, tmpfs) or Redis 7 (`redis:6379`)
on the rig network for verify commands that need a datastore; they are not part of the egress allowlist
because they never leave the network.

Selection order: project `.factory/Dockerfile` (built `FROM` the runtime) → `RIG_IMAGE` → `factory-rig:rust`.
A project declares its needs in `.factory/runtime.toml` (`[runtime] name`, `[[tools]] bin/version_cmd`);
`factory doctor` checks them and names the runtime image that provides what is missing.

Build: `docker/build.sh <runtime> [--project <dir>] [--conformance]` builds base → runtime → project
image, assembles `docker/egress/allowlist` from base + runtime + project fragments, builds the
egress proxy, and optionally runs `docker/runtimes/conformance.sh` against the runtime's `sample/`
(verify commands executed exactly as the Verifier does: `/bin/sh`, repo root, repo root on `PATH`).

Published images: `ghcr.io/the-zoop-troop/rig-base` and `ghcr.io/the-zoop-troop/rig-<runtime>`, tags `<yyyymmdd>` and `latest`, built by `.github/workflows/runtimes.yml` (matrix + conformance on every change under `docker/`).

## Bring your own image, MCP servers, skills

- `.factory/Dockerfile` in the project (`ARG BASE` / `FROM ${BASE}`; `docker/build.sh <runtime> --project <dir>` passes the runtime image as `BASE`) adds project-specific tools; `.factory/allowlist` adds egress hosts. The sandbox is unchanged: `doctor` and conformance still require uid 10001 and zero capabilities.
- `.factory/mcp.json` (`{"mcpServers": {name: {command, args, env} | {url, headers}}}`) is parsed once (`app::mcp`) and rendered per harness: Claude `--mcp-config … --strict-mcp-config`, OpenCode `mcp` config block, Codex `-c mcp_servers.<name>.*`. Remote hosts must be in the allowlist; `doctor` lists them.
- Skills travel with the project repo (`.claude/skills`, `.codex/skills`, `.opencode/skills`, `.agents/skills`) and are picked up by the harness in the worktree; `doctor` reports what it sees. Nothing skill-related is baked into images.
