# Bootstrap security checklist

The threat model is `docs/SECURITY.md`; the sandbox design is
`docs/design-docs/rig-sandbox.md`. This is the operator's walkthrough before a factory runs
unattended or its console leaves loopback. **The rig container is the trust boundary;
everything inside it is untrusted** — agents get full tool access *because* of that boundary,
never without it.

## The sandbox (verify, don't assume)

- [ ] Rootless daemon: `docker info | grep -i rootless`
- [ ] Rigs run uid 10001, `cap_drop: ALL`, `no-new-privileges`, pids/mem/cpu limits — set in
      `compose.yaml`; runtimes never relax them (conformance asserts `CapEff` is zero).
- [ ] Rig network is `internal: true`; only `egress` bridges out, default-deny with
      `docker/egress/allowlist`. Review the assembled allowlist after every `build.sh`:
      `grep -cvE '^\s*(#|$)' docker/egress/allowlist` and read what is in it.
- [ ] No host mounts beyond the named volumes; no Docker socket, SSH keys, or host home
      inside any rig.

## Credentials

- [ ] Every credential arrives as env at start from a gitignored file
      (`docker/rig.env` / `~/.factory/secrets/<rig>.env`, 0600 in a 0700 dir). Never baked
      into an image, never a host credential file mounted.
- [ ] Git tokens are fine-grained, one repo each, Contents read/write only.
- [ ] `doctor` shows **only the intended credential** per rig — a rig with a Claude token
      AND an OpenAI key doubles the exfiltration surface for no reason.
- [ ] Provider spend: per-task budgets (tokens / wall-clock / attempts, default
      400k / 45 min / 3) plus `--max-budget-usd` caps; set provider-side limits too — the
      sandbox does not protect the provider account's spend.

## Branches

- [ ] `RIG_MAIN` is a feature branch; `RIG_PROTECTED_BRANCHES` still lists `main,master`.
- [ ] `main` is branch-protected **on the remote** (the factory can damage the repo inside
      the rig; the remote protection is what keeps damage from propagating).
- [ ] Only the Integrator pushes (`RIG_REMOTE`); workers cannot.

## Console (the only externally reachable process)

- [ ] Tokens: per **person**, random 32 bytes, stored only as sha256 hashes in
      `tokens.toml`, minimal scopes (`watch` < `plan` < `resolve` < `admin`) per rig.
- [ ] Rotation: replace the hash, restart the console; treat a token pasted into a chat or
      shell history as burned.
- [ ] TLS (`--profile tls`, Caddy) before any non-loopback exposure; `CONSOLE_PORT` stays
      bound to 127.0.0.1.
- [ ] The console holds no provider credential and cannot merge or override verification;
      every action and refusal is audited into the rig's event log.
- [ ] Alert webhook / OTLP collector receive task ids and titles only — never code or
      credentials — but treat their endpoints as part of the trust surface anyway.

## Supply chain

- [ ] `cargo deny` runs in CI (advisories + license allowlist); tool binaries in images are
      pinned and checksum-verified.
- [ ] Skill/MCP additions in project repos are code review surface: they run inside worker
      sessions.

## Known non-protections (say them out loud to the operator)

- The project repository **inside** the rig: agents may damage it. Remote branch protection
  + Integrator-only pushes are the real guard.
- Provider account spend beyond the configured caps.
- A malicious dependency inside the project's own toolchain (that is the project's CI
  problem too — the egress allowlist narrows, not eliminates, it).
