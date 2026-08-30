# `factory` CLI reference (generated)

- **Status:** generated · **Verified:** by `cargo xtask gen-docs --check` in CI. Do not edit by hand.

Source: `crates/factory/src/cli.rs` via `factory <cmd> --help`.

## `factory`

```text
Autonomous AI software factory

Usage: factory [OPTIONS] <COMMAND>

Commands:
  version    Print build/version information
  metrics    Throughput report: stage timings, critical path, concurrency (log file, or --rig)
  bead       Inspect beads through the factory's typed view
  doctor     Check that this host or rig can run the factory (tools, ledger, repo, credentials)
  watch      Summarize the ledger: tasks per epic by state, incidents, questions
  inbox      Items that need a human: open incidents and questions
  rig        Manage rigs on this host: one compose project per rig, one console over all of them
  stop       Cancel an epic through the console: its open tasks are closed (needs --rig)
  telegram   Run a Telegram bot over a remote rig (long polling; needs --rig and --token)
  plan       Run the Planner: turn a plan (text or file) into an epic of task + verify beads
  work       Run a Worker: claim ready tasks and hand each to a fresh Claude Code session
  verify     Run the Verifier: check every task awaiting verification
  integrate  Run the Integrator: land verified branches on main
  help       Print this message or the help of the given subcommand(s)

Options:
      --workdir <WORKDIR>  Directory containing `.beads/` (defaults to the current directory) [default: .]
      --rig <RIG>          Operate a remote rig through its console instead of a local ledger (`https://host/rigs/<name>`); applies to watch, inbox, plan, stop, doctor [env: FACTORY_RIG=]
      --token <TOKEN>      Bearer token for --rig [env: FACTORY_TOKEN]
  -h, --help               Print help
  -V, --version            Print version
```

## `factory doctor`

```text
Check that this host or rig can run the factory (tools, ledger, repo, credentials)

Usage: factory doctor [OPTIONS]

Options:
      --repo <REPO>        Path to the project clone [default: repo]
      --workdir <WORKDIR>  Directory containing `.beads/` (defaults to the current directory) [default: .]
      --probe              Also send a one-token request through every configured harness (costs a fraction of a cent)
      --rig <RIG>          Operate a remote rig through its console instead of a local ledger (`https://host/rigs/<name>`); applies to watch, inbox, plan, stop, doctor [env: FACTORY_RIG=]
      --token <TOKEN>      Bearer token for --rig [env: FACTORY_TOKEN]
  -h, --help               Print help
```

## `factory watch`

```text
Summarize the ledger: tasks per epic by state, incidents, questions

Usage: factory watch [OPTIONS]

Options:
      --interval <INTERVAL>  Seconds between refreshes; omit to print once
      --workdir <WORKDIR>    Directory containing `.beads/` (defaults to the current directory) [default: .]
      --rig <RIG>            Operate a remote rig through its console instead of a local ledger (`https://host/rigs/<name>`); applies to watch, inbox, plan, stop, doctor [env: FACTORY_RIG=]
      --token <TOKEN>        Bearer token for --rig [env: FACTORY_TOKEN]
  -h, --help                 Print help
```

## `factory inbox`

```text
Items that need a human: open incidents and questions

Usage: factory inbox [OPTIONS]

Options:
      --resolve <RESOLVE>  Resolve this bead (closes it; reopens its task if it was an incident)
      --workdir <WORKDIR>  Directory containing `.beads/` (defaults to the current directory) [default: .]
      --note <NOTE>        Note recorded with the resolution [default: "resolved by operator"]
      --rig <RIG>          Operate a remote rig through its console instead of a local ledger (`https://host/rigs/<name>`); applies to watch, inbox, plan, stop, doctor [env: FACTORY_RIG=]
      --token <TOKEN>      Bearer token for --rig [env: FACTORY_TOKEN]
  -h, --help               Print help
```

## `factory plan`

```text
Run the Planner: turn a plan (text or file) into an epic of task + verify beads

Usage: factory plan [OPTIONS]

