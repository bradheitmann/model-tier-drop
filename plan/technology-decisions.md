# Architecture, technology decisions and data contracts (REVENG P09)

## Reference architecture

```
                 ┌────────────────────────────┐
  agent in any   │  skill/SKILL.md + references│  (text; harness-neutral procedure)
  harness ─────▶ └────────────┬───────────────┘
                              │ follows
          ┌───────────────────┼─────────────────────────┐
          ▼                   ▼                         ▼
  tier-drop CLI        hosted MCP (Worker)         dsh adapter
  crates/cli           crates/worker               adapters/dsh
  local ledger I/O     stateless tools + skill     native tools → CLI
          │                   │                         │
          └──────────┬────────┴─────────────────────────┘
                     ▼
            tier-drop-core (Rust library, no I/O, no model calls)
            registry · QoS gate · ladder · plan_next · floors ·
            disqualification · decide · holdout split · eval · rights
```

The core is a pure library (serde types and functions over them). The CLI adds files (JSONL ledger). The Worker adds HTTP and embeds the skill text. The dsh adapter adds a Cordis plugin that shells out to the CLI and a patch row that mounts the hosted MCP. Harnesses execute model calls; the engine plans and records.

Alternatives considered and rejected: (a) the engine calling models directly, rejected because it would tie the engine to providers and break the WASM build and the fleet rule; (b) a TypeScript Worker calling a wasm-pack build of the core, rejected because workers-rs 0.8.7 lets one Rust crate serve the whole MCP without a generated JavaScript glue layer we would have to maintain; (c) a D1-backed shared ledger in the Worker, rejected for this release because it adds secrets, auth and a second state authority (recorded as deferred).

## Technology decisions

| Decision | Choice | Evidence (2026-10-19) | Exit |
|---|---|---|---|
| Engine language | Rust 2021, toolchain pinned 1.98.1 (MSRV 1.91), `serde`, `serde_json`, `sha2`, `schemars` 1.x | Owner rule: Rust default. Toolchain present. | None needed. |
| CLI | `clap` derive, JSON on stdin/stdout, JSONL ledger | Harness-neutral interface; dsh and Pi adapters shell out to it. | Any harness can call the MCP instead. |
| Worker | `worker` 0.8.7 (workers-rs), `worker-build` 0.8.7, wrangler 4.98.0, `compatibility_date: 2026-10-16` | crates.io check; reference deployment pattern (`observability`, custom domain, stateless MCP, `ttlMs`/`cacheScope`). Generated `build/worker/shim.mjs` is build output, git-ignored, not authored JavaScript. | Strict TypeScript Worker with the same handlers if workers-rs fails on the host. |
| MCP protocol | JSON-RPC 2.0 over POST `/mcp`, JSON responses, handshake versions 2025-03-26/2025-06-18/2025-11-25 and stateless `_meta` 2026-07-28 | Copied from the reference Worker which Claude Code, Codex, dsh and Pi (via `mcp-remote`) already consume. | None. |
| dsh adapter | Cordis 4.0.2 plugin, strict TS, ES modules, Node 24+, pnpm; tools via `defineTool` from `@deepseek-ai/dsh-tools` 0.1.6-alpha.2; MCP mount via `@deepseek-ai/dsh-mcp-client` `streamable-http` | Inspected installed dsh packages and the local `dsh-jev` plugin. | The MCP mount alone works with no native code. |
| Skill format | Directory bundle `SKILL.md` + `references/`, YAML frontmatter `name`, `description`, `whenToUse`, `metadata` | `dsh-skill-filesystem` README; same shape Claude Code and Pi read. | None. |
| Licence | Apache-2.0 | Commission default; sibling repos are mixed. | None. |
| Tests | `cargo test` (unit + CLI integration via `assert_cmd`-free std::process), `node --test` with Node's native TypeScript execution for the adapter | Keeps the adapter free of a test framework dependency. | vitest if Node type stripping proves insufficient. |

## Data contracts (normative for every slice)

All records are JSON. Field names are `snake_case`. Enumerations are lower-case strings. Timestamps are RFC 3339 UTC. Hashes are lower-case hex SHA-256. Serde: `#[serde(deny_unknown_fields)]` on every record; enums `rename_all = "snake_case"`; tagged unions use `tag = "kind"`.

