# Eval harness

The eval harness turns attempts into evidence. Its outputs are what disqualify a tier. It has four parts: the metric vector, held-out sets split by source family, eval runs, and the review profile that governs how outputs are judged.

## Metric vector

Every attempt carries `metrics`, a map from metric name to a number between 0 and 1. It must contain every metric in `policy.required_floors` (default `quality`, `accuracy`, `completeness`). Add family-specific metrics when they matter, for example `field_f1` for extraction or `exact_match` for classification, and give each a floor in the task.

- Floors are set in the task record before any result is seen. They are never moved after results exist.
- `check_floors` compares each metric with its floor. A missing required metric is a failure with `observed: null`, never a pass.
- Each attempt names its judge: `{ "kind": "deterministic" | "model" | "human", "ref": "..." }`. `ref` identifies the scorer: a script and version, a model id, or a reviewer id. Prefer deterministic scorers where an expected output exists.
- `quality` for ranking is the mean of the required-floor metrics.

## Eval items and held-out sets

An `EvalItem` is one input with known expectations:

```
{ item_id, family, source_family, input_sha256, expected_sha256 | null, oracle: "exact" | "judge" | "human" }
```

`source_family` is where the item came from: one document set, one customer, one data source, one generator. Items from the same source share wording, layout and errors, so a model trained on some of them will score unfairly well on the rest. The held-out set therefore takes whole source families, never single items.

Run `tier-drop eval split --items items.json` (MCP: `split_holdout`). The rule:

1. For each distinct `source_family`, compute `sha256(seed_be_bytes || source_family)`, where `seed_be_bytes` is `policy.holdout.seed` as 8 big-endian bytes and `source_family` is its UTF-8 bytes.
2. Take the first 8 bytes of the digest as a big-endian unsigned 64-bit integer.
3. The family is held out when `value / u64::MAX < policy.holdout.fraction` (default 0.2).

The split is deterministic, independent of item order, and assigns every item of a source family to the same side. The result is an `EvalSplit` with `holdout_source_families`, `train_source_families` and one `{ item_id, split }` per item.

Use the split this way:

- Held-out items are the evidence set for disqualification and for comparing candidates. Never train on them.
- Train items may become training data only when their attempts' rights allow `train` (`references/dataset.md`).
- Keep the seed fixed for a family. Changing it reshuffles which sources are held out and invalidates comparisons with earlier runs.

When you run an attempt on an eval item, set the attempt's `eval_item_id`. That links the attempt to its expected output and its split.

## From attempts to an eval run

`tier-drop eval summarise --ledger DIR --family F --tier T --candidate c.json` (MCP: `summarise_eval`) aggregates the attempts for one family, tier and candidate into an `EvalRun`:

```
{ eval_run_id, family, tier, candidate, attempt_ids, n, aggregate: { metric: mean }, pass_rate, floors, recorded_at, record_sha256 }
```

`pass_rate` is the share of the `n` attempts that cleared every floor. The CLI appends the run to `eval-runs.jsonl`.

## From evidence to a disqualification

`tier-drop attempt record` calls `assess_disqualification` after every attempt. For the family and tier it produces a `Disqualification` when either:

- the evidence meets the policy: at least `policy.disqualify.min_attempts` attempts (default 5) and a pass rate at or below `policy.disqualify.max_pass_rate` (default 0.5). The reason is `floors_not_met` and `evidence` lists the attempt ids (or eval run ids);
- a structural reason applies (`shape_mismatch`, `no_provisioned_candidates`, `no_qos_qualified_candidates`, `rights_forbid_remote`).

Four failing attempts with `min_attempts: 5` produce nothing. Five produce `floors_not_met`. An eval run on held-out items is the strongest evidence, because it measures the tier on sources it has never seen.

Every disqualification binds `registry_as_of` to the registry's `as_of`. When you publish a newer registry (new models, new levels, new deployments), older disqualifications stop being in force and the tier is tried again. A disqualification with a non-null `expires_at` also stops being in force at that time, so evidence can age out on its own.

## Review profile: strict by default

The task's `review` field controls how outputs are judged. Strict is the default: every assertion in an output is checked under the strict rule, and one unsupported or wrong assertion fails it.

A sampled profile, `{ "sampled": { "class": "...", "decision_ref": "..." } }`, checks only a sample of assertions. It is allowed only when every condition below holds:

1. The task belongs to a named low-risk class, written in `class`.
2. A recorded decision, cited in `decision_ref`, approved sampling for that class.
3. The sampled profile's false-clear rate (wrong outputs it would pass) was measured within 2 % on a sealed, human-labelled set for that class.
4. The false-clear rate is re-measured continuously on new labelled samples.
5. Review reverts to strict automatically when a re-measurement exceeds 2 % or is missing.

Some content is always strict, whatever the class:

- numbers;
- absence claims ("the contract has no change-of-control clause");
- modality changes (table to text, image to text, audio to text);
- anything feeding a financial figure or a regulated disclosure.

Set `metadata.financial: "true"` or `metadata.regulated: "true"` on such tasks. The engine then refuses a sampled review, as it does for numeric extraction.
