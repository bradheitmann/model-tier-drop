# Tiers

A tier is a compute category. It says what kind of model the work runs on, not which model. Which models belong to a tier is configuration data in the registry (`references/registry.md`). The model names on this page are indicative as of 2026-10-19; they are examples of the category, not instructions to use them.

## The ladder

`Tier` values and their ordinals:

| `Tier` value | Ordinal | Default ladder position |
|---|---|---|
| `system_one` | 1 | first |
| `specialised` | 2 | second |
| `cheap_generalist` | 3 | third |
| `frontier` | 4 | fourth |
| `tailored` | 5 | first when bound and `policy.tailored_first` is true, otherwise last |

`ladder(task)` starts from `[system_one, specialised, cheap_generalist, frontier]`. When the registry has a `tailored` model whose `families` include the task's family and `policy.tailored_first` is true, `tailored` is placed at the front. Otherwise it is appended at the end, where it is skipped unless a tailored model is bound to the family.

The task moves along the ladder one tier at a time. It leaves a tier only when a disqualification for that tier and the task's family is in force, or when `tier-drop plan` skips the tier for a structural reason. "Next tier up" in this skill means the next tier on the task's ladder.

## Tier 1: `system_one`

What it is for: decision-shaped work. The tier accepts only the decision shapes `classify`, `choose`, `route` and `yes_no`. Output is a label, a choice among fixed options, a route or a yes/no answer.

Indicative models as of 2026-10-19: Jev, Laya and similar decision models.

What disqualifies it: attempts on the family that fail the floors, for example a classifier whose `accuracy` on held-out items stays below the floor across at least `policy.disqualify.min_attempts` attempts with a pass rate at or below `policy.disqualify.max_pass_rate`.

Structural skip: `shape_mismatch` for any task whose `shape` is not a decision shape. A `generate`, `extract` or `summarise` task never starts at tier 1; `tier-drop plan` reports the skip with its detail, and you do not need to record attempts for it.

## Tier 2: `specialised`

What it is for: small, fast models trained for exactly the job, such as document layout parsing, OCR, or compressing text to a target length.

Indicative models as of 2026-10-19: ScaleDown models, Docling, Qwen OCR models.

A tier-2 model must list the families it serves (`families` non-empty). It is a candidate only for those families.

What disqualifies it: floor failures on evidence, as for every tier. A layout parser whose `completeness` on tables falls below the floor is a typical case.

Structural skip: `no_provisioned_candidates` when no tier-2 model is bound to the task's family, or none lists the task's shape.

## Tier 3: `cheap_generalist`

What it is for: generalists that are inexpensive and close to the frontier on the job. Prefer deployments on your own hardware (`locality: "local"`).

Indicative models as of 2026-10-19: DeepSeek 4.1 Flash, Qwen 3.8 27B, GLM-5.3-flash.

What disqualifies it: floor failures on evidence across every provisioned model and reasoning level, and then across the compositions the plan offers (panel with a tier-4 judge, cascade with a tier-4 auditor, draft with a tier-4 finisher).

Structural skips: `no_provisioned_candidates`, or `no_qos_qualified_candidates` when every deployment fails the QoS gate.

## Tier 4: `frontier`

What it is for: work the cheaper tiers cannot do. Try the best open-weight models first, then closed models. Order inside the tier comes from cost, so make the registry costs accurate.

Indicative models as of 2026-10-19: GLM-5.3, Qwen 3.8 Max, Mistral 4, Kimi K3 (open weight), then Opus 5.5 and GPT-6.1-Sol.

What disqualifies it: floor failures on evidence. There is no tier above the default ladder's top, so tier 4 has no compositions with a next-tier judge. When tier 4 is disqualified and no tailored model follows, the plan is `exhausted` and the decision's `winner` is null.

Structural skips: as tier 3. A task with `send_remote: false` loses every remote deployment here (`rights_forbid_remote`); if only remote frontier models are provisioned, the tier is skipped with `no_qos_qualified_candidates`.

## Tier 5: `tailored`

What it is for: a model you fine-tuned, pruned (for example with REAP), trained with reinforcement learning, or otherwise modified for one high-volume family that tiers 1 to 4 already satisfy. It exists to do that family's work more cheaply or faster.

No model names are indicative for this tier; each tailored model is your own artefact, named in your registry.

A tier-5 model must list its `families`. It is a candidate only for them. `references/dataset.md` describes when a family is worth a tailored model.

What disqualifies it: the same evidence rule. A tailored model that drifts below the floors on new held-out items is disqualified, and the ladder continues at tier 1.

## Structural skip reasons

| Reason | When `tier-drop plan` reports it |
|---|---|
| `shape_mismatch` | tier 1 and a non-decision task shape |
| `no_provisioned_candidates` | no model in the tier serves the family and shape (always the case for tier 2 or tier 5 with no bound model) |
| `no_qos_qualified_candidates` | every model in the tier is removed by the QoS gate |
| `rights_forbid_remote` | remote deployments removed because `send_remote` is false |

`floors_not_met` and `operator` are the evidence reasons. They are recorded as `Disqualification` records in `disqualifications.jsonl` and are what normally moves a task up the ladder.
