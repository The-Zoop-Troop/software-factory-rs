# Autonomous AI Software Factory — Architecture v0.1

> Input: a high-level plan for a new or existing software project.
> Output: a completed, verified, merged codebase.
> Core substrate: **Beads** (state), **A2A** (agent interface), **rootless Docker** (blast radius).

Three inversions define the design, each stated positively:

| Inversion | What it means here |
|---|---|
| **No orchestrator** | Work is pulled, never assigned. The beads dependency graph *is* the scheduler; idle workers claim `ready` beads with a lease. Nothing is a single point of failure or bottleneck. |
| **Done means verified** | A task cannot close on a model's word. Every task carries an executable verify check; only the Verifier advances it, and only the Integrator lands it. |
| **YOLO only inside a rig** | Agents run with full tool access, but only inside a rootless container with default-deny egress. The container is the blast radius; the worktree is the unit of concurrency. |

Supporting choices: the ledger (Beads) is the only coordination surface; worktrees are ephemeral per task, never per agent; supervision is leases and budgets on the bead, not a watchdog hierarchy; agents start every task from zero with a curated context packet.

---

## 1. Design principles

1. **The ledger is the truth.** If it isn't in beads, it didn't happen. Agent context windows are cache, not state. Any agent can die at any time and the factory loses nothing but the in-flight turn.
2. **Pull, don't push.** No orchestrator assigns work. Work becomes `ready` when its dependencies close; idle workers claim it with a lease. This removes the single point of failure and the single point of bottleneck that a central orchestrator creates.
3. **Every agent is an A2A server.** Planner, Worker, Verifier, Integrator, Steward are all opaque A2A endpoints with Agent Cards. The harness behind the card (Claude Code, Codex, a Rust program, a human) is swappable without touching the factory.
4. **Done means verified.** A bead cannot close on the worker's word. Every implementation bead has a paired verify bead with an executable check. Trust is placed in verification, not in the model.
5. **The container is the trust boundary; everything inside is untrusted.** No host mounts except the project volume, no Docker socket, no SSH keys, default-deny network, credentials injected at start and scoped to the session.
6. **Budgets are first-class.** Every bead carries a token/time/retry budget. Exceeding it is a state transition, not an infinite loop.

---

## 2. Topology

```
HOST (your machine or a VM)
│
│  rootless dockerd            ← the only privileged-ish thing, runs as your uid
│
└─ factory network (internal, no default route)
   │
   ├─ [egress-proxy]           allowlist: api.anthropic.com, git remote, package registries
   │
   ├─ [factory]                one container per project ("rig")
   │    ├─ /work/repo          bare clone + worktrees  (named volume)
   │    ├─ /work/.beads        Dolt-backed beads DB    (named volume)
   │    ├─ stewardd            A2A server: lease sweeper, budget enforcer, human inbox
   │    ├─ planner             A2A server (LLM harness)
   │    ├─ worker × N          A2A servers (LLM harness), stateless, claim from `bd ready`
   │    ├─ verifier × M        A2A servers, run the verify command, no LLM required for most
   │    └─ integrator          A2A server, merge queue
   │
   └─ [console]                thin UI/CLI on host: talks A2A to stewardd, renders beads, answers INPUT_REQUIRED
```

**Why one container per rig, not one per agent.** Agents sharing a filesystem and a Dolt instance is what makes worktrees and beads cheap. Per-agent containers are a later hardening tier (§9), not the baseline. The container is the blast radius; the worktree is the concurrency unit.

**What crosses the boundary.** Only: (a) A2A/JSON-RPC over the internal network, (b) git push to the remote via the egress proxy, (c) LLM API calls via the egress proxy. Nothing else.

---

## 3. The ledger: how Beads is used

Beads is used as-is (the `bd` CLI + Dolt). We add conventions, not forks.

### 3.1 Bead types (by label/type)

| Type | Created by | Closed by | Purpose |
|---|---|---|---|
| `epic` | Planner | Steward, when all children closed | One per high-level plan item |
| `task` | Planner / Worker (decomposition) | Verifier only | A unit of implementation, sized for one worker session |
| `verify` | Planner (paired with each task) | Verifier | Holds an executable acceptance check |
| `merge` | Verifier (on verify pass) | Integrator | Branch is ready for the merge queue |
| `question` | Any agent | Human (via console) | `INPUT_REQUIRED` surfaced to a person |
| `incident` | Steward | Human or Steward | Budget exceeded, repeated failure, lease storm |

### 3.2 Required fields on a `task` bead

```yaml
acceptance:        # what "done" means — written by Planner before any code exists
  - "cargo test -p auth passes"
  - "POST /login returns 401 on bad password (see verify bead)"
verify_bead: af-1234           # paired verify bead id
budget:
  tokens: 400000
  wall_clock: 45m
  attempts: 3
lease:                         # set on claim, cleared on release
  holder: worker-07
  expires: 2026-08-28T15:10:00Z
branch: task/af-1233           # worktree branch, created on claim
base: main@<sha>               # what the branch was cut from
```

