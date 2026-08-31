# Operations: lifecycle, upgrades, backups, rotation

Define once per shell: `R="docker compose -p factory-<rig> --env-file ~/.factory/<rig>/compose.env -f compose.yaml"`
(run from the factory repository). Console project:
`C="docker compose -p factory-console --env-file ~/.factory/console/compose.env -f ~/.factory/console/compose.yaml"`.

## Start / stop / status

```sh
factory rig list                      # registry: name, repo, runtime, harness, console port
factory rig doctor                    # ledger volume + running services per rig
factory rig stop <rig>                # roles + egress down, LEDGER STAYS UP (history readable)
factory rig start <rig>               # roles back up
factory rig console [down]            # the shared console over every rig
$R ps                                 # one rig's services
docker ps --format '{{.Names}}\t{{.Image}}\t{{.Status}}'
```

A stopped rig showing `running=[ledger]` is **healthy by design** — the console still reads
its history. Do not "fix" it by starting the roles.

## Upgrade / rebuild / restart

The binaries **and the web console UI are baked into the images at build time** — restarting
a container without rebuilding its image changes nothing, and rebuilding without recreating
containers changes nothing visible. Always do both, in this order:

```sh
git pull
docker/build.sh <runtime>             # once per runtime in use; base is rebuilt each time
# end with the runtime whose egress allowlist should win (build.sh assembles it per run)
```

Then recreate **exactly what was running**:

```sh
$C up -d --remove-orphans                       # console
$R up -d ledger                                 # per stopped rig: ledger only
$R up -d                                        # per RUNNING rig: all its roles
```

Verify, never assume:

```sh
factory rig doctor                              # every rig ok, expected services
docker ps --format '{{.Names}}\t{{.Status}}'    # (healthy) on ledgers
curl -s localhost:7700/ | grep -o 'assets/index-[^"]*'   # console asset hash changed ⇒ new UI
docker logs factory-console-console-1 --tail 5  # "console listening … rigs=N"
```

Rules learned from real runs:

- **Recreate only what was running.** `up -d` on a stopped rig's project starts its workers —
  an upgrade must not resurrect a deliberately stopped rig. Use `up -d ledger` there.
- `docker compose restart` does NOT pick up a new image; only `up -d` (recreate) does.
- Systemd hosts: `systemctl --user restart factory-rig@<name> factory-console` does the
  compose calls for you — but only if the units are installed and enabled.
- Old image layers accumulate: `docker image prune` after a few upgrade rounds (never
  `-a` on a shared host without checking `docker ps` first).

## Scheduled maintenance and reports

- **Weekly sweep** — `docker/systemd/factory-maintenance.timer` runs `docker/maintenance.sh`:
  backs up every registered rig to `~/.factory/backups/`, prunes archives older than 30 days,
  and warns when a rig's `RIG_GIT_TOKEN` expires within 14 days (a mid-run 401 costs
  attempts; rotate before, not after). Run it by hand any time; nonzero exit = warnings, and
  `journalctl --user -u factory-maintenance` has the log.
- **Reboot resilience** — install the user units (`docker/systemd/`): `factory-console`,
  `factory-rig@<name>` for rigs that should run, `factory-ledger@<name>` for rigs that are
  deliberately stopped (ledger-only, history stays readable), `factory-maintenance.timer`;
  then `loginctl enable-linger $USER`. Fix `FACTORY_COMPOSE`/paths when copying if the repo
  is not at `~/git-repos/software-factory-rs`.
- **Cost report** — `docker/cost-report.sh` (optionally `USD_PER_MTOK=<rate>`): per-rig
  epics/tasks/attempts/landed, total tokens, and wasted tokens (attempts that did not land),
  read from each rig's own event log — works for stopped rigs.
- **Image hygiene** — monthly: `docker/build.sh --pull <runtime>` per runtime in use
  (refreshes the FROM images for CVE fixes), then the recreate procedure above; finish with
  `docker image prune`.

## Alerts

One line turns on push notifications for "needs a human" and "epic finished":
`CONSOLE_ALERT_URL=https://<webhook>` in `~/.factory/console/compose.env`, then
`factory rig console`. The console posts `{"rig","text"}` — a Slack incoming webhook, ntfy
topic, or PagerDuty relay all work; `CONSOLE_ALERT_INTERVAL` (seconds, default 30) tunes the
sweep. The Telegram bot (`--profile telegram`) is the chat-native alternative.

## Backup / restore

```sh
factory rig backup <rig> --to backups/          # <rig>-ledger-<ts>.tgz + <rig>-repo-<ts>.tgz
$R down                                          # restore refuses while anything runs
factory rig restore <rig> --ledger backups/<rig>-ledger-<ts>.tgz [--repo backups/<rig>-repo-<ts>.tgz]
$R up -d && factory rig doctor
```

- The two volumes ARE the rig's state; the images are stateless.
- Run the **restore drill** quarterly per rig (`docs/RELIABILITY.md` has the 5-step script);
  record date and outcome in `docs/QUALITY_SCORE.md`. A backup that has never been restored
  is a hope, not a backup.
- Always `factory rig backup` before `factory rig destroy --volumes` — history lives exactly
  as long as the ledger volume.

## Credential and token rotation

- **Harness/git credentials**: edit `~/.factory/<rig>/rig.env` (or the secrets file and
  re-copy), then recreate the affected services (`$R up -d`). A 401/revoked error mid-run
  needs the same: fix the file, recreate, then Resume-from-branch on any environment
  incidents it caused.
- **Console tokens**: replace the sha256 entry in `~/.factory/console/tokens.toml`, then
  `factory rig console` (regenerates + restarts). A token that touched a chat log or shell
  history is burned.
- **Ledger password**: `~/.factory/ledger.password` is shared by every rig on the host;
  changing it means updating each rig's `compose.env` and recreating ledgers + console.

## Teardown

```sh
factory rig stop <rig>                 # keep history
factory rig backup <rig> --to backups/ && factory rig destroy <rig> --volumes   # forget it
```

Then prune: `docker volume ls | grep factory-<rig>` should be empty; `docker image prune`
for orphaned layers.
