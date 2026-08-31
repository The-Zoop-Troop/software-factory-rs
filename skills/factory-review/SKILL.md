---
name: factory-review
description: Review what a software-factory epic actually landed — walk the feature branch commit by commit, verify beads first, confirm tests were added (not just passed), read the docs against the code, snapshot the metrics, and turn wanted changes into a review-notes plan slice. Use when asked to review an epic, review what landed, audit a feature branch the factory produced, or close out a phase.
---

# Factory review — read what landed like a colleague's PR

An epic closing means every task was **verified and landed**; it does not mean anyone read
the code. This skill is the closing step of every phase: the factory made the diff small
enough to review — so review it. Wanted changes go back **through the factory**, not as hand
edits.

Handles: `R="docker compose -p factory-<rig> --env-file ~/.factory/<rig>/compose.env -f compose.yaml"`;
remote access via `FACTORY_RIG`/`FACTORY_TOKEN`. Context: `docs/guides/first-project.md` §7.

## Workflow

1. **Fetch and frame the diff.** One squash commit per task, titled by task; `main` untouched.

   ```sh
   git fetch origin
   git log --oneline origin/main..origin/<feature-branch>
   git diff --stat origin/main...origin/<feature-branch>
   git log -1 origin/main        # still the pre-run commit — if not, stop and investigate
   ```

2. **Verify beads first — the commands are the contract.** For each task, read what actually
   ran (console task drawer, or `$R exec steward sh -c 'cd /work/rig && factory bead show <task-id>'`).
   A verify command weaker than the plan intended is a **plan fix for the next phase**, not a
   code edit now. Note any task that landed on a retry — read its failed attempt's verify
   output to learn why.

3. **Tests were ADDED, not just passed.** Every task's acceptance said tests; confirm the
   diff contains them:

   ```sh
   git diff origin/main...origin/<feature-branch> --stat -- '*test*' '*spec*'
   ```

   A task whose diff has no test changes passed on pre-existing coverage — flag it.

4. **Docs against code.** The planner puts a docs/overlay task in each epic; it is the
   likeliest to drift when two tasks conflicted mid-run. Read the changed docs against the
   changed code, not against the plan.

5. **Snapshot the metrics** while reviewing is cheap:

   ```sh
   factory --rig … metrics --epic <id> --json > runs/<epic>-metrics.json
   ```

   First-pass rate and retry tax are the plan-quality feedback for the next epic
   (`factory-operator` skill, `references/scaling-and-metrics.md`).

6. **Route the findings.**
   - Wanted changes → a new plan slice: `Review notes for <epic>: <numbered findings>` —
     submitted like any plan (`factory-operator`, `references/plan-writing.md`). Hand commits
     are tolerated (the Integrator fetches and fast-forwards) but the next session rebases
     onto them blind — prefer the plan slice.
   - Factory defects (bad decomposition, weak verify, infra incidents) → beads on the
     factory repo, not the project.
   - Clean review → say so explicitly, then gate the next phase
     (`factory rig stop <rig>` keeps history readable).

## Review checklist

- [ ] `main` untouched; one commit per task; branch fast-forwards from the pre-run base
- [ ] every verify bead's commands are the ones the plan intended
- [ ] every task's diff adds or extends tests
- [ ] docs tasks read true against the final code
- [ ] retried tasks' failure modes understood (plan? environment? model?)
- [ ] metrics snapshot saved; retry tax and first-pass noted for the next plan
- [ ] findings routed: plan slice / factory bead / explicit "clean"
