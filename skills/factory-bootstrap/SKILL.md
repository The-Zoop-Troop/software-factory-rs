---
name: factory-bootstrap
description: Stand up a software factory from a bare Linux host — build the rig images, prepare project repositories, create rigs, bring up the shared console, and verify every step. Use when asked to install, set up, bootstrap, or create a factory, a rig, or run a new project through the factory for the first time. Docker (rootless) deployment only.
---

# Factory bootstrap — from bare host to first landed task

You are standing up an **autonomous software factory**: plan in → verified, merged code out.
This skill takes a Linux host from nothing to running rigs with a console over them, with a
verification gate after every step. Run it top to bottom for a new host; jump to a numbered
step when only part is missing.

**Sources of truth** (prefer them over memory when they disagree with this skill):
`README.md` (quick start) · `docs/DEPLOYMENT.md` (runbook) · `docs/guides/first-project.md`
(worked end-to-end example) · `docs/SECURITY.md` (threat model) ·
`docs/references/runtimes.md` (images) · `docs/generated/cli.md` (exact flags).

## The shape of what you are building

| Piece | What it is | Where it lives |
|---|---|---|
| Rig | One project = one sandboxed compose project (`factory-<name>`) | volumes `ledger`, `repo`, `cache` |
| Roles | `steward`, `verifier`, `integrator`, `worker`×N, `planner` — services sharing one runtime image | `compose.yaml` in this repo |
| Ledger | The rig's Dolt SQL server over its bead database; first up, health-checked | `ledger` service, volume `/work/rig` |
| Egress | The only route out: tinyproxy with a default-deny domain allowlist | `egress` service |
| Console | The one externally reachable process: A2A API + embedded browser UI, over every rig's ledger | `factory rig console`, port 7700 |
| Host files | Registry, per-rig env + secrets, console config | `~/.factory` (`FACTORY_ROOT`) |

## Workflow

Each step ends with a **gate**. Do not continue past a failed gate — fix it first;
`references/host-and-deployment.md` and the operator skill's troubleshooting reference cover
the common failures.

1. **Host prerequisites.** Linux, **rootless** Docker with Compose v2, one harness credential
   (Claude / Codex / OpenCode-compatible provider), outbound access to the allowlisted hosts.
   → Gate: `docker info 2>/dev/null | grep -i rootless` says true; `docker compose version` ≥ v2.

2. **Clone and build the images.** `git clone --recurse-submodules …/software-factory-rs`,
   then `docker/build.sh <runtime>` for every runtime your projects need (see the table in
   `docs/references/runtimes.md`; pick by what *verification* needs, not just the language).
   → Gate: `build.sh` exits 0 — it smoke-checks each image for the harness CLIs and ledger
   tools, and prints `RIG_IMAGE=…`. Details: `references/host-and-deployment.md`.

3. **Prepare each project repository.** `.factory/runtime.toml` (+ optional `allowlist`,
   `Dockerfile`, `mcp.json`, harness skills dirs), a feature branch for the factory to land
   on, a verify command that proves change works (tests, not builds), and a fine-grained git
   token scoped to exactly that repo. → Gate: the checklist at the end of
   `references/repo-preparation.md` — every box, per repo.

4. **Create the rigs.** One secrets file per rig (0600, under a 0700 dir), then
   `factory rig create <name> --repo-url … --runtime <rt> --harness <h> --main <feature-branch>
   --secrets <file> --no-start`. Create all rigs first; start them one at a time.
   → Gate: `factory rig list` shows every rig; `factory rig doctor` after each start shows
   `ledger=yes` and the expected services.

5. **First start + doctor.** Start one rig; run doctor inside it:
   `docker compose -p factory-<rig> --env-file ~/.factory/<rig>/compose.env -f compose.yaml run --rm shell doctor --probe`.
   → Gate: repo on the feature branch, runtime matches `runtime.toml`, **only** the intended
   credential present, probe returns one token per configured harness.

6. **Console + tokens.** Mint per-person tokens (`openssl rand -hex 32`, hash with
   `console hash-token`, one entry per person in `~/.factory/console/tokens.toml` with the
   scopes they need: `watch` / `plan` / `resolve` / `admin`), then `factory rig console`.
   → Gate: `curl -s localhost:7700/rigs/<rig>/.well-known/agent-card.json | jq .skills[].id`
   answers, and the browser UI at `http://127.0.0.1:7700/` shows every rig after pasting a token.

7. **Security pass.** Walk `references/security-checklist.md` — every box — before exposing
   the console beyond loopback (TLS via the `caddy` profile) or leaving the factory unattended.

8. **First plan.** Hand off to the `factory-operator` skill: write a real epic
   (`references/plan-writing.md` there), submit it, watch it land, review the branch.

## Rules

- **Never bake a credential into an image**; everything arrives as env at container start
  from a gitignored file. Never mount host credential files, SSH keys, or the Docker socket.
- **`main` stays untouched**: point `RIG_MAIN` at a feature branch; the Integrator refuses
  branches in `RIG_PROTECTED_BRANCHES` (`main,master` default). Protect `main` on the remote too.
- **One rig = one repo = one scoped token.** A fine-grained token has one resource owner;
  repos across two orgs need two tokens.
- **Verify commands are the contract.** A repo with no tests gets tests in its first epic —
  a build alone proves little.
- Run `factory rig` commands **from this repository** (it supplies `compose.yaml`), or set
  `FACTORY_COMPOSE`.

## References

- `references/host-and-deployment.md` — host setup, image building, single- and multi-rig
  deployment, console, TLS, systemd units, and the verification gates in full.
- `references/repo-preparation.md` — everything a project repo needs before its rig exists.
- `references/security-checklist.md` — the bootstrap-time security walkthrough.
