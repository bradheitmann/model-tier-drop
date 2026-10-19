---
name: model-tier-drop
description: Route any model-using task by tier drop, cheapest tier first, moving up only after the lower tier is disqualified for that activity on recorded evidence, and leave a routing record, eval record and dataset record behind.
whenToUse: Use before any task that calls a model (classify, extract, transform, generate, review, summarise, code) when you must choose which model runs it, justify that choice with evidence, or build eval and training data from the attempts. Do not use for work that calls no model.
metadata:
  version: "0.1.0"
  harnesses: [claude-code, codex, dsh, pi, any]
---

# Model tier drop

Choose the model for a task by starting at the cheapest tier and moving up one tier at a time. A tier is left behind only when a disqualification for the task's activity is recorded on evidence, or when the tier cannot serve the task at all (a structural skip). Every task leaves three things behind: a routing decision, its attempts (which are also dataset rows), and any disqualifications and eval runs it produced.

The engine behind this procedure never calls a model. It plans, checks floors, assesses disqualification, decides and records. You execute every model call through your harness and report the result back to the engine. You reach the engine through the `tier-drop` command line tool or through the hosted MCP tools (see Install).

## The rule

Every model-using task is routed by tier, cheapest first. A task moves up a tier only after the lower tier has been disqualified for that activity (the task `family`), on evidence. Before a tier is left behind, the evidence must show that no provisioned model and reasoning-level combination in the tier, and no composition of them, can do the job.

The five tiers are compute categories, not fixed model lists:

| Ordinal | `Tier` value | What it is |
|---|---|---|
| 1 | `system_one` | Decision-shaped models: classify, choose among fixed options, route, yes/no. |
| 2 | `specialised` | Small models trained for exactly one job, such as layout parsing or OCR. |
| 3 | `cheap_generalist` | Inexpensive generalists close to the frontier, preferably on your own hardware. |
| 4 | `frontier` | Frontier and frontier-lite models; open-weight first, then closed. |
| 5 | `tailored` | Models you fine-tune, prune, or train with reinforcement learning for one high-volume family. |

The default ladder is tiers 1, 2, 3, 4. When the registry binds a tailored model to the task's family and `policy.tailored_first` is true, `tailored` goes first, because it is built to be the cheapest qualified compute for that family. A bound tailored model with `tailored_first` false is the last tier on the ladder; an unbound `tailored` tier is not on the ladder. Which models sit in each tier is configuration data in your registry file. See `references/tiers.md` and `references/registry.md`.

Embedding models are not a tier. They are helpers any tier may use for retrieval, deduplication, clustering and routing features. They are never routing candidates.

QoS is a cutoff gate, not a weight. A deployment whose availability, p95 latency or error rate misses the task's `qos` requirement is removed before any scoring. A remote deployment is removed when the task's rights set `send_remote: false`. Unmeasured QoS is removed unless `policy.allow_unmeasured_qos` is true. Each removal carries a reason.

Ranking among qualified candidates follows a fixed order:

1. Quality, accuracy and completeness: the mean of the metrics named in `policy.required_floors`.
2. Cost per task (`cost_usd`), among candidates within `policy.quality_band` of the best quality.
3. Speed (`latency_ms`), among those within `policy.cost_band` (relative) of the cheapest.

Only attempts that answered and cleared every floor are ranked. A candidate below a floor never ranks above one that clears it. The winner comes from the lowest ladder tier that has a qualified attempt, and never from a disqualified tier.

Review is strict by default. Every assertion in an output is judged under the strict rule. A sampled review profile applies only when a recorded decision names a low-risk class; numbers, absence claims, modality changes and anything feeding a financial figure or regulated disclosure are always strict. See `references/eval-harness.md`.

## Before you route