Options:
      --repo <REPO>
          Path to the project clone (the Planner reads it for context in later phases)
          
          [default: repo]

      --workdir <WORKDIR>
          Directory containing `.beads/` (defaults to the current directory)
          
          [default: .]

      --main <MAIN>
          Integration branch; tasks are cut from its current tip
          
          [default: main]

      --rig <RIG>
          Operate a remote rig through its console instead of a local ledger (`https://host/rigs/<name>`); applies to watch, inbox, plan, stop, doctor
          
          [env: FACTORY_RIG=]

      --file <FILE>
          Read the plan from this file instead of --text

      --token <TOKEN>
          Bearer token for --rig
          
          [env: FACTORY_TOKEN]

      --text <TEXT>
          The plan, inline

      --after <AFTER>
          `rig:epic` this plan waits for (with --rig only)

      --harness <HARNESS>
          LLM harness behind the Planner

          Possible values:
          - claude:   Claude Code headless (`claude -p`)
          - opencode: `OpenCode` headless server (`opencode serve`), any configured provider
          - codex:    Codex CLI headless (`codex exec --json`); needs `OPENAI_API_KEY` or a Codex login
          
          [default: claude]

      --model <MODEL>
          Model: Claude model name, or `provider/model` for opencode

      --effort <EFFORT>
          Thinking effort: low | medium | high | max (harness default when omitted)
          
          [env: RIG_EFFORT=]

      --task-tokens <TASK_TOKENS>
          Token budget per task the Planner writes onto new tasks (default 400000)
          
          [env: RIG_TASK_TOKENS=]

      --max-budget-usd <MAX_BUDGET_USD>
          Spend cap for the planner run, USD (claude only)
          
          [default: 2]

      --queue
          Serve the plan queue instead: plan each open `plan_request` bead (from the console)

      --interval <INTERVAL>
          With --queue: keep polling every N seconds (one sweep when omitted)

      --events <EVENTS>
          With --queue: event log path (JSONL, appended) for planner progress
          
          [default: .factory/events.jsonl]

  -h, --help
          Print help (see a summary with '-h')
```

## `factory work`

```text
Run a Worker: claim ready tasks and hand each to a fresh Claude Code session

Usage: factory work [OPTIONS]

Options:
      --repo <REPO>
          Path to the project clone
          
          [default: repo]

      --workdir <WORKDIR>
          Directory containing `.beads/` (defaults to the current directory)
          
          [default: .]

      --rig <RIG>
          Operate a remote rig through its console instead of a local ledger (`https://host/rigs/<name>`); applies to watch, inbox, plan, stop, doctor
          
          [env: FACTORY_RIG=]

      --worktrees <WORKTREES>
          Directory for task worktrees
          
          [default: .factory/worktrees]

      --events <EVENTS>
          Event log path (JSONL, appended)
          
          [default: .factory/events.jsonl]

      --token <TOKEN>
          Bearer token for --rig
          
          [env: FACTORY_TOKEN]

      --main <MAIN>
          Integration branch tasks are cut from
          
          [default: main]

      --agent <AGENT>
          This worker's identity (lease holder)
          
          [default: worker-1]

      --lease-ttl <LEASE_TTL>
          Lease TTL, seconds; heartbeats renew at a third of this
          
          [default: 300]

      --max-turns <MAX_TURNS>
          Harness turn cap per task session
          
          [default: 200]

      --max-budget-usd <MAX_BUDGET_USD>
          Spend cap per task session, USD (claude only)
          
          [default: 5]

      --harness <HARNESS>
          LLM harness behind the Worker

          Possible values:
          - claude:   Claude Code headless (`claude -p`)
          - opencode: `OpenCode` headless server (`opencode serve`), any configured provider
          - codex:    Codex CLI headless (`codex exec --json`); needs `OPENAI_API_KEY` or a Codex login
          
          [default: claude]

      --model <MODEL>
          Model: Claude model name, or `provider/model` for opencode

      --effort <EFFORT>
          Thinking effort: low | medium | high | max (harness default when omitted)
          
          [env: RIG_EFFORT=]

      --interval <INTERVAL>
          Seconds to wait when nothing is ready; omit to run one task (or none) and exit

  -h, --help
          Print help (see a summary with '-h')
```

## `factory verify`

```text
Run the Verifier: check every task awaiting verification

Usage: factory verify [OPTIONS]

Options:
      --repo <REPO>            Path to the project clone [default: repo]
      --workdir <WORKDIR>      Directory containing `.beads/` (defaults to the current directory) [default: .]
      --rig <RIG>              Operate a remote rig through its console instead of a local ledger (`https://host/rigs/<name>`); applies to watch, inbox, plan, stop, doctor [env: FACTORY_RIG=]
      --worktrees <WORKTREES>  Directory for throwaway worktrees [default: .factory/worktrees]
      --events <EVENTS>        Event log path (JSONL, appended) [default: .factory/events.jsonl]
      --token <TOKEN>          Bearer token for --rig [env: FACTORY_TOKEN]
      --interval <INTERVAL>    Seconds between passes; omit to run once and exit
  -h, --help                   Print help
```

## `factory integrate`

```text
Run the Integrator: land verified branches on main

Usage: factory integrate [OPTIONS]

