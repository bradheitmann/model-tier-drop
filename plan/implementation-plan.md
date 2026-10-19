# Implementation plan (dependency-ordered build)

## Execution graph

```
S-01 scaffold+plan ──┬──▶ S-02 core ──▶ S-03 CLI ──┐
                     ├──▶ S-04 skill ───────────────┼──▶ S-07 dsh adapter ──┐
                     ├──▶ S-08 plan graph           │                       ├──▶ S-09 README+publish
                     │                  S-02+S-04 ──▶ S-05 worker ──▶ S-06 deploy ──┘
```

Each `S-nn` is followed by its `V-nn` before merge. Order of merges to `main`: S-01, then any of S-02 / S-04 / S-08 as they pass, then S-03 and S-05, then S-06, then S-07, then S-09.

## Waves

| Wave | Slices in parallel | Worktrees |
|---|---|---|
| 0 | S-01 (lead) | repository, branch `plan` |
| 1 | S-02 core (Opus), S-04 skill (Opus), S-08 plan graph (Opus) | `~/.codex/worktrees/model-tier-drop-s02`, `-s04`, `-s08` |
| 2 | S-03 CLI (Opus), S-05 worker (Opus) | `-s03`, `-s05` |
| 3 | S-06 deploy (lead), S-07 dsh adapter (Opus) | `-s07` |
| 4 | S-09 README + publication (lead) | branch `s09` |

QA (Sonnet) verifies each slice in its worktree before the lead merges with `--no-ff` and removes the worktree. Three total submissions per slice; the fourth is an escalation in the final report.

## Milestones

1. **Engine proven**: S-02 and S-03 accepted; the README walkthrough runs end to end on a temporary directory.
2. **Skill served**: S-04, S-05 and S-06 accepted; the hosted MCP answers `initialize`, `tools/list` and `tools/call` live and serves `SKILL.md` byte-identical to the repository.
3. **Harness attached**: S-07 accepted; an isolated dsh profile composes the adapter and the MCP mount.
4. **Published**: S-08 and S-09 accepted; the public repository builds green in CI from a fresh clone.

## Handoff

The final report to the Intake PM lists every slice with its QA verdict and commands, the final test/clippy/tsc/gitleaks outputs, the live MCP transcript, the dsh verification, and anything not done with its exact blocker.