1. **Name the family.** The family is the activity that disqualifications attach to. Use `<domain>.<activity>` in snake_case, for example `contracts.extract_parties`, and keep it stable across runs. Two tasks share a family only when the same evidence should decide both. Standing disqualifications for the family are reused, so a task does not re-run every lower tier live.
2. **Write the task record first.** Fill every `Task` field: `task_id`, `family`, `shape`, `input_sha256`, `rights`, `floors`, `qos`, `review`, `metadata`, `created_at`. The floors must include every metric in `policy.required_floors` (by default `quality`, `accuracy`, `completeness`). Floor metrics are numbers from 0 to 1, except metrics whose name ends in `_raw`, which may exceed 1. Set floors before you see any result. The ledger enforces this: `attempt record` rejects an attempt whose `recorded_at` precedes the task's `created_at` (issue `recorded_before_task_created`). If a floor was wrong, write a new task with a new `task_id`; never edit floors after results exist.
3. **Set the review profile.** Leave `review` as `"strict"` unless a recorded decision names a sampled class for this family. Set `metadata.financial` or `metadata.regulated` to `"true"` when they apply. For an `extract` task that extracts numbers, set `metadata.numeric` to `"true"`. The engine refuses sampled review in all three cases.
4. **Copy and edit the registry.** The example is `registry/registry.example.json` in the repository; print it with `tier-drop registry example > registry.json`. Replace the example deployments, models, costs and QoS with your own measured values, then run `tier-drop registry validate registry.json`. Do not route against the example registry unedited.
5. **Know the policy.** Thresholds, bands and the held-out seed live in the policy. `tier-drop policy default` prints the embedded default; copy it, edit it, and pass `--policy policy.json` to every command that takes one. Without `--policy` the default applies.
6. **Check rights.** Decide `process`, `train`, `send_remote` and `export` for the input before any call. If the input may not leave your machines, set `send_remote: false`; remote deployments are then removed by the QoS gate. Run `tier-drop qos --task task.json --registry registry.json` to see what survives the gate and why the rest was removed.
7. **Build the eval set.** Collect inputs with known expectations for the family as `EvalItem` records, each with its `source_family`, and run `tier-drop eval split --items items.json --ledger DIR`, which prints the `EvalSplit` and appends it to `DIR/eval-splits.jsonl`. This fixes which source families are held out before any attempt exists. Route each eval item as its own task, with `task_id` `<family>:<item_id>` and the attempt's `eval_item_id` set. See `references/eval-harness.md`.
8. **Pick one ledger directory** for the family and use it for every command below.

## Procedure

1. **Load the in-force disqualifications.** Run `tier-drop ledger show --ledger DIR --family F --kind disqualifications`. A disqualification is in force when its `family` matches, its `registry_as_of` is on or after the registry's `as_of`, and it has not expired. `tier-drop plan` applies this rule for you; reading the list tells you why tiers will be skipped.
2. **Plan.** Run `tier-drop plan --task task.json --registry registry.json --ledger DIR`. The result is either `{ "kind": "try", "tier", "candidates", "skipped", "detail" }` or `{ "kind": "exhausted", "skipped" }`. Each `skipped` entry gives a lower tier, a reason (`disqualified` when an in-force record exists; otherwise `shape_mismatch`, `no_provisioned_candidates` or `no_qos_qualified_candidates`) and a detail.
3. **Take candidates in the order given.** Singles (one model at one reasoning level) come first, ordered by: open-weight models before closed (`policy.prefer_open_weight`, default true), then models on local deployments before remote (`policy.prefer_local`, default true), then cost ascending, then reasoning-level order as listed in the registry. This puts open-weight models first in the frontier tier and your own equipment first among the cheap generalists. Compositions (panel, then cascade, then draft) are offered only after every single in the tier has an attempt. Do not reorder, and do not jump to a higher tier.
4. **Execute each candidate through your harness.** Send the task input to the model or models the candidate names, at the named reasoning level. For a composition, follow `references/composition.md`. Score the output against the task's floors with a recorded judge (`deterministic`, `model` or `human`, plus a `ref`).
5. **Record every attempt, including failures and abstentions.** Write the `Attempt` JSON (outcome `answered`, `abstained`, `failed` or `timed_out`; metrics; judge; `cost_usd`; `latency_ms`; tokens; hashes; the task's `rights`; `eval_item_id` for an eval item) and pipe it to `tier-drop attempt record --ledger DIR --task task.json --registry registry.json`. It first runs `validate_attempt` (task id, family, tier and model ids must match the task and registry; `recorded_at` must not precede the task's `created_at`) and exits 1 with the issues if any fail. A missing required metric counts as a floor failure. An attempt you do not record did not happen as far as the routing record is concerned.
6. **Read the attempt result.** `attempt record` prints a `FloorResult` (`passed`, and each failure with its `min` and `observed` value). It then assesses `floors_not_met` for the family and tier. A disqualification is appended only when every candidate the tier offers for the task has at least one attempt (every model and level, then panel, cascade and draft where the next tier up has a helper), and then the tier has at least `policy.disqualify.min_attempts` attempts with a pass rate at or below `policy.disqualify.max_pass_rate`. Structural skips are never written as disqualification records; they appear in the plan and the decision.
7. **Decide when to stop in a tier.** When an attempt passes every floor, the tier can serve the task. You may run the remaining singles of the same tier first so ranking can pick the cheapest and fastest qualified one. Do not try any higher tier once the current tier has a qualified attempt.
8. **Re-plan after every attempt.** Run `tier-drop plan` again. The plan moves to the next tier only when a disqualification for the current tier is in force. If the plan returns the same tier with no untried candidates and no disqualification exists, the evidence is not yet sufficient: record more attempts on held-out eval items of the family until `attempt record` can assess. Never move up a tier by hand.
9. **Use operator disqualification only with evidence.** `tier-drop disqualify --ledger DIR --registry registry.json --family F --tier T --reason REASON --evidence id[,id…] --detail "…" [--expires-at RFC3339]` records a disqualification you assert. `REASON` is any disqualification reason (`shape_mismatch`, `no_provisioned_candidates`, `no_qos_qualified_candidates`, `rights_forbid_remote`, `floors_not_met`, `operator`); use `operator` unless another fits exactly. `--evidence` cites the attempt ids or eval run ids that justify it; this is the only way an eval run id enters a disqualification. It is recorded and reviewable like any other.
10. **Decide.** Run `tier-drop decide --task task.json --registry registry.json --ledger DIR`. It appends and prints the `RoutingDecision`: the ladder, the status of each tier with references, the ranking and the winner. Use the winner's output as the task's result. When no tier qualified, `winner` is null; report that and give the `decision_id`. Running `decide` again without new attempts produces an identical record.
11. **Append nothing by hand.** Every ledger line is written by `tier-drop`, which fills `record_sha256`. Run `tier-drop ledger verify --ledger DIR` before you report.

