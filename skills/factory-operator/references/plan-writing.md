# Writing good plans

The plan text is **the whole contract** the planner works from. It becomes an epic of 3–8
tasks, each with acceptance criteria and a verify command. Most bad runs trace back to a
plan that made the planner guess.

## Shape

One epic per phase per repository. 300–600 words. Structure it as:

1. **Goal** — one paragraph, outcome not activity ("users can reset passwords by email",
   not "add code to the auth module").
2. **Repository facts** it should rely on — file paths, existing types and functions, where
   similar code lives. The planner reads the repo, but naming the anchors removes guesses.
3. **Deliver** — an explicit bulleted list of artifacts (endpoints, functions, components,
   migrations, docs).
4. **Tests** — the cases that define done, by name ("resetting with an expired token
   returns 410 and sends nothing"). "Add tests" is not a case.
5. **Docs to update** — which files.
6. **Constraints** — language/framework rules, what NOT to touch, what external systems do
   not exist in the rig (no live payment gateway, no vendor API — say what to fake instead),
   and environment rules ("pin `playwright` to the preinstalled version; never run
   `playwright install`").
7. **Verify** — the command(s) every task must leave passing.

## Template

```
Goal: <one paragraph>.

Context: <paths, types, existing patterns to follow>.

Deliver:
- <artifact>
- <artifact>

Tests (define done):
- <named case>
- <named case>

Docs: update <files>.

Constraints: <rules; absent systems and their fakes; environment rules>.

Every task must leave `<verify command>` passing.
```

## Context beyond the repo: reference beads

Decisions and phase context that live outside the repository go in as **reference beads** —
every worker session reads them before starting:

```sh
$R exec -T steward sh -c 'cd /work/rig && bd create "Plan: settled decisions" -t task -p 3 \
  -l fac:kind=reference --no-inherit-labels --body-file - --json' < decisions.md
```

Use them for: the decisions table from your exec plan, API contracts between repos, style
rules too long for the epic text.

## Phase gating across rigs

Pick upstream epics in the plan form's **After** field (or
`factory --rig … plan --after backend:be-1`). The request waits until each closes; the
console then appends their **contracts** — what actually landed: commit range, files, public
surface — and the planner reads those instead of your guesses about the upstream work. Keep
the phase order written in the exec plan so anyone can see what waits on what. A canceled
upstream epic surfaces as a question on the dependent rig, not a silent hang.

## Budgets, models, effort

- `RIG_TASK_TOKENS` sets the per-task token budget the planner writes (default 400k). Large
  refactors and chatty harnesses need more; a budget-exhausted incident mid-run is a signal
  to raise it, not to retry blind.
- Plan strong, work cheap: `RIG_PLANNER_MODEL`/`RIG_PLANNER_EFFORT` vs
  `RIG_WORKER_MODEL`/`RIG_WORKER_EFFORT` (effort: low|medium|high|max). The same flags exist
  per invocation on `factory plan` / `work`.
- Planner spend cap: `factory plan --max-budget-usd` (claude only).

## Anti-patterns

- **The kitchen-sink epic** — 2000 words spanning three concerns. Split into phases; gate
  with After.
- **Outcome-free tasks** — "improve error handling". The planner cannot write a verify
  command for it; you get a build-passes epic.
- **Secret constraints** — anything you know about the environment that the plan does not
  say, the model will trip over (browsers, absent daemons, protected paths).
- **Re-planning what needed guidance** — when one task misread the code, Retry-with-guidance
  on that task beats a new epic.
- **Hand-editing the branch mid-epic** — allowed (the Integrator fetches and fast-forwards)
  but the next session rebases onto it blind; prefer a review-notes plan slice after the
  epic closes.
