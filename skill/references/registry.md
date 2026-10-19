# Registry

The registry lists what you have provisioned: deployments, models by tier, and embedding helpers. Model names are configuration data in this file, never instructions in the procedure. Copy the example, edit it for your own deployments, and validate it before routing.

```
tier-drop registry example > registry.json     # MCP: registry_example; HTTP: GET /registry/example.json
# edit registry.json
tier-drop registry validate registry.json      # MCP: registry_validate
```

The example lives at `registry/registry.example.json` in the repository. Its models, costs and QoS values are placeholders.

## Top level

```
{ "schema_version": "tier-drop.registry/1",
  "as_of": "YYYY-MM-DD",
  "deployments": [ Deployment ],
  "models": [ Model ],
  "embeddings": [ EmbeddingModel ] }
```

`as_of` is the date your model list was current. Disqualifications bind it as `registry_as_of`; when you raise `as_of` after changing the registry, older disqualifications stop being in force and lower tiers are tried again with the new models.

## Deployment

```
{ "id": "local-mini", "provider": "...", "locality": "local" | "remote", "qos": QosObserved | null }
```

| Field | Meaning |
|---|---|
| `id` | Short id that models refer to. Use ids, never hostnames or account identifiers. |
| `provider` | Who serves it: your own server software or an API vendor. |
| `locality` | `local` for your own hardware, `remote` for anything the input leaves your machines to reach. `send_remote: false` removes every `remote` deployment. |
| `qos` | Observed quality of service, or null when unmeasured. |

### QoS fields are measured values

```
{ "availability": 0.995, "p95_latency_ms": 1800, "error_rate": 0.004,
  "window_started": "2026-10-01", "window_ended": "2026-10-18", "sample_count": 4210 }
```

These are measurements over a stated window, not targets or vendor claims. Compute them from your attempts (`failed` and `timed_out` outcomes, `latency_ms`) or from your monitoring. `sample_count` says how many requests the window covers. A deployment with `qos: null` is excluded by the QoS gate unless `policy.allow_unmeasured_qos` is true. The gate compares these values with the task's `qos` requirement (`availability_min`, `p95_latency_max_ms`, `error_rate_max`) and removes failing deployments before scoring, with a reason per exclusion.

### Adding a deployment

1. Add a `Deployment` with a new `id`, its `provider` and `locality`.
2. Leave `qos` null until you have measured a window, or measure one first.
3. Add or move models to it by setting their `deployment_id`.
4. Raise `as_of` if the change should reopen tiers that were disqualified under the old registry.
5. Run `tier-drop registry validate registry.json`.

## Model

| Field | Meaning |
|---|---|
| `id` | Unique id used in candidates and attempts. |
| `display_name` | Human-readable name. |
| `tier` | `system_one`, `specialised`, `cheap_generalist`, `frontier` or `tailored`. |
| `deployment_id` | The deployment that serves it. Must exist. |
| `open_weight` | True when the weights are open. |
| `reasoning_levels` | Levels the plan tries, in order. `["none"]` when the model has no levels. Never empty. |
| `shapes` | Task shapes it serves. Tier 1 may list only `classify`, `choose`, `route`, `yes_no`. |
| `families` | Families it serves. Empty means any family. Required non-empty for `specialised` and `tailored`. |
| `cost` | `input_usd_per_mtok`, `output_usd_per_mtok`. Used to order candidates. Never negative. |
| `speed` | `output_tokens_per_second`, `median_latency_ms`; either may be null. |
| `context_window` | Tokens. |
| `notes` | Free text, for example "placeholder cost". |

## EmbeddingModel

```
{ "id", "deployment_id", "dimensions", "notes" }
```

Embedding models are helpers, not a tier. They are listed separately and can never be routing candidates. An embedding id reused as a model id is rejected.

## Validation

`validate_registry` rejects: duplicate ids; an unknown `deployment_id`; a tier-1 model with a non-decision shape; a `specialised` or `tailored` model with empty `families`; empty `reasoning_levels`; negative cost; an embedding model id reused as a model id. Fix every issue before routing.

## Indicative model names

The example registry fills the tiers with models indicative as of 2026-10-19. They show what each category looks like; replace them with what you have provisioned.

| Tier | Indicative as of 2026-10-19 |
|---|---|
| `system_one` | Jev, Laya |
| `specialised` | a Docling model, a Qwen OCR model, a ScaleDown model |
| `cheap_generalist` | DeepSeek 4.1 Flash, Qwen 3.8 27B, GLM-5.3-flash |
| `frontier` | GLM-5.3, Qwen 3.8 Max, Mistral 4, Kimi K3, then Opus 5.5, GPT-6.1-Sol |
| `tailored` | one example bound to a family; yours will be your own models |

The example also has two embedding helpers and deployments with ids only. No hostnames and no keys appear in it; keep it that way in your copy if you commit it.
