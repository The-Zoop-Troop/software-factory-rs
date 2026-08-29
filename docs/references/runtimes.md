# Runtime images
- **Status:** accepted · **Verified:** 2026-08-29 (`docker/build.sh rust --conformance`)

| Image | Adds to `factory-rig:base` | Caches under `/work/cache` | Egress fragment |
|---|---|---|---|
| `factory-rig:base` | nothing — factory binaries, `bd`, git, make, harness CLIs, `rg fd jq yq shellcheck shfmt sqlite3 gh just git-lfs` | — | `docker/egress/allowlist.base` |
| `factory-rig:rust` | rustup 1.97.1 (rustfmt, clippy, llvm-tools), cargo-nextest, cargo-llvm-cov, gcc/pkg-config/libssl | `rustup/`, `cargo/` | crates.io, static.rust-lang.org |

Selection order: project `.factory/Dockerfile` (built `FROM` the runtime) → `RIG_IMAGE` → `factory-rig:rust`.
A project declares its needs in `.factory/runtime.toml` (`[runtime] name`, `[[tools]] bin/version_cmd`);
`factory doctor` checks them and names the runtime image that provides what is missing.

Build: `docker/build.sh <runtime> [--project <dir>] [--conformance]` builds base → runtime → project
image, assembles `docker/egress/allowlist` from base + runtime + project fragments, builds the
egress proxy, and optionally runs `docker/runtimes/conformance.sh` against the runtime's `sample/`
(verify commands executed exactly as the Verifier does: `/bin/sh`, repo root, repo root on `PATH`).
