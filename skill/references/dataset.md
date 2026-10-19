# Dataset

Every attempt is a dataset row. The ledger's `attempts.jsonl` is the dataset; there is no second copy to keep in step. Building the dataset is therefore not a separate job: it happens when you follow the procedure and record every attempt.

## Every attempt, including failures and abstentions

Record an attempt for every candidate you execute, whatever happened:

| `outcome` | Meaning | Why it is kept |
|---|---|---|
| `answered` | The candidate returned an output, which was scored. | Positive and negative examples, depending on the floors. |
| `abstained` | The candidate declined or said it could not answer. | Shows where a tier knows its limits; useful for routers and for calibration. |
| `failed` | The call errored or the output could not be parsed or scored. | Counts against the tier and feeds QoS measurement. |
| `timed_out` | No answer within the harness's limit. | Counts against the tier and feeds latency measurement. |

A dataset made only of successes overstates every tier. Failures and abstentions are the evidence that disqualifies a tier and the negatives a tailored model learns from.

Each row carries what training and evaluation need without reopening the source: `task_id`, `family`, `tier`, `candidate`, `outcome`, `metrics`, `judge`, `cost_usd`, `latency_ms`, `tokens`, `input_sha256`, `output_sha256` (null when there is no output), `rights`, `eval_item_id`, `recorded_at`, `record_sha256`. The ledger stores hashes of inputs and outputs, not the content. Keep content in your own store, addressed by those hashes.

## Rights by purpose

Each attempt carries the task's `Rights` as they were at attempt time:

```
{ "process": bool, "train": bool, "send_remote": bool, "export": bool }
```

| Purpose | What it allows |
|---|---|
| `process` | Running models on the input at all. |
| `train` | Using the input and output to train, fine-tune or evaluate a model you build. |
| `send_remote` | Sending the input to a deployment with `locality: "remote"`. When false, the QoS gate removes remote deployments with `rights_forbid_remote`. |
| `export` | Copying the rows out of the ledger to another system or party. |

Decide rights before the first call and write them in the task. Rights on recorded attempts are never edited. If the rights for a source change, write new tasks with the new rights; the old rows keep the rights they were made under.

## Export filtered by purpose

```
tier-drop dataset export --ledger DIR --purpose process|train|send_remote|export [--family F]
```

The export returns only attempts whose rights allow the named purpose (`dataset_filter(purpose)` in the engine). An attempt with `train: false` is omitted from `--purpose train` and included in `--purpose process` when `process` is true. Use the purpose that matches what you will do with the rows: `train` before building a tailored model, `export` before handing rows to anyone else.

Combine the export with the held-out split: train only on rows whose `eval_item_id` falls in the train split, and keep held-out rows for evaluation (`references/eval-harness.md`).

## Deletion

`tier-drop` only appends; it never deletes. Deletion, when a source's owner requires it, is a local file operation: remove the affected lines from `attempts.jsonl` (and from other files that refer to the same task, if required), then run `tier-drop ledger verify --ledger DIR`. Each line has its own `record_sha256`, so removing whole lines does not break the others. Removing attempts removes evidence: a disqualification whose `evidence` lists deleted attempt ids is no longer backed by the ledger, so re-run the family's evaluation and let `attempt record` assess again.

## When a family becomes a tier-5 candidate

A tailored model is worth building for a family when all of these hold:

1. **High volume.** The family repeats constantly, so a cheaper or faster model saves real money or time. Count the family's tasks in `tasks.jsonl` over a recent window.
2. **Stable.** The task definition, floors and input sources have not changed over that window. A family whose floors or shape keep changing will outrun any tailored model.
3. **Measured pass rate.** A lower tier, usually tier 3 or tier 4, already clears the floors reliably, measured by an `EvalRun` on held-out source families. The tailored model has a target and a teacher.
4. **Sample size.** There are enough recorded attempts to train and to evaluate on held-out source families separately. As a starting point, aim for several hundred train-split rows and a held-out set covering at least ten source families; these are guidance, not policy fields.
5. **Rights allow training.** `tier-drop dataset export --purpose train --family F` returns enough rows. Rows without `train: true` do not count.

Once built, add the model to the registry with `tier: "tailored"` and the family in `families`. With `policy.tailored_first` true it is tried first for that family, and it is disqualified on evidence like any other tier.
