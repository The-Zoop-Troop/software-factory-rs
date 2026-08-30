# Guide: your first project through the factory

- **Status:** draft · **Verified:** in progress (a multi-repo feature run, 2026-08-30)

This walks one real change through the factory end to end: several repositories, a feature
branch that must never touch `main`, tests as the definition of done, and an operator who only
uses the browser console. Every command is copy-pasteable; placeholders are in `<angle brackets>`.

## 0. What you need before you start

- Repositories the factory may **clone and push branches to**, each with a feature branch
  (`<feature-branch>`) checked out on the remote. `main` stays untouched: the rig refuses to land
  on a protected branch (`RIG_PROTECTED_BRANCHES`, default `main,master`) — protect `main` on the
  remote too.
- A credential scoped to exactly those repositories (a fine-grained GitHub token: *Contents:
  read/write*, nothing else). The rig never sees your SSH keys.
- A harness credential (Codex, Claude Code or an OpenAI-compatible provider for OpenCode).
- A verify command per repository that proves the change works (tests, not just a build). If a
  repository has no tests, the first epic adds them.
- A plan in prose: what to build, in phases, with acceptance criteria you would check yourself.

_(sections 1–8 are filled in as the run proceeds)_

## 1. Prepare each repository
## 2. Create the rigs
## 3. Open the console
## 4. Write and submit the epics
## 5. Watch
## 6. Act on incidents
## 7. Review what landed
## 8. The end-state sweep and teardown