### Tier

`"tailored" | "system_one" | "specialised" | "cheap_generalist" | "frontier"` with ordinals 5, 1, 2, 3, 4. `Tier::ladder_default()` is `[system_one, specialised, cheap_generalist, frontier]`; `tailored` is inserted at the front by `ladder(task)` when the registry binds a tailored model to the task's family and `policy.tailored_first` is true; when a tailored model is bound but `tailored_first` is false it is appended at the end as a real tier; when none is bound it is not on the ladder (plans and decisions report it as a trailing `skipped` entry with `no_provisioned_candidates`).

### TaskShape

`classify | choose | route | yes_no | extract | transform | generate | review | summarise | code | other`. Decision shapes: `classify, choose, route, yes_no`. A tier-1 model may list only decision shapes.

### Rights

```
{ "process": bool, "train": bool, "send_remote": bool, "export": bool }
```

### Registry (`registry/registry.example.json`)

```
Registry {
  schema_version: "tier-drop.registry/1",
  as_of: "YYYY-MM-DD",                       // date the model names were indicative
  deployments: [Deployment],
  models: [Model],
  embeddings: [EmbeddingModel],
  notes: string | null                       // optional free text (the example says its values are not measurements)
}
Deployment { id, provider, locality: "local" | "remote", qos: QosObserved | null }
QosObserved { availability: f64, p95_latency_ms: u64, error_rate: f64, window_started: date, window_ended: date, sample_count: u64 }
Model {
  id, display_name, tier: Tier, deployment_id, open_weight: bool,
  reasoning_levels: [string],                // ["none"] when the model has no levels
  shapes: [TaskShape],
  families: [string],                        // empty = any family; required non-empty for tier 2 and tier 5
  cost: { input_usd_per_mtok: f64, output_usd_per_mtok: f64 },
  speed: { output_tokens_per_second: f64 | null, median_latency_ms: u64 | null },
  context_window: u64,
  notes: string
}
EmbeddingModel { id, deployment_id, dimensions: u64, notes }
```

`validate_registry` rejects: duplicate ids; unknown `deployment_id`; tier-1 model with a non-decision shape; tier-2 or tier-5 model with empty `families`; empty `reasoning_levels`; negative cost; an embedding model id reused as a model id.

### Policy (`policy.default.json`, embedded in the core as the default)

```
Policy {
  schema_version: "tier-drop.policy/1",
  tailored_first: true,
  prefer_open_weight: true,
  prefer_local: true,
  required_floors: ["quality", "accuracy", "completeness"],
  quality_band: 0.02,
  cost_band: 0.10,
  disqualify: { min_attempts: 5, max_pass_rate: 0.5, require_full_coverage: true },
  panel: { max_members: 3 },
  cascade: { audit_sample_rate: 0.1 },
  allow_unmeasured_qos: false,
  holdout: { fraction: 0.2, seed: 20261019 }
}
```

### Task

```
Floor { metric: string, min: f64 }

Task {
  schema_version: "tier-drop.task/1",            // default when omitted on input
  task_id, family, shape: TaskShape, input_sha256,
  rights: Rights,
  floors: [Floor],                               // must include every policy.required_floors metric
  qos: { availability_min: f64, p95_latency_max_ms: u64, error_rate_max: f64 },
  review: "strict" | { "sampled": { "class": string, "decision_ref": string } },
  metadata: { [string]: string },                // "financial": "true" or "regulated": "true" forbid sampled review; "numeric": "true" with shape extract too
  created_at,
  record_sha256                                  // filled by the CLI when the task is appended to tasks.jsonl
}
```

### Candidate (tagged union, `kind`)

```
{ kind: "single",  model_id, reasoning_level }
{ kind: "panel",   members: [ {model_id, reasoning_level} ], judge: {model_id, reasoning_level} }
{ kind: "cascade", first: {model_id, reasoning_level}, auditor: {model_id, reasoning_level}, audit_sample_rate: f64 }
{ kind: "draft",   drafter: {model_id, reasoning_level}, finisher: {model_id, reasoning_level} }
```