When you use the hosted MCP tools instead of the command line tool, the computations are the same and nothing is stored on the server. The twelve MCP tools and the commands they match are:

| Command | MCP tool |
|---|---|
| `tier-drop registry example` | `registry_example` |
| `tier-drop registry validate` | `registry_validate` |
| `tier-drop qos` | `qos_gate` |
| `tier-drop plan` | `plan_next` |
| `tier-drop attempt record` | `validate_attempt` (`{ task, attempt, registry }`), then `check_floors`, then `assess_disqualification` (`{ family, tier, task, registry, attempts, policy? }`) |
| `tier-drop decide` | `decide` |
| `tier-drop eval split` | `split_holdout` |
| `tier-drop eval summarise` | `summarise_eval` |
| `tier-drop schemas` | `schemas` |
| (read this skill) | `skill_read` |

With the MCP tools you pass the task, registry, attempts and disqualifications in each call. Ledger writes still go through `tier-drop`. If the command line tool is not available, keep every record the tools return verbatim in your task output and state that the ledger was not written.

## Composition patterns

A tier may use these before it is disqualified. The judge, auditor or finisher is a helper from the next tier up, in the fixed order `system_one` → `specialised` → `cheap_generalist` → `frontier`, with `tailored` → `frontier`. `frontier` has no next tier up, so it has no compositions. A helper must pass the QoS gate and serve the task's family. The tier under test is the tier of the members, the first model or the drafter.

- **Panel.** Two to `policy.panel.max_members` models of the tier answer independently; a judge from the next tier up chooses or merges. Use it when singles of the tier fail in different ways, so selection can recover quality.
- **Cascade with audited pass-through.** The cheapest single answers every input; an auditor from the next tier up checks a sample at `policy.cascade.audit_sample_rate`. Unaudited outputs pass through. Use it for high-volume work where the single is usually right and you can measure how often it is wrong.
- **Draft.** A model of the tier drafts; a model of the next tier up finishes. Use it when the tier gets structure and coverage right but misses precision the finisher can add cheaply.
- **Embedding helpers.** Retrieval, deduplication, clustering or routing features computed with an embedding model. Any tier and any composition may use them. They are never a candidate and never a tier.

Details, cost accounting and how to record each pattern: `references/composition.md`.

## Records you leave behind

All records are canonical JSON lines under one ledger directory, each with `schema_version` and `record_sha256`:

