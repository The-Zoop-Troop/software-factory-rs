# `factory` CLI reference (generated)

- **Status:** generated · **Verified:** by `cargo xtask gen-docs --check` in CI. Do not edit by hand.

Source: `crates/factory/src/cli.rs` via `factory <cmd> --help`.

## `factory`

```text
Autonomous AI software factory

Usage: factory [OPTIONS] <COMMAND>

Commands:
  version    Print build/version information
  bead       Inspect beads through the factory's typed view
  doctor     Check that this host or rig can run the factory (tools, ledger, repo, credentials)
  watch      Summarize the ledger: tasks per epic by state, incidents, questions
  inbox      Items that need a human: open incidents and questions
  plan       Run the Planner: turn a plan (text or file) into an epic of task + verify beads
  work       Run a Worker: claim ready tasks and hand each to a fresh Claude Code session
  verify     Run the Verifier: check every task awaiting verification
  integrate  Run the Integrator: land verified branches on main
  help       Print this message or the help of the given subcommand(s)

Options:
      --workdir <WORKDIR>  Directory containing `.beads/` (defaults to the current directory) [default: .]
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
  -h, --help               Print help
```

## `factory watch`

```text
Summarize the ledger: tasks per epic by state, incidents, questions

Usage: factory watch [OPTIONS]

Options:
      --interval <INTERVAL>  Seconds between refreshes; omit to print once
      --workdir <WORKDIR>    Directory containing `.beads/` (defaults to the current directory) [default: .]
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

      --file <FILE>
          Read the plan from this file instead of --text

      --text <TEXT>
          The plan, inline

      --harness <HARNESS>
          LLM harness behind the Planner

          Possible values:
          - claude:   Claude Code headless (`claude -p`)
          - opencode: `OpenCode` headless server (`opencode serve`), any configured provider
          - codex:    Codex CLI headless (`codex exec --json`); needs `OPENAI_API_KEY` or a Codex login
          
          [default: claude]

      --model <MODEL>
          Model: Claude model name, or `provider/model` for opencode

      --max-budget-usd <MAX_BUDGET_USD>
          Spend cap for the planner run, USD (claude only)
          
          [default: 2]

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

      --worktrees <WORKTREES>
          Directory for task worktrees
          
          [default: .factory/worktrees]

      --events <EVENTS>
          Event log path (JSONL, appended)
          
          [default: .factory/events.jsonl]

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
      --worktrees <WORKTREES>  Directory for throwaway worktrees [default: .factory/worktrees]
      --events <EVENTS>        Event log path (JSONL, appended) [default: .factory/events.jsonl]
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
      --worktrees <WORKTREES>          Directory for throwaway worktrees [default: .factory/worktrees]
      --events <EVENTS>                Event log path (JSONL, appended) [default: .factory/events.jsonl]
      --main <MAIN>                    Integration branch [default: main]
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
  -h, --help               Print help
```

