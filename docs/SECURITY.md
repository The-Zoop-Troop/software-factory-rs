# Security

- **Status:** accepted · **Verified:** rig acceptance checks 2026-08-28

**Boundary.** The rig container is the trust boundary; everything inside it — code, agents, tools — is untrusted. Agents run with full tool access *because* of this boundary, never without it.

**What the rig enforces.** Rootless daemon; non-root uid inside the userns; `cap_drop: ALL`; `no-new-privileges`; resource limits; internal network with no default route; egress only through a default-deny domain allowlist; no host mounts beyond two named volumes; no Docker socket, SSH keys, or host home.

**Credentials.** Injected as env at start from a gitignored file; scoped to one repo/provider; rotated by the operator. An agent can only exfiltrate them to hosts that already hold them. Never bake a credential into the image; never mount a host credential file.

**Protected branches.** The Integrator refuses to run when the integration branch (`RIG_MAIN`) is in `RIG_PROTECTED_BRANCHES` (`main,master` by default): landing on a feature branch is a configuration, landing on `main` is a deliberate override. Combine with branch protection on the remote so a bug cannot get there either.

**What it does not protect.** The project repository inside the rig (agents may damage it — that is why `main` on the remote must be branch-protected and only the Integrator pushes) and the provider account's spend (use per-session budgets and provider-side limits).

**Remote control.** The console is the only externally reachable process. It holds no provider credential (plans are queued as beads for the rig's own planner), authenticates every request with a hashed, per-client bearer token scoped per rig and verb, audits every action and refusal into the rig's event log, and cannot merge or override verification. Run it behind TLS (`docker compose --profile tls up caddy`); rotate tokens by replacing their hash; the alert webhook and OTLP collector receive task ids and titles, never code or credentials.

**Supply chain.** `cargo deny` (advisories + license allowlist) in CI; pinned, checksum-verified tool binaries in the image.

**Reporting.** Open a `question` bead or a GitHub issue marked security; do not post exploits in public issues.
