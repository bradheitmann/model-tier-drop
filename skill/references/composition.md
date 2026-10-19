# Composition patterns

Before a tier is left behind, the evidence must show that no single model and reasoning level in it, and no composition of them, can do the job. `tier-drop plan` offers compositions only after every single in the tier has an attempt, in this order: panel, cascade, draft. A composition is a `Candidate` with a `kind` tag. Each is recorded as one `Attempt` whose `tier` is the tier under test.

The tier under test is the tier of the panel members, the cascade's first model or the drafter. The judge, auditor or finisher is a helper from the next tier up, in the fixed order `system_one` → `specialised` → `cheap_generalist` → `frontier`, with `tailored` → `frontier`. `frontier` has no next tier up, so it has no compositions. The helper is the cheapest model of the next tier up that passes the QoS gate and serves the task's family. When no such helper exists, the plan offers no compositions for the tier, and the disqualification coverage rule does not require them.

## Panel

```
{ "kind": "panel",
  "members": [ { "model_id": "...", "reasoning_level": "..." }, ... ],
  "judge":   { "model_id": "...", "reasoning_level": "..." } }
```

How to execute: send the same input to every member independently. Do not show members each other's outputs. Give the judge the input and all member outputs, and ask it to pick the best output or merge them into one answer under the task's rules. The judge's output is the attempt's output.

The plan builds the panel from the cheapest QoS-qualified models of the tier, at their lowest reasoning level, up to `policy.panel.max_members`. A panel needs at least two members; a tier with only one qualified model gets no panel.

When to use: singles of the tier fail in different, uncorrelated ways. Selection or agreement then recovers quality the singles miss alone. A panel rarely helps when every member fails on the same inputs.

How to record: one attempt with the panel candidate. `cost_usd` is the sum of every member call and the judge call. `latency_ms` is wall time from first request to the judge's answer. Score the final output against the floors as for a single.

## Cascade with audited pass-through

```
{ "kind": "cascade",
  "first":   { "model_id": "...", "reasoning_level": "..." },
  "auditor": { "model_id": "...", "reasoning_level": "..." },
  "audit_sample_rate": 0.1 }
```

How to execute: the first model (the cheapest single of the tier) answers. A sample of outputs, at `audit_sample_rate` (from `policy.cascade.audit_sample_rate`), is sent to the auditor with the input. The auditor accepts or corrects. Unaudited outputs pass through unchanged. Decide which inputs are sampled before you see the output, so sampling cannot be steered toward easy cases.

When to use: high-volume work where the first model is usually right. The audits measure how often it is wrong and correct the sampled cases.

How to record: one attempt per input, with the cascade candidate. When the input was audited, the attempt's metrics come from scoring the audited output, and `cost_usd` includes the auditor call. When it was not audited, score the passed-through output with the task's judge as usual. The pass-through is audited in aggregate: the audited attempts in the ledger measure the error rate of the unaudited ones, and that rate is evidence for or against the tier.

## Draft for the next tier

```
{ "kind": "draft",
  "drafter":  { "model_id": "...", "reasoning_level": "..." },
  "finisher": { "model_id": "...", "reasoning_level": "..." } }
```

How to execute: the drafter (the cheapest single of the tier) produces a full draft. The finisher receives the input and the draft and returns the final output. Tell the finisher to keep what is correct and fix what is not, so its work is short.

When to use: the tier gets structure, coverage or format right but misses precision that a stronger model can add at a fraction of the cost of writing from scratch.

How to record: one attempt with the draft candidate. `cost_usd` is drafter plus finisher. Score the finisher's output. If the draft wins, the routing record shows the tier under test as `selected` and names the finisher in the candidate, so the next-tier spend is visible.

## Embedding helpers

Embedding models are not a tier and are never candidates. `validate_registry` rejects an embedding id reused as a model id. Any tier and any composition may use them for:

- retrieval: picking the passages or examples sent with the input;
- deduplication: collapsing near-identical inputs or outputs before judging;
- clustering: grouping inputs into source families for held-out splits;
- routing features: signals a tier-1 router uses to choose among fixed routes.

How to record: there is no separate record. Add the embedding cost to the attempt's `cost_usd` and its time to `latency_ms`. Embedding models are listed under `embeddings` in the registry with `id`, `deployment_id`, `dimensions` and `notes`. A remote embedding deployment is subject to the same `send_remote` right as any model.

## What a composition does not change

- Floors are the task's floors. A composition never lowers them.
- Rights apply to every model in the composition. A remote judge, auditor or finisher is removed by the QoS gate when `send_remote` is false.
- Review is the task's review profile. A judge model scoring an output follows the strict rule unless a recorded decision allows sampling.
- A composition that fails counts as evidence against the tier, like any failed single.
