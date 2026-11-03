# Security

- **Status:** accepted · **Verified:** 2026-11-03 against `compose.yaml`, `docker/entrypoint.sh` and the code; rig acceptance checks last run 2026-08-28; untrusted children checked by `docker/holes-check/run.sh` on 2026-11-03

**Boundary.** The rig container is the trust boundary; everything inside it — code, agents, tools — is untrusted. Agents run with full tool access *because* of this boundary, never without it.

**What the rig enforces.** Rootless daemon; non-root uid inside the userns; `cap_drop: ALL`; `no-new-privileges`; resource limits; internal network with no default route; egress only through a default-deny domain allowlist; three named volumes and no host mounts for the roles (the rig-local console bind-mounts `docker/console`); no Docker socket, SSH keys, or host home.

**Credentials.** Injected as env at start from a gitignored file; scoped to one repo/provider; rotated by the operator. Every role receives the same file, but the children that run untrusted work (harness sessions, verify commands, Integrator checks) start without the factory's own secrets, other harnesses' credentials or secret-shaped names, and cannot read them from a role's `/proc`; only the Integrator keeps `RIG_GIT_TOKEN` (`docs/design-docs/rig-sandbox.md`). `RIG_AUTH` (`subscription` or `api-key`) chooses which kind the harnesses use and removes the other at start, so a shared env file cannot bill the wrong account; a rig holding both kinds for a harness and no choice refuses to start. An agent can only exfiltrate them to hosts that already hold them. Never bake a credential into the image; never mount a host credential file.

**Protected branches.** The Integrator refuses to run when the integration branch (`RIG_MAIN`) is in `RIG_PROTECTED_BRANCHES` (`main,master` by default): landing on a feature branch is a configuration, landing on `main` is a deliberate override. Combine with branch protection on the remote so a bug cannot get there either.

**What it does not protect.** The project repository inside the rig (agents may damage it — that is why `main` on the remote must be branch-protected) and the provider account's spend (use per-session budgets and provider-side limits).

**Known gaps inside the rig.** Untrusted children can write only their own worktree, branch, home, temp dir and cache (Landlock), but they can still read the ledger's files, the event log and other tasks' worktrees, and reach the ledger server's port without its password. They can move other task branches, which the Verifier and Integrator ignore because they check out recorded commits. A rig on a host without Landlock refuses to start unless `RIG_CONFINE=off`, which runs children unconfined. Test edits are detected from file paths only, so a test inside a source file is not seen. `docker/holes-check/run.sh` repeats these routes in a running rig.

**Remote control.** The console is the only externally reachable process. It holds no provider credential (plans are queued as beads for the rig's own planner), authenticates every request with a hashed, per-client bearer token scoped per rig and verb, audits every action and refusal into the rig's event log, and cannot merge or override verification. `CancelTask` can close tasks unverified, but that lands nothing. Run it behind TLS (`docker compose --profile tls up caddy`); rotate tokens by replacing their hash; the alert webhook and OTLP collector receive task ids and titles, never code or credentials.

**Supply chain.** `cargo deny` (advisories + license allowlist) in CI; pinned, checksum-verified tool binaries in the image.

**Reporting.** Open a `question` bead or a GitHub issue marked security; do not post exploits in public issues.
