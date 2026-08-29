# Security

- **Status:** accepted · **Verified:** rig acceptance checks 2026-08-28

**Boundary.** The rig container is the trust boundary; everything inside it — code, agents, tools — is untrusted. Agents run with full tool access *because* of this boundary, never without it.

**What the rig enforces.** Rootless daemon; non-root uid inside the userns; `cap_drop: ALL`; `no-new-privileges`; resource limits; internal network with no default route; egress only through a default-deny domain allowlist; no host mounts beyond two named volumes; no Docker socket, SSH keys, or host home.

**Credentials.** Injected as env at start from a gitignored file; scoped to one repo/provider; rotated by the operator. An agent can only exfiltrate them to hosts that already hold them. Never bake a credential into the image; never mount a host credential file.

**What it does not protect.** The project repository inside the rig (agents may damage it — that is why `main` on the remote must be branch-protected and only the Integrator pushes) and the provider account's spend (use per-session budgets and provider-side limits).

**Supply chain.** `cargo deny` (advisories + license allowlist) in CI; pinned, checksum-verified tool binaries in the image.

**Reporting.** Open a `question` bead or a GitHub issue marked security; do not post exploits in public issues.
