# Amendment 2026-10-31: adjacent escalation moves

Status: proposed by PM-MODEL-TIER-DROP; seam agreed with PM-MISTER-CLEAN and A/EXEC-PM on 2026-10-31. Mister Clean's half: `project/planning/seams/ESCALATION-ADJACENCY-SEAM.md` in mister-clean (merged to mister-clean main a900908 via PR 14; harness echo in the Move left open, not needed now). Binds slice S-10 / V-10.

## Source

Owner decision 2026-10-31 (recorded in reveng `charter/owner-decisions/2026-10-31-model-routing-and-escalation.md` §2 to §4):

- tier-drop does not change; frontier models are used when available, with no opt-in.
- When a recommended model fails at a task, the failure goes to Mister Clean as a case study and unlocks adjacent options for the next run: a higher reasoning level; a different model at the same tier (another family or architecture); one tier up; or one tier down to a specialised small model that fits the task. Never several tiers at once. Tier boundaries are fuzzy today and sharpen with data.

## The seam

| Concern | Owner |
|---|---|
| Tier ladder data: each provisioned model's tier, lineage, reasoning levels, shapes, bound families, QoS | model-tier-drop (`registry`) |
| Enumerating the lawful adjacent moves from a failed candidate | model-tier-drop (`adjacent`, pure core) |
| Choosing among the moves (failure class, evals, privacy, quota, free-inference tripwire), the failure case study, the outcome of the next run | Mister Clean (router) |
| The routing decision for the move taken | model-tier-drop ledger; Mister Clean cites its id and never copies it |
| Evidence that a tier boundary is wrong | Mister Clean sends a dated observation; tier-drop binds or rejects it through a registry change under plan/ and QA |

The engine stays pure: `adjacent` reads its arguments and returns a value. It runs no model and reads no file, clock or network. Harnesses are not modelled; Mister Clean adds the harness when it turns a move into an agent card.

## Relationship to the ladder rule

`plan_next` is unchanged: a tier is left only after a recorded disqualification with full coverage. `adjacent` answers a different question, asked by a router that started from a recommended candidate rather than from the bottom of the ladder: "after this candidate failed, which next candidates are one step away?" Both use the same registry, the same task ladder and the same QoS gate, so the two never disagree about which models fit a task.

## Definitions

- **Effective ladder.** `ladder(task)` filtered to tiers with at least one *fitting* model and no in-force disqualification for the task's family. A tier that is empty for this task, or disqualified for its family, is not a step. Each move lists the tiers it passed over this way in `passed_over`, so a skipped tier is never silent.
- **Fitting model.** The model serves `task.shape`, its `families` is empty or contains `task.family`, and it passes `qos_gate` for the task (QoS and `send_remote` rights).
- **Lineage.** A new optional `Model.lineage` string, naming the model's maker and architecture family (for example `qwen`, `deepseek`, `glm`). This is a model fact owned by model-atlas (card fields `lineage.maker` and `lineage.architecture` in model-atlas schemas v1 at a0717d8; agreed with PM-MODEL-ATLAS 2026-10-31); until a registry resolve step reads model-atlas exports, the example registry carries it as configuration. A missing lineage on either side counts as different, and the move carries `lineage_known: false`.

## Moves, in output order

1. `more_reasoning`: the same model at the next reasoning level after `from.reasoning_level` in the registry's order (levels are listed lowest first). Only the next level. No move when `from` is already at its last level.
2. `same_tier_other_lineage`: every fitting model in `from`'s tier whose lineage differs from `from`'s, at each of its reasoning levels.
3. `one_tier_up`: every fitting model in the next tier up the effective ladder, at each of its reasoning levels. No move when `from`'s tier is the top of the effective ladder.
4. `one_tier_down_specialist`: every fitting model in the next tier down the effective ladder that is bound to the task's family (`families` contains `task.family`) or is a `system_one` model (which serves only decision shapes), at each of its reasoning levels.

Within a kind, targets follow the existing single-candidate order (open-weight first, local first, cost ascending, registry level order). A move never spans more than one effective-ladder step. An empty list is valid and means a human decides next.

## Record

```
AdjacentMoves {
  schema_version: "tier-drop.adjacent_moves/1",
  task_id, family,
  from: { model_id, reasoning_level, tier: Tier },
  ladder: [Tier],                       // the effective ladder used
  moves: [ {
    kind: "more_reasoning" | "same_tier_other_lineage" | "one_tier_up" | "one_tier_down_specialist",
    to: { model_id, reasoning_level },
    tier_from: Tier, tier_to: Tier,
    passed_over: [ { tier, reason } ], // reason: no_fitting_candidates | disqualified
    lineage_known: bool
  } ],
  issues: [Issue]                       // non-empty only when the input is invalid; moves is then empty
}
```

Invalid input yields issues: `unknown_model` (from.model_id not in the registry), `unknown_reasoning_level`, `from_not_fitting` (a warning, not a refusal: the router may have started from a model the gate would now exclude; moves are still enumerated).

AdjacentMoves is a pure answer and is not a ledger record. The move the router takes is recorded as an ordinary attempt for the next run, so the existing records already hold it.

## Surfaces

- Core: `adjacent(from: &ModelRef, &Task, &Registry, &[Disqualification], &Policy) -> AdjacentMoves`.
- CLI: `tier-drop adjacent --from from.json --task t.json --registry r.json [--ledger DIR] [--policy p.json]` (the ledger supplies in-force disqualifications, as for `plan`).
- MCP: tool `adjacent` `{ from, task, registry, disqualifications?, policy? }` returns AdjacentMoves (thirteen tools).
- Schemas: `adjacent_moves` added (eleven names).
- Skill: a short section "After a failure: adjacent moves" saying the router chooses and tier-drop enumerates.

## Owner decision point

Skipping a tier that has no fitting model, or is disqualified for the family, counts as one step, and the move records it in `passed_over`. This is PM-MODEL-TIER-DROP's reading of "never multiple tiers" (an empty or disproven tier adds no capability). If the owner reads it otherwise, `one_tier_up` and `one_tier_down_specialist` return nothing when the neighbouring tier is empty. That is a one-line change, and S-10 keeps a test for each reading.
