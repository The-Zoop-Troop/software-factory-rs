# Runtime images
- **Status:** accepted · **Verified:** 2026-08-29 (`docker/build.sh rust --conformance`)

| Image | Adds to `factory-rig:base` | Caches under `/work/cache` | Egress fragment |
|---|---|---|---|
| `factory-rig:base` | nothing — factory binaries, `bd`, git, make, harness CLIs, `rg fd jq yq shellcheck shfmt sqlite3 gh just git-lfs` | — | `docker/egress/allowlist.base` |
| `factory-rig:rust` | rustup 1.97.1 (rustfmt, clippy, llvm-tools), cargo-nextest, cargo-llvm-cov, gcc/pkg-config/libssl | `rustup/`, `cargo/` | crates.io, static.rust-lang.org |
| `factory-rig:python` | uv 0.12.7 (Python 3.13 managed, venvs, pyproject/lock), ruff, pytest, build-essential | `uv/`, `uv-python/` | pypi.org, files.pythonhosted.org |
| `factory-rig:node` | Node 24 LTS, corepack, pnpm 11 (npm/yarn via corepack) | `pnpm/`, `npm/`, `corepack/` | registry.npmjs.org, nodejs.org |
| `factory-rig:go` | Go 1.27, golangci-lint 2.13, gcc | `go/`, `go-build/` | proxy.golang.org, sum.golang.org, go.dev |

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