### 3.3 The state machine (per task)

```
open ──claim──▶ leased ──push──▶ in_verify ──pass──▶ mergeable ──merged──▶ closed
                 │                  │
                 │ lease expired    │ fail (attempts < budget)
                 ▼                  ▼
               open  ◀──────── open (+failure note appended)
                                    │ fail (attempts exhausted)
                                    ▼
                                incident
```

Notes are append-only. Every failure leaves the verifier's output on the bead so the next worker sees what already went wrong — this is the memory that survives agent death.

### 3.4 Dependencies do the scheduling

`bd ready` returns beads whose blockers are closed. That query *is* the scheduler. The Planner encodes ordering as `blocks` edges; the factory has no other notion of priority beyond the beads priority field.

---

## 4. The agents (all A2A servers)

Each is a process inside the rig container exposing an A2A endpoint and an Agent Card. Roles are identified by **skill tags** on the card, so the factory discovers "who can verify Rust" rather than hardcoding names.

### 4.1 Planner
- Input: the high-level plan (text, or an `epic` bead someone wrote by hand).
- Output: an epic DAG in beads — tasks with acceptance criteria, verify beads with concrete commands, `blocks` edges.
- For an **existing** project, the Planner first runs a "survey" turn: reads the repo, records architecture notes as a `reference` bead so workers don't re-derive them.
- The Planner is re-invoked by the Steward when an epic stalls (all remaining tasks blocked or in incident) to **re-plan**, not to babysit.

### 4.2 Worker (× N, stateless)
Loop:
1. `bd ready --type task` → pick one → `bd update --claim` (atomic lease).
2. `git worktree add /work/wt/<bead> -b task/<bead> <base>`.
3. Spawn the LLM harness in that worktree with a system prompt built from: bead description + acceptance + failure notes + the epic's reference beads. **The harness has no memory of previous beads.** Fresh context every task.
4. On harness exit: commit, push branch to the *local* bare repo, `bd update --state in_verify`, remove worktree, release lease.
5. Heartbeat: extend the lease every N minutes while the harness is running. If the worker process dies, the lease expires and the bead returns to `open`.

The worker never decides it is done. It only decides it has nothing more to do.