`candidate.tier(registry)` is the tier of `model_id` / the members / `first` / `drafter`: the tier under test. The judge, auditor and finisher come from the next tier up.

### Attempt

```
Attempt {
  schema_version: "tier-drop.attempt/1",
  attempt_id, task_id, family, tier: Tier, candidate: Candidate,
  outcome: "answered" | "abstained" | "failed" | "timed_out",
  metrics: { [metric]: f64 },
  judge: { kind: "deterministic" | "model" | "human", ref: string },
  cost_usd: f64, latency_ms: u64, tokens: { input: u64, output: u64 },
  input_sha256, output_sha256: string | null,
  rights: Rights,
  eval_item_id: string | null,
  recorded_at,
  record_sha256                                   // over canonical JSON of the record without this field
}
```

### FloorResult

```
{ attempt_id, passed: bool, failures: [ { metric, min, observed: f64 | null } ] }
```

### Disqualification

```
Disqualification {
  schema_version: "tier-drop.disqualification/1",
  disqualification_id, family, tier: Tier,
  reason: "shape_mismatch" | "no_provisioned_candidates" | "no_qos_qualified_candidates"
        | "rights_forbid_remote" | "floors_not_met" | "operator",
  evidence: [string],                             // attempt ids or eval run ids
  detail: string,
  registry_as_of: date,
  decided_at,
  expires_at: timestamp | null,
  record_sha256
}
```

A disqualification is **in force** for a task when `family` matches and `registry_as_of >= registry.as_of` (string comparison of `YYYY-MM-DD`). The engine has no clock, so it honours any `expires_at`; the CLI drops records whose `expires_at` is earlier than its clock before calling the engine, and MCP callers pass only the records they consider current. The engine writes `floors_not_met` records only; structural reasons (`shape_mismatch`, `no_provisioned_candidates`, `no_qos_qualified_candidates`, `disqualified`) appear as plan skips and in the routing decision's `tiers[]`. `rights_forbid_remote` is an exclusion reason inside the QoS gate detail, and a valid `reason` for operator-authored records. `floors_not_met` requires, with `policy.disqualify.require_full_coverage` true, that every candidate the tier offers for the task has at least one attempt in the family, then `n >= min_attempts` and `pass_rate <= max_pass_rate`. Ids are deterministic: `disqualification_id = "dq-" + first 16 hex of sha256(family|tier|sorted attempt ids)`; `decided_at` = latest `recorded_at` among the evidence attempts.

### Plan (output of `plan_next`)

```
{ kind: "try",       tier: Tier, candidates: [Candidate], skipped: [ { tier, reason, detail } ], detail: string }
{ kind: "exhausted", skipped: [ { tier, reason, detail } ] }
```

Skip reasons: `shape_mismatch`, `no_provisioned_candidates`, `no_qos_qualified_candidates` (the QoS and rights exclusions are listed in `detail`), `disqualified` (the in-force record id in `detail`). A `try` with an empty `candidates` list means every candidate of the tier has an attempt and the tier awaits disqualification assessment; the plan never moves up without a Disqualification.

Candidate order inside a tier: untried singles ordered by (open-weight before closed-weight when `policy.prefer_open_weight`, local deployment before remote when `policy.prefer_local`, then cost ascending, then reasoning level order as listed in the registry; cost = input plus output price per million tokens). This is the owner's tier text: open-weight first in tier 4, own equipment preferred in tier 3. Then one panel of the cheapest `panel.max_members` (at least two) QoS-qualified models at their lowest level with the cheapest QoS-qualified next-tier model as judge; then a cascade (cheapest single, next-tier auditor, `cascade.audit_sample_rate`); then a draft (cheapest single, next-tier finisher). The next tier up is fixed: system_one→specialised→cheap_generalist→frontier, tailored→frontier, frontier has none (so frontier gets no compositions). Helpers must pass the QoS gate and fit the family. Compositions are offered only after every single has an attempt and are omitted when no next-tier model is QoS-qualified.

### RoutingDecision

