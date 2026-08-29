# bd 1.2.2 — what the factory relies on
- **Status:** reference · **Verified:** 2026-08-29 against the binary

- JSON everywhere: `bd show <id> --json` (array of one), `bd ready --json`, `bd list --json`, `bd create --json` (`{id}`), `bd update … --json`.
- `bd show` JSON omits `labels`/`metadata`/`notes` when empty — decode with defaults.
- `--metadata '<json>'` **replaces** the whole metadata object; `--set-metadata k=v` merges flat keys.
- `--labels a,b` at create; children inherit parent labels unless `--no-inherit-labels`.
- Dependencies: `bd dep add <dependent> <blocker>` = dependent NEEDS blocker. `bd create --deps blocks:X` means "this bead **blocks** X" (opposite). `bd close` refuses a bead with open blockers unless `--force`.
- `--defer <when>` hides a bead from `bd ready`; `bd update <id> --defer ""` clears it.
- Ephemeral beads (`--ephemeral`) are hidden from `bd list`.
- No native leases/heartbeats in 1.2.2 (`bd heartbeat`/`reclaim` are unknown commands) — the factory keeps leases in metadata.
- `bd list --status open,in_progress` (comma form); `--all` includes closed; `--parent <id>` lists children.
- Errors go to stderr with exit 1; "no issue found matching" ⇒ not found.
- `bd metrics off` disables telemetry; the rig entrypoint runs it.