### 4.3 Verifier (× M)
- Claims `verify` beads whose paired task is `in_verify`.
- Checks out the branch in a fresh worktree, runs the verify command(s) from the bead **exactly**, captures output.
- Pass → creates a `merge` bead. Fail → appends output to the task bead, decrements attempts, reopens the task.
- Most verification needs no LLM (it's `cargo test`, `npm test`, a curl script). An LLM-backed "review" verifier is a second verifier with a different skill tag, used only when the Planner asks for it.
- **Flaky-test deflection:** a verify bead that fails, then passes on an identical commit, is tagged `flaky` and the Steward opens an incident against the *test*, not the task.

### 4.4 Integrator (merge queue)
- Drains `merge` beads in batches.
- Batch-then-bisect: rebase batch onto `main`, run the project's full verify suite once. Pass → fast-forward `main`, close all. Fail → bisect the batch, isolate the offender, reopen it with the conflict/failure note, merge the rest.
- Pushes `main` to the remote through the egress proxy. This is the **only** agent allowed to push to the remote.

### 4.5 Steward (the only daemon, no LLM)
A small program, not an agent-with-a-prompt. Responsibilities:
- Sweep expired leases → reopen beads.
- Enforce budgets → move beads to `incident`.
- Close epics whose children are all closed; re-invoke the Planner when an epic stalls.
- Serve the **human inbox**: all `question` and `incident` beads, exposed over A2A as `INPUT_REQUIRED` tasks so the console can render and answer them.
- Emit a structured event log (JSONL) of every state transition — this is the observability pipeline. Nothing fancier is needed in v0.

This is ~500 lines of deterministic code in place of a supervisor hierarchy. The insight: agent supervision doesn't need an agent, it needs a lease and a clock.

---

## 5. A2A mapping

| Factory concept | A2A concept |
|---|---|
| Epic | `contextId` |
| Bead | `Task.id` (the bead id is the A2A task id) |
| Bead state | `TaskState` (`open`→`SUBMITTED`, `leased`→`WORKING`, `question`→`INPUT_REQUIRED`, `closed`→`COMPLETED`, `incident`→`FAILED`) |
| Bead notes | `Task.history` |
| Branch / verify output / diff | `Artifact` |
| Role (worker, verifier…) | `AgentSkill.tags` on the Agent Card |
| Console watching progress | `SubscribeToTask` / SSE |
| Human answer to a question | `SendMessage` with `taskId` on the `INPUT_REQUIRED` task |

Why bother with A2A inside a single container? Because it makes the harness pluggable (Claude Code today, anything tomorrow), it gives the console a standard API for free, and it means a rig can later be split across machines without changing a single agent.

---

## 6. Security posture (rootless Docker, limited scope)

Baseline, non-negotiable:
- Rootless dockerd; container runs as a non-root uid *inside* the userns too.
- `--network factory-internal` (`--internal` bridge, no default route). Only the egress proxy has a second NIC to the outside.
- Egress proxy allowlist: LLM API host, git remote host, language package registries. Everything else is refused and logged.
- Mounts: `repo` and `beads` named volumes only. **No** `~/.ssh`, **no** Docker socket, **no** host home.
- Credentials: LLM API key and a **deploy-key scoped to one repo** injected as env at `docker run`, rotated per session. Agents can exfiltrate them only to the allowlisted hosts, which already have them.
- `--cap-drop ALL`, `--security-opt no-new-privileges`, `--pids-limit`, memory + CPU limits (protects the host from a runaway `cargo build`, which is the realistic failure mode).
- Optional: `--runtime runsc` (gVisor) as a drop-in upgrade — the architecture doesn't change.

What this does **not** protect: the project repo itself. That is by design (YOLO inside the box). The remote branch protection on `main` plus the Integrator being the only pusher is the recovery path.

---

## 7. Human interface

The human is an A2A client, nothing more. The console:
- Submits the plan (`SendMessage` to the Planner).
- Streams the epic (`SubscribeToTask` on the `contextId`).
- Answers `INPUT_REQUIRED` questions and resolves incidents.
- Can inject a hand-written bead at any time (e.g. "stop, change the auth approach") — that's just `bd create` with a `blocks` edge on the affected tasks.

Start as a CLI (`factory plan`, `factory watch`, `factory inbox`). A web console over A2UI is a natural later step given the A2A base.

---

## 8. Context engineering (why workers are stateless)

Long-lived agent sessions accumulate context and drift, and then need supervisors to notice. We invert it: **every task starts from zero** with a curated packet:

1. The bead (description, acceptance, prior failure notes).
2. The epic's `reference` beads (architecture survey, decisions).
3. A structural code map for the files the Planner named — tree-sitter symbol index, not `grep` output. This is an MCP tool the harness calls, provided by the rig.

That's the whole packet. Small, deterministic, reproducible. If a task needs more than fits, the Planner sized it wrong, and the Worker's correct move is to split the bead (`bd create` two children, `blocks` the parent) and release it — decomposition is a *result*, not a failure.

---

## 9. Roadmap

**Phase 0 — walking skeleton (1 rig, 1 of each agent, no LLM in verifier/integrator)**
- Container image + compose file with the network/egress layout from §6.
- `bd` conventions from §3 as a `factory` wrapper CLI.
- Steward: lease sweep, budget, event log.
- Worker: claim → worktree → Claude Code headless → push → in_verify.
- Verifier: run command, pass/fail.
- Integrator: single-PR fast-forward (no batching yet).
- Planner: one prompt that emits beads.
- Exit criterion: give it a 5-task plan on a toy repo, walk away, come back to a merged `main`.

**Phase 1 — parallelism and robustness**
- N workers, lease heartbeats, worker death → reopen.
- Batch-then-bisect Integrator.
- Flaky detection.
- Planner re-plan on stall.
- Console `watch` and `inbox`.

**Phase 2 — context quality**
- Tree-sitter index as an MCP tool inside the rig.
- Survey turn for existing repos.
- Prompt registry: system prompts are versioned beads, so A/B'ing a Worker prompt is a diff in the ledger.

**Phase 3 — hardening (opt-in tiers)**
- gVisor runtime.
- Per-worker sub-containers (worker gets its own container, shares the volumes).
- Snippet-level SCA/license check as a Verifier skill.
- Multi-rig: several project containers, one console.

---

## 10. Open decisions

| Decision | Recommendation | Why |
|---|---|---|
| Language for Steward / factory CLI / A2A servers | **Rust** | Deterministic, small static binaries in the image, no runtime to secure; you already have a Rust FP skill set up. Go is the alternative for `bd` affinity. |
| Harness behind the Worker card | Claude Code headless (`claude -p`) first; OpenCode (`opencode serve` HTTP API) and Codex (`codex exec --json`) implemented behind the same `Harness` port | Proven: OpenCode against a custom OpenAI-compatible provider completed an epic inside the rig. Codex needs a Responses-API endpoint (OpenAI). |
| Beads backing store | Dolt (beads default) | Get branch/merge semantics on the ledger for free; revisit only if it's a pain in the container. |
| Verifier LLM review | Off by default | Executable checks are the trust anchor; LLM review is advisory. |
| One container per rig vs per agent | Per rig | See §2. Per-agent is Phase 3. |

---

## 11. Naming

Roles are named by function so a reader knows what a thing does: Planner, Worker, Verifier, Integrator, Steward. A project instance is a **rig**. The whole thing is **the factory**.