```
RoutingDecision {
  schema_version: "tier-drop.routing_decision/1",
  decision_id, task_id, family, ladder: [Tier],
  tiers: [ { tier, status: "disqualified" | "skipped" | "tried" | "selected", refs: [string], detail } ],
  winner: { candidate, attempt_id, floor_result: FloorResult, score: { quality: f64, cost_usd: f64, latency_ms: u64 } } | null,
  ranking: [ { candidate, attempt_id, quality, cost_usd, latency_ms } ],
  review: Task.review,
  policy_sha256,
  decided_at,
  record_sha256
}
```

`quality` = mean of the attempt's metrics named in `policy.required_floors` (a missing metric counts as 0). `ranking` holds every qualified attempt of the task across tiers, grouped by ladder tier lowest first; inside a tier: sort by quality descending; candidates within `quality_band` of the best form a band ordered by `cost_usd` ascending; within `cost_band` (relative) of the cheapest, order by `latency_ms` ascending. Only attempts with `outcome: answered` and `floor_result.passed` are ranked. The winner is the first entry of the lowest ladder tier that has a qualified attempt; a tier with an in-force disqualification never supplies the winner. `tiers[]` lists every ladder tier (and `tailored` as a trailing `skipped` entry when unbound) with status `disqualified` (refs = record ids), `skipped` (detail = structural reason, or `not_tried` for an open tier above the winner with no attempts), `tried` (refs = attempt ids) or `selected` (ref = the winning attempt). Deterministic, no clock: `decision_id = "rd-" + first 16 hex of sha256(task_id|sorted attempt ids)`; `decided_at` = latest `recorded_at` among the task's attempts, or `task.created_at` when none; `policy_sha256 = record_sha256(policy)`. Re-running `decide` without new attempts yields an identical record; ledger order shows the latest.

### Eval records

```
EvalItem  { item_id, family, source_family, input_sha256, expected_sha256: string | null, oracle: "exact" | "judge" | "human" }
EvalSplit { schema_version: "tier-drop.eval_split/1", family, seed: u64, fraction: f64,
            holdout_source_families: [string], train_source_families: [string],
            items: [ { item_id, split: "holdout" | "train" } ], record_sha256 }
EvalRun   { schema_version: "tier-drop.eval_run/1", eval_run_id, family, tier, candidate, attempt_ids: [string], n: u64,
            aggregate: { [metric]: f64 }, pass_rate: f64, floors: [Floor], recorded_at, record_sha256 }
```

`split_holdout`: for each distinct `source_family`, compute `sha256(seed_be_bytes || source_family)`; take the first 8 bytes as a big-endian u64; the family is held out when `(value as f64 / u64::MAX as f64) < fraction`. Deterministic, whole families, independent of item order.

### Ledger layout

```
<ledger_dir>/
  tasks.jsonl
  attempts.jsonl
  decisions.jsonl
  disqualifications.jsonl
  eval-runs.jsonl
  eval-splits.jsonl
```

One canonical JSON record per line. Canonical JSON: keys sorted, no whitespace, numbers as serde_json prints them, `record_sha256` excluded from the hashed bytes. `ledger verify` recomputes every hash and reports `file:line` on mismatch.

### Canonical JSON, ids and versions

`canonical_json(value)` sorts object keys recursively and prints compact JSON. Task and attempt ids are caller-supplied strings; the CLI generates `ulid`-shaped ids (`crockford base32`, 26 chars) when absent. Disqualification, decision and eval-run ids are derived hashes (above). Every ledger record (`Task`, `Attempt`, `Disqualification`, `RoutingDecision`, `EvalRun`, `EvalSplit`) carries `schema_version` (`tier-drop.task/1`, `tier-drop.attempt/1`, `tier-drop.disqualification/1`, `tier-drop.routing_decision/1`, `tier-drop.eval_run/1`, `tier-drop.eval_split/1`); it may be omitted on input and is filled with the default, and it is part of the hashed bytes. `record_sha256` may be omitted on input; the engine and CLI fill it.

## CLI surface (`tier-drop`)