Options:
      --repo <REPO>                    Path to the project clone [default: repo]
      --workdir <WORKDIR>              Directory containing `.beads/` (defaults to the current directory) [default: .]
      --rig <RIG>                      Operate a remote rig through its console instead of a local ledger (`https://host/rigs/<name>`); applies to watch, inbox, plan, stop, doctor [env: FACTORY_RIG=]
      --worktrees <WORKTREES>          Directory for throwaway worktrees [default: .factory/worktrees]
      --events <EVENTS>                Event log path (JSONL, appended) [default: .factory/events.jsonl]
      --token <TOKEN>                  Bearer token for --rig [env: FACTORY_TOKEN]
      --main <MAIN>                    Integration branch [default: main]
      --protected <PROTECTED>          Branches the factory must never integrate into or push (comma-separated) [env: RIG_PROTECTED_BRANCHES=] [default: main,master]
      --remote <REMOTE>                Remote to push main to after landing (omit for local-only)
      --check <CHECKS>                 Project-wide check to run on the rebased head before landing (repeatable)
      --check-timeout <CHECK_TIMEOUT>  Timeout per check, seconds [default: 1200]
      --interval <INTERVAL>            Seconds between passes; omit to run once and exit
  -h, --help                           Print help
```

## `factory bead`

```text
Inspect beads through the factory's typed view

Usage: factory bead [OPTIONS] <COMMAND>

Commands:
  show  Show a bead with its factory kind, state, budget and lease decoded
  help  Print this message or the help of the given subcommand(s)

Options:
      --workdir <WORKDIR>  Directory containing `.beads/` (defaults to the current directory) [default: .]
      --rig <RIG>          Operate a remote rig through its console instead of a local ledger (`https://host/rigs/<name>`); applies to watch, inbox, plan, stop, doctor [env: FACTORY_RIG=]
      --token <TOKEN>      Bearer token for --rig [env: FACTORY_TOKEN]
  -h, --help               Print help
```

## `factory stop`

```text
Cancel an epic through the console: its open tasks are closed (needs --rig)

Usage: factory stop [OPTIONS] <EPIC>

Arguments:
  <EPIC>  The epic id

Options:
      --workdir <WORKDIR>  Directory containing `.beads/` (defaults to the current directory) [default: .]
      --rig <RIG>          Operate a remote rig through its console instead of a local ledger (`https://host/rigs/<name>`); applies to watch, inbox, plan, stop, doctor [env: FACTORY_RIG=]
      --token <TOKEN>      Bearer token for --rig [env: FACTORY_TOKEN]
  -h, --help               Print help
```

## `factory telegram`

```text
Run a Telegram bot over a remote rig (long polling; needs --rig and --token)

Usage: factory telegram [OPTIONS] --bot-token <BOT_TOKEN> --chat <CHATS>

Options:
      --bot-token <BOT_TOKEN>  Bot token from `@BotFather` [env: TELEGRAM_BOT_TOKEN]
      --workdir <WORKDIR>      Directory containing `.beads/` (defaults to the current directory) [default: .]
      --chat <CHATS>           Chat ids allowed to talk to the bot (repeatable); others are ignored
      --rig <RIG>              Operate a remote rig through its console instead of a local ledger (`https://host/rigs/<name>`); applies to watch, inbox, plan, stop, doctor [env: FACTORY_RIG=]
      --poll <POLL>            Seconds between task polls for push notifications [default: 30]
      --token <TOKEN>          Bearer token for --rig [env: FACTORY_TOKEN]
  -h, --help                   Print help
```

## `factory rig`

```text
Manage rigs on this host: one compose project per rig, one console over all of them

Usage: factory rig [OPTIONS] <COMMAND>

Commands:
  create   Register a rig, write its env + secrets, and start it
  list     Rigs on this host
  destroy  Stop a rig and forget it; `--volumes` also deletes its ledger and repo
  stop     Stop a rig's roles and egress; the ledger stays up so its history stays readable
  start    Start a rig again
  doctor   Ledger volume and running services per rig
  backup   Archive a rig's ledger and repo volumes into a directory
  restore  Replace a stopped rig's ledger (and optionally repo) volume from `rig backup` tarballs
  console  Bring the shared console up (or down) over every registered rig
  help     Print this message or the help of the given subcommand(s)

Options:
      --root <ROOT>        Where rig files live (registry, per-rig env and secrets, console config); `~` expands [env: FACTORY_ROOT=] [default: ~/.factory]
      --workdir <WORKDIR>  Directory containing `.beads/` (defaults to the current directory) [default: .]
      --compose <COMPOSE>  The shared rig compose file [env: FACTORY_COMPOSE=] [default: compose.yaml]
      --rig <RIG>          Operate a remote rig through its console instead of a local ledger (`https://host/rigs/<name>`); applies to watch, inbox, plan, stop, doctor [env: FACTORY_RIG=]
      --token <TOKEN>      Bearer token for --rig [env: FACTORY_TOKEN]
  -h, --help               Print help
```

