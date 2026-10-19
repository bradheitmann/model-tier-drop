# Eval harness

The eval harness turns attempts into evidence. Its outputs are what disqualify a tier. It has four parts: the metric vector, held-out sets split by source family, eval attempts and eval runs, and the review profile that governs how outputs are judged.

## Metric vector

Every attempt carries `metrics`, a map from metric name to a number. It must contain every metric in `policy.required_floors` (default `quality`, `accuracy`, `completeness`). Add family-specific metrics when they matter, for example `field_f1` for extraction or `exact_match` for classification, and give each a floor in the task.

- Metrics named in floors are between 0 and 1, except metrics whose name ends in `_raw` (for example `tokens_saved_raw`), which may exceed 1.
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

Build the item list and run `tier-drop eval split --items items.json --ledger DIR` (MCP: `split_holdout`) before the first attempt, so the held-out families exist from the start. The rule:

1. For each distinct `source_family`, compute `sha256(seed_be_bytes || source_family)`, where `seed_be_bytes` is `policy.holdout.seed` as 8 big-endian bytes and `source_family` is its UTF-8 bytes.
2. Take the first 8 bytes of the digest as a big-endian unsigned 64-bit integer.
3. The family is held out when `value / u64::MAX < policy.holdout.fraction` (default 0.2).

The split is deterministic, independent of item order, and assigns every item of a source family to the same side. The result is an `EvalSplit`:

```
{ schema_version, family, seed, fraction, holdout_source_families, train_source_families,
  items: [ { item_id, split: "holdout" | "train" } ], record_sha256 }
```

With `--ledger DIR` the command line tool appends it to `DIR/eval-splits.jsonl`, so the split in force for a family is on record before any attempt. Read it back with `tier-drop ledger show --ledger DIR --family F --kind eval-splits`.

Use the split this way:

- Held-out items are the evidence set for disqualification and for comparing candidates. Never train on them.
- Train items may become training data only when their attempts' rights allow `train` (`references/dataset.md`).
- Keep the seed fixed for a family. Changing it reshuffles which sources are held out and invalidates comparisons with earlier runs.

## Eval attempts

An eval attempt is an ordinary attempt with `eval_item_id` set. Route each eval item as its own task: one task per eval item, with `task_id` `<family>:<item_id>` recommended, the family's floors, and the item's `input_sha256`. Record every eval attempt through `tier-drop attempt record`. That is what feeds disqualification: the attempts in the ledger are the evidence.

## From attempts to a disqualification

After every attempt, `tier-drop attempt record` calls `assess_disqualification` for the family and tier. It assesses only `floors_not_met`. A disqualification is appended when both hold:

1. **Full coverage** (`policy.disqualify.require_full_coverage`, default true): every candidate the tier offers for the task has at least one attempt. That is every model and reasoning level, then the panel, cascade and draft when a next-tier helper exists.
2. **Enough failing evidence**: at least `policy.disqualify.min_attempts` attempts (default 5) and a pass rate at or below `policy.disqualify.max_pass_rate` (default 0.5).

Four failing attempts with `min_attempts: 5` produce nothing. Five, with full coverage, produce `floors_not_met` with the attempt ids as `evidence`. The record's `disqualification_id` is `dq-` plus the first 16 hex characters of SHA-256 over the family, the tier and the sorted evidence attempt ids, and its `decided_at` is the latest `recorded_at` among those attempts, so the same evidence always gives the same record. The MCP tool `assess_disqualification` takes `{ family, tier, task, registry, attempts, policy? }`; it needs the registry to list the tier's candidates for the coverage rule and takes `registry_as_of` from it. Attempts on held-out items are the strongest evidence, because they measure the tier on sources it has never seen.

Structural reasons (`shape_mismatch`, `no_provisioned_candidates`, `no_qos_qualified_candidates`) are never written as disqualification records. They are reported in the plan and in the decision's `tiers[]`.

## Eval runs

`tier-drop eval summarise --ledger DIR --family F --tier T --candidate c.json` (MCP: `summarise_eval`) writes an `EvalRun` summary for one family, tier and candidate:

```
{ schema_version, eval_run_id, family, tier, candidate, attempt_ids, n, aggregate: { metric: mean }, pass_rate, floors, recorded_at, record_sha256 }
```

`pass_rate` is the share of the `n` attempts that cleared every floor. `eval_run_id` is a derived id with the `er-` prefix. The CLI appends the run to `eval-runs.jsonl`. An eval run summarises evidence; it does not by itself disqualify anything. To disqualify a tier on an eval run, record a disqualification with `tier-drop disqualify` that cites it:

```
tier-drop disqualify --ledger DIR --registry registry.json --family F --tier T \
  --reason operator --evidence er-…[,attempt-id…] --detail "…" [--expires-at RFC3339]
```

`--reason` accepts any disqualification reason (`shape_mismatch`, `no_provisioned_candidates`, `no_qos_qualified_candidates`, `rights_forbid_remote`, `floors_not_met`, `operator`); `operator` is the usual choice when you cite an eval run. The record takes `registry_as_of` from the registry and `decided_at` from the command line tool's clock. This is the only path by which an eval run id enters a disqualification's `evidence`.

## Lifetime of a disqualification

Every disqualification binds `registry_as_of` to the registry's `as_of`. When you publish a newer registry (new models, new levels, new deployments), older disqualifications stop being in force and the tier is tried again. A disqualification with a non-null `expires_at` stops being in force at that time: the CLI drops expired records using its own clock before calling the engine, which reads no clock.

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

Mark such tasks so the engine enforces it. Set `metadata.financial: "true"` or `metadata.regulated: "true"` where they apply. For an `extract` task that extracts numbers, set `metadata.numeric: "true"`. The engine refuses a sampled review for any task carrying one of these.