```
tier-drop registry validate <registry.json>
tier-drop registry example                        # prints registry/registry.example.json
tier-drop policy default                          # prints the embedded default policy
tier-drop plan     --task t.json --registry r.json [--ledger DIR] [--policy p.json]
tier-drop qos      --task t.json --registry r.json [--policy p.json]
tier-drop attempt record --ledger DIR --task t.json --registry r.json [--policy p.json] < attempt.json
                   # any outcome; validates, fills schema_version and record_sha256, appends, prints FloorResult;
                   # appends a Disqualification when coverage and the policy threshold are met; appends the task to tasks.jsonl when new
tier-drop decide   --task t.json --registry r.json --ledger DIR [--policy p.json]      # appends and prints RoutingDecision; appends the task when new
tier-drop disqualify --ledger DIR --registry r.json --family F --tier T --reason REASON --evidence id[,id…] --detail "…" [--expires-at RFC3339]
                   # operator-authored record; REASON is any Disqualification reason; registry_as_of from the registry, decided_at from the clock
                   # plan and decide drop records whose expires_at is earlier than the CLI clock before calling the engine
tier-drop ledger verify --ledger DIR
tier-drop ledger show   --ledger DIR [--family F] [--kind attempts|decisions|disqualifications|eval-runs|eval-splits|tasks]
tier-drop eval split    --items items.json [--policy p.json] [--ledger DIR]   # prints the EvalSplit; appends it to eval-splits.jsonl when --ledger is given
tier-drop eval summarise --ledger DIR --family F --tier T --candidate c.json [--task t.json]
tier-drop dataset export --ledger DIR --purpose process|train|send_remote|export [--family F]
tier-drop schemas [NAME]                          # all, or one of: registry policy task attempt disqualification routing_decision eval_item eval_split eval_run plan
```

Exit codes: 0 ok; 1 validation or floor failure (the JSON output still describes it); 2 usage error.

## MCP tool surface (Worker)

| Tool | Input | Output (`structuredContent`) |
|---|---|---|
| `skill_read` | `{ path?: "SKILL.md" \| "references/<name>.md" }` | `{ path, text }`; no path lists the available paths |
| `registry_example` | `{}` | the embedded example registry |
| `registry_validate` | `{ registry }` | `{ valid, issues: [Issue] }` (`Issue { code, message }`) |
| `qos_gate` | `{ task, registry, policy? }` | `{ qualified: [model_id], excluded: [ { model_id, deployment_id, reason } ] }` |
| `plan_next` | `{ task, registry, disqualifications?, attempts?, policy? }` | Plan |
| `check_floors` | `{ task, attempt, policy? }` | FloorResult |
| `assess_disqualification` | `{ family, tier, task, registry, attempts, policy? }` (`registry_as_of` is taken from the registry) | `{ disqualification: Disqualification \| null }` |
| `validate_attempt` | `{ task, attempt, registry }` | `{ valid, issues: [Issue] }` |
| `decide` | `{ task, registry, disqualifications?, attempts, policy? }` | RoutingDecision |
| `split_holdout` | `{ items, policy? }` | EvalSplit |
| `summarise_eval` | `{ family, tier, candidate, attempts, floors }` | EvalRun |
| `schemas` | `{ name? }` | `{ schemas: { name: JSON Schema } }` |

`tools/list` result: `{ tools, ttlMs: 3600000, cacheScope: "public" }` (the list never depends on the caller). Errors: `{ ok: false, error: { code, message } }` with `isError: true`.

GET routes: `/` (name, version, mcp URL), `/skill/SKILL.md`, `/skill/references/<name>.md`, `/registry/example.json`, `/schemas/<name>.json`, `/robots.txt`.

## dsh adapter surface

Package `dsh-tier-drop` (private, built with `tsc`). Exports `name = "dsh-tier-drop"`, `inject = ["tools"]`, `Config` (schemastery), `apply(ctx, config)`.

```
Config { binary: string, ledgerDir: string, registryPath: string, policyPath?: string, timeoutMs?: number (default 30000) }
Tools:  tier_drop_plan { task }          → Plan
        tier_drop_qos { task }           → QosGateResult
        tier_drop_record_attempt { task, attempt } → FloorResult (+ disqualification if appended)
        tier_drop_decide { task }        → RoutingDecision
        tier_drop_ledger_show { family?, kind? } → records
```

`cordis.patch.yml` rows: `tier-drop` (native plugin) and `mcp-tier-drop` (`@deepseek-ai/dsh-mcp-client`, `transport: streamable-http`, `url: https://model-tier-drop-mcp.bradheitmann.ai/mcp`, `serverName: tier-drop`).
