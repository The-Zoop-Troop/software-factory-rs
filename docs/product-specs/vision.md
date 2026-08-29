# Vision

- **Status:** accepted · **Verified:** against the running system, 2026-08-29 · **Owner:** product

## One sentence

Give the factory a plan; get back a verified, merged codebase — with no orchestrator to babysit, no unverified "done", and no agent ever running loose on your machine.

## The problem

Coding agents are good at tasks and bad at projects. Left alone on a project they drift, stall, over-scope, and report success they haven't earned. The usual fixes — a central orchestrator agent, permission prompts, humans reviewing every diff — reintroduce the bottleneck the agents were meant to remove.

## The bet

Three inversions and one discipline make agent throughput safe to trust:

1. **No orchestrator.** Work is a dependency graph in a ledger. Workers pull what is ready and hold a lease while they hold it. When a worker dies, its lease expires and the work returns. There is no agent whose failure stops the factory.
2. **Done means verified.** Every task is planned with an executable check before any code exists. A task advances only when that check passes in a clean checkout, and lands only after the project's own checks pass on the rebased result. Models propose; verification disposes.
3. **YOLO only inside a rig.** Agents get full tool access — that is where their productivity comes from — but only inside a rootless container with default-deny egress and no host credentials. The container is the blast radius.

4. **The factory's control flow is a typed railway.** Every role turns facts into decisions and effects through one total state machine; every failure is a typed value that says what to do next. That is why an incident is something an agent or a person can act on, not a log to read. (See `docs/design-docs/railway.md`.)

Everything else follows: stateless workers with a curated context packet, budgets on every task, incidents instead of infinite retries, and a harness port that lets any coding agent (Claude Code, OpenCode against any OpenAI-compatible provider, Codex) do the work.

## Who it is for

Open-source builders and small teams who want agent throughput on real repositories without handing an agent their laptop, their credentials, or the last word on whether something works.

## What success looks like

- A newcomer can clone the repo, follow `docs/DEPLOYMENT.md`, and have a rig complete a multi-task plan on their project unattended.
- Zero-incident unattended runs are the norm, and every incident is a legible, actionable bead.
- Swapping the model or harness is a flag, not a fork.
- The repository is the whole system of record: an agent or a person can understand every decision from the repo alone.

## What it is not

- Not an IDE assistant or chat tool. It runs unattended and reports through the ledger.
- Not a hosted service. It runs on your hardware, against your providers.
- Not a replacement for judgment. Humans write plans, set budgets, answer questions, and resolve incidents.