| File | Record | Written by |
|---|---|---|
| `DIR/tasks.jsonl` | `Task` | `tier-drop attempt record` and `tier-drop decide`, when the `task_id` is not yet present |
| `DIR/attempts.jsonl` | `Attempt` (also the dataset row) | `tier-drop attempt record` |
| `DIR/disqualifications.jsonl` | `Disqualification` | `tier-drop attempt record` (`floors_not_met`), `tier-drop disqualify` |
| `DIR/decisions.jsonl` | `RoutingDecision` | `tier-drop decide` |
| `DIR/eval-runs.jsonl` | `EvalRun` | `tier-drop eval summarise` |
| `DIR/eval-splits.jsonl` | `EvalSplit` | `tier-drop eval split --ledger DIR` |

Read any of them with `tier-drop ledger show --ledger DIR [--family F] --kind attempts|decisions|disqualifications|eval-runs|eval-splits|tasks`.

The routing decision states which tiers were tried, why each was disqualified or skipped, which candidate won, and on what evidence. Field-by-field meaning: `references/routing-record.md`. Print the JSON Schema for any record with `tier-drop schemas NAME`.

## Eval harness and datasets

- `references/eval-harness.md`: the metric vector, held-out sets split by whole source family, how eval attempts and eval runs become disqualification evidence, and the strict review default with the conditions for the sampled exception.
- `references/dataset.md`: every attempt as a dataset row, rights by purpose, `tier-drop dataset export --purpose`, deletion, and what makes a family a tier-5 candidate.
- `references/tiers.md`: what each tier is for, what disqualifies it, and the structural skips.
- `references/registry.md`: registry fields, adding a deployment, QoS as measured values, model names as configuration.
- `references/composition.md`: panels, cascades, drafts and embedding helpers in detail.
- `references/routing-record.md`: every `RoutingDecision` field.

## Install

**Command line tool.** From a clone of the repository: `cargo install --path crates/cli`. This installs `tier-drop`. Check with `tier-drop schemas routing_decision`.

**Hosted MCP.** Live since 2026-10-19 at `https://model-tier-drop-mcp.bradheitmann.ai/mcp` (Streamable HTTP, JSON responses, stateless, no authentication, nothing stored). The same server serves this file at `GET /skill/SKILL.md`, each reference at `GET /skill/references/<name>.md`, the example registry at `GET /registry/example.json` and schemas at `GET /schemas/<name>.json`.

- Claude Code: `claude mcp add --transport http tier-drop https://model-tier-drop-mcp.bradheitmann.ai/mcp`. Tools then appear as `mcp__tier-drop__plan_next` and so on.
- Codex: add to `~/.codex/config.toml`:
  ```
  [mcp_servers.tier-drop]
  url = "https://model-tier-drop-mcp.bradheitmann.ai/mcp"
  ```
- dsh: apply the repository's `adapters/dsh/cordis.patch.yml`, which mounts the server through `@deepseek-ai/dsh-mcp-client` with `transport: streamable-http` and `serverName: tier-drop`. The adapter also adds local tools (`tier_drop_plan`, `tier_drop_qos`, `tier_drop_record_attempt`, `tier_drop_decide`, `tier_drop_ledger_show`) that call `tier-drop` and write the ledger.
- Pi: Pi has no MCP client inside the agent. Reach the server through `mcp-remote` in `~/.pi/mcp.json`:
  ```
  { "mcpServers": { "tier-drop": { "command": "npx",
      "args": ["-y", "mcp-remote", "https://model-tier-drop-mcp.bradheitmann.ai/mcp"] } } }
  ```
  For ledger writes, call `tier-drop` from an extension in `~/.pi/agent/extensions/`.
- Any other harness: fetch `GET /skill/SKILL.md` and call the MCP tools, or run `tier-drop` from a shell.

**Skill.** Symlink the repository's `skill/` directory as `model-tier-drop`:

| Harness | Path |
|---|---|
| Claude Code | `~/.claude/skills/model-tier-drop` |
| Codex | `~/.agents/skills/model-tier-drop` |
| dsh | `~/.dsh/skills/model-tier-drop` (it also reads `~/.agents/skills`) |
| Pi | `~/.pi/agent/skills/model-tier-drop` |

Example: `ln -s "$PWD/skill" ~/.agents/skills/model-tier-drop`.
