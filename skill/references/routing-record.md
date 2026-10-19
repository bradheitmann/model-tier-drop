# Routing record

Every routed task ends with one `RoutingDecision` appended to `<ledger_dir>/decisions.jsonl` by `tier-drop decide` (or returned by the MCP tool `decide`, which stores nothing). It answers four questions: which tiers were tried, why each lower tier was disqualified or skipped, which candidate won, and on what evidence.

Print its JSON Schema with `tier-drop schemas routing_decision`.

## Fields

| Field | Type | Meaning |
|---|---|---|
| `schema_version` | string | Record format version. Every ledger record carries one. |
| `decision_id` | string | `rd-` followed by the first 16 hex characters of SHA-256 over the `task_id` and the sorted ids of the attempts considered. The same inputs always give the same id. |
| `task_id` | string | The task this decision routes. Matches a line in `tasks.jsonl`. |
| `family` | string | The task's activity. Disqualifications are matched on it. |
| `ladder` | array of `Tier` | The tier order used for this task, from `ladder(task)`. |
| `tiers` | array | One entry per ladder tier, in ladder order (below). |
| `winner` | object or null | The chosen candidate and its evidence (below). Null when no tier has a qualified attempt. |
| `ranking` | array | Every qualified attempt across all tiers, lowest ladder tier first (below). |
| `review` | `"strict"` or `{ "sampled": { "class", "decision_ref" } }` | The task's review profile, copied from the task. |
| `policy_sha256` | string | SHA-256 of the canonical policy used, so the decision can be reproduced. |
| `decided_at` | RFC 3339 UTC timestamp | The latest `recorded_at` among the attempts considered, or the task's `created_at` when there are none. The engine reads no clock. |
| `record_sha256` | string | SHA-256 of the canonical JSON of this record without this field. |

### `tiers[]`

| Field | Meaning |
|---|---|
| `tier` | The `Tier` value. |
| `status` | One of `disqualified`, `skipped`, `tried`, `selected` (below). |
| `refs` | Ids that justify the status: disqualification ids for `disqualified`, attempt ids for `tried` and `selected`, empty for `skipped`. |
| `detail` | For `skipped`, the reason and its detail; otherwise one sentence on the status. |

Status values:

- `disqualified`: an in-force disqualification exists for the family and tier. `refs` holds the disqualification ids. This tier never supplies the winner, even if it has a passing attempt.
- `skipped`: the tier was not tried. `detail` gives a structural reason (`shape_mismatch`, `no_provisioned_candidates`, `no_qos_qualified_candidates`) or `not_tried` for an open tier above the winner that has no attempts.
- `tried`: attempts exist, but the tier neither won nor is disqualified.
- `selected`: the winner came from this tier.

`tailored` appears in `tiers` in one of two places: first, when a tailored model is bound to the family (it is then first on the ladder), or last as a `skipped` entry when none is bound.

Tiers above the selected one normally show `skipped` with `not_tried`. That is expected: the rule forbids trying a higher tier once a lower tier qualifies.

### `winner`

| Field | Meaning |
|---|---|
| `candidate` | The `Candidate` that won: `single`, `panel`, `cascade` or `draft` (see `references/composition.md`). |
| `attempt_id` | The attempt whose output is the task's result. |
| `floor_result` | The `FloorResult` of that attempt: `attempt_id`, `passed` (always true for a winner), `failures` (empty for a winner). |
| `score.quality` | Mean of the attempt's metrics named in `policy.required_floors`. |
| `score.cost_usd` | The attempt's cost per task. |
| `score.latency_ms` | The attempt's latency. |

### `ranking[]`

Each entry has `candidate`, `attempt_id`, `quality`, `cost_usd`, `latency_ms`. Only attempts with `outcome: "answered"` whose `FloorResult` passed are ranked. Entries are grouped by tier, lowest ladder tier first. Inside each tier the band rule orders them:

1. Sort by `quality`, highest first.
2. Candidates whose quality is within `policy.quality_band` (absolute, default 0.02) of the best form the top band. Inside it, order by `cost_usd`, cheapest first.
3. Candidates whose cost is within `policy.cost_band` (relative, default 0.10) of the cheapest in the band are ordered by `latency_ms`, fastest first.

The winner is the first entry of the lowest ladder tier that has a qualified attempt and is not disqualified. A cheaper higher-tier attempt never beats a qualified lower-tier one, and an attempt below a floor is never ranked at all.

## Related records the decision points to

`Attempt` (in `attempts.jsonl`): `schema_version`, `attempt_id`, `task_id`, `family`, `tier`, `candidate`, `outcome`, `metrics`, `judge` (`kind`, `ref`), `cost_usd`, `latency_ms`, `tokens` (`input`, `output`), `input_sha256`, `output_sha256`, `rights`, `eval_item_id`, `recorded_at`, `record_sha256`.

`Disqualification` (in `disqualifications.jsonl`): `schema_version`, `disqualification_id`, `family`, `tier`, `reason` (`floors_not_met` or `operator`), `evidence` (attempt ids, or eval run ids for an operator record), `detail`, `registry_as_of`, `decided_at`, `expires_at`, `record_sha256`.

`record_sha256` may be omitted on input; `tier-drop` fills it before writing.

## Reading a decision

To answer "why did this task run on that model": open the decision, read `tiers` from the top, follow each `refs` id into `disqualifications.jsonl` or `attempts.jsonl`, then read `winner.floor_result` and the first entries of `ranking`. To check that nothing was altered, run `tier-drop ledger verify --ledger DIR`; it recomputes every `record_sha256` and names any `file:line` that does not match.

Never edit or append a decision by hand. Running `tier-drop decide` again with no new attempts yields an identical record. After new attempts, or after fixing the registry or policy, `decide` writes a new record. The ledger keeps every line; the last line for a `task_id` in `decisions.jsonl` is the current decision.
