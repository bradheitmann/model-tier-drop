# model-tier-drop

Tiered model routing with evidence-based disqualification. Every model-using task starts at the cheapest tier and moves up only after that tier has been disqualified for the task's activity on recorded evidence. The repository holds a harness-neutral skill that tells an agent how to do that, a Rust engine and command line tool that plan, check and record, a hosted MCP server that serves the skill and the engine's pure operations, and an adapter for the dsh harness.

Hosted MCP: `https://model-tier-drop-mcp.bradheitmann.ai/mcp` (Streamable HTTP, JSON responses, stateless, no account needed). The same server serves the skill at `https://model-tier-drop-mcp.bradheitmann.ai/skill/SKILL.md`.

## The rule

| Tier | Compute category | Used when |
|---|---|---|
| 1 `system_one` | decision-shaped models | the task is a decision: classify, choose, route, yes/no |
| 2 `specialised` | small models trained for exactly one job | a model is bound to the task's family |
| 3 `cheap_generalist` | inexpensive generalists, preferably on your own equipment | the task needs general ability |
| 4 `frontier` | frontier models, open-weight first, then closed | nothing cheaper clears the floors |
| 5 `tailored` | your own fine-tuned, pruned or RL-trained models, one per high-volume stable family | a tailored model is bound to the family (then it is tried first) |

Embedding models are helpers for retrieval, deduplication and clustering; they are never a routing tier. Quality of service (availability, latency, error rate) is a cutoff gate applied before any scoring. Among candidates that clear the gate, routing ranks by quality, accuracy and completeness first, then cost per task, then speed. Quality floors are written into the task before any result is seen. A tier is left only when a disqualification for the task's family is recorded on evidence: every provisioned model and reasoning level in the tier, and the panel, cascade and draft compositions it allows, must have been tried. Model names are configuration (`registry/registry.example.json`, indicative as of 2026-10-19), never code. Review is strict by default.

Every task leaves records behind, as append-only JSON lines with a per-record hash: the routing decision (tiers tried, why each was skipped or disqualified, the winner and its evidence), every attempt including failures and abstentions (each carrying the data's rights for process, train, send_remote and export), disqualifications, held-out splits by source family and eval-run summaries. Those records are the eval harness and the dataset from which a tailored model can later be built.

## What is here

| Path | What it is |
|---|---|
| [`skill/`](skill/) | `SKILL.md` and six references: the procedure an agent follows, in any harness |
| [`crates/core/`](crates/core/) | `tier-drop-core`: typed registry, QoS gate, ladder, plans, floors, disqualification, decisions, held-out splits, dataset rights. Pure: no I/O, no clock, no model calls; builds for `wasm32` |
| [`crates/cli/`](crates/cli/) | `tier-drop`: the command line over the core, with a local JSONL ledger |
| [`crates/worker/`](crates/worker/) | the MCP server, a Cloudflare Worker in Rust (workers-rs) |
| [`adapters/dsh/`](adapters/dsh/) | the dsh plugin: five local tools over `tier-drop` plus the MCP mount; [`adapters/README.md`](adapters/README.md) explains how a Pi adapter would differ |
| [`registry/`](registry/) | the example provisioned-model registry and the default policy |
| [`plan/`](plan/) | the REVENG-shaped plan: commission, requirements with acceptance tests, data contracts, paired slice contracts, the typed graph and its compiled build |

## Install the command line tool

Requires Rust (the toolchain is pinned in `rust-toolchain.toml`; `rustup` installs it on first use).

```sh
git clone https://github.com/bradheitmann/model-tier-drop
cd model-tier-drop
cargo install --path crates/cli
tier-drop schemas routing_decision | head -5
```

## Walkthrough

Install the CLI from a clone of this repository with `cargo install --path crates/cli`, then run the commands below in an empty directory. They route one `classify` task: copy the example registry, write the task with its floors before any result, ask for a plan, record two attempts (one answered, one abstained), decide, verify the ledger and export the rows that may be used for training. The integration test `crates/cli/tests/cli.rs` runs this block exactly as written.

The task sets floors for `quality`, `accuracy` and `completeness` (the policy's required metrics), forbids sending the input to remote deployments, asks for strict review and states the QoS it accepts:

```sh
tier-drop registry example > registry.json
tier-drop registry validate registry.json
cat > task.json <<'EOF'
{
  "task_id": "ticket-4711",
  "family": "ticket.priority",
  "shape": "classify",
  "input_sha256": "6708cfa2dde6152d5035c74ab58c9e623324ec1abf2435431c2e4296a945e4ef",
  "rights": { "process": true, "train": true, "send_remote": false, "export": true },
  "floors": [
    { "metric": "quality", "min": 0.8 },
    { "metric": "accuracy", "min": 0.9 },
    { "metric": "completeness", "min": 0.9 }
  ],
  "qos": { "availability_min": 0.99, "p95_latency_max_ms": 5000, "error_rate_max": 0.02 },
  "review": "strict",
  "metadata": {},
  "created_at": "2026-10-19T09:00:00Z"
}
EOF
tier-drop plan --task task.json --registry registry.json --ledger ledger
cat > attempt-1.json <<'EOF'
{
  "task_id": "ticket-4711",
  "family": "ticket.priority",
  "tier": "system_one",
  "candidate": { "kind": "single", "model_id": "jev", "reasoning_level": "none" },
  "outcome": "answered",
  "metrics": { "quality": 0.92, "accuracy": 1.0, "completeness": 1.0 },
  "judge": { "kind": "deterministic", "ref": "label-exact-match" },
  "cost_usd": 0.0,
  "latency_ms": 42,
  "tokens": { "input": 180, "output": 3 },
  "input_sha256": "6708cfa2dde6152d5035c74ab58c9e623324ec1abf2435431c2e4296a945e4ef",
  "output_sha256": "3946ca64ff78d93ca61090a437cbb6b3d2ca0d488f5f9ccf3059608368b27693",
  "rights": { "process": true, "train": true, "send_remote": false, "export": true },
  "eval_item_id": null
}
EOF
tier-drop attempt record --ledger ledger --task task.json --registry registry.json < attempt-1.json
cat > attempt-2.json <<'EOF'
{
  "task_id": "ticket-4711",
  "family": "ticket.priority",
  "tier": "system_one",
  "candidate": { "kind": "single", "model_id": "laya", "reasoning_level": "none" },
  "outcome": "abstained",
  "metrics": {},
  "judge": { "kind": "deterministic", "ref": "label-exact-match" },
  "cost_usd": 0.0,
  "latency_ms": 55,
  "tokens": { "input": 180, "output": 0 },
  "input_sha256": "6708cfa2dde6152d5035c74ab58c9e623324ec1abf2435431c2e4296a945e4ef",
  "output_sha256": null,
  "rights": { "process": true, "train": true, "send_remote": false, "export": true },
  "eval_item_id": null
}
EOF
tier-drop attempt record --ledger ledger --task task.json --registry registry.json < attempt-2.json  # exits 1: an abstention fails the floors and is still recorded
tier-drop decide --task task.json --registry registry.json --ledger ledger
tier-drop ledger verify --ledger ledger
tier-drop dataset export --ledger ledger --purpose train
```

What each step shows:

- `plan` answers `"kind": "try"` in tier `system_one` with the two local decision models as single candidates.
- `attempt record` fills `attempt_id` (a ULID) and `recorded_at` (now) when the attempt omits them, refuses an attempt recorded before the task's `created_at`, appends the task to `ledger/tasks.jsonl` the first time it sees it, appends the attempt to `ledger/attempts.jsonl` and prints `{ "floor_result": …, "disqualification": … }`. The abstained attempt fails the floors, so that command exits 1 (in a script run with `set -e`, put `|| true` after it); it is recorded all the same, because failures and abstentions are dataset rows too.
- `decide` appends the routing decision to `ledger/decisions.jsonl` and prints it; its `winner` is the `jev` attempt.
- `ledger verify` recomputes the `record_sha256` of every line of the six ledger files and exits 1 naming `file:line` for any line that was altered.
- `dataset export --purpose train` prints, one JSON line each, the attempts whose recorded rights allow training.

Exit codes: 0 success; 1 validation or floor failure (the JSON on stdout still describes it); 2 usage error.

The full command list is in [`plan/technology-decisions.md`](plan/technology-decisions.md) under "CLI surface": `registry validate|example`, `policy default`, `plan`, `qos`, `attempt record`, `decide`, `disqualify`, `ledger verify|show`, `eval split|summarise`, `dataset export`, `schemas`.

## Hosted MCP

`POST https://model-tier-drop-mcp.bradheitmann.ai/mcp` speaks MCP over Streamable HTTP with JSON responses (handshake versions 2025-03-26, 2025-06-18, 2025-11-25 and the stateless `_meta` form 2026-07-28). It stores nothing and needs no key. Twelve tools: `skill_read`, `registry_example`, `registry_validate`, `qos_gate`, `plan_next`, `check_floors`, `validate_attempt`, `assess_disqualification`, `decide`, `split_holdout`, `summarise_eval`, `schemas`. Ledger writes stay local, through `tier-drop` or the dsh adapter.

```sh
curl -s -H 'Content-Type: application/json' -H 'Accept: application/json' \
  https://model-tier-drop-mcp.bradheitmann.ai/mcp \
  -d '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25"}}' | jq .result.serverInfo
```

Add it to a harness:

- Claude Code: `claude mcp add --transport http tier-drop https://model-tier-drop-mcp.bradheitmann.ai/mcp`
- Codex: in `~/.codex/config.toml`, `[mcp_servers.tier-drop]` with `url = "https://model-tier-drop-mcp.bradheitmann.ai/mcp"`
- dsh: install the adapter below, or add a `@deepseek-ai/dsh-mcp-client` row with `transport: streamable-http` and that URL (see [`adapters/dsh/cordis.patch.yml`](adapters/dsh/cordis.patch.yml))
- Pi: an `mcp-remote` entry in `~/.pi/mcp.json` (`npx -y mcp-remote https://model-tier-drop-mcp.bradheitmann.ai/mcp`)

The server's source, build, local run, deploy and rollback steps, and the live verification transcript are in [`crates/worker/README.md`](crates/worker/README.md).

## The skill

Symlink `skill/` as `model-tier-drop` where your harness reads skills: `~/.claude/skills/` (Claude Code), `~/.agents/skills/` (Codex, also read by dsh), `~/.dsh/skills/` (dsh), `~/.pi/agent/skills/` (Pi). For example `ln -s "$PWD/skill" ~/.agents/skills/model-tier-drop`. Agents without a skill directory can fetch `GET /skill/SKILL.md` from the hosted server. The skill names only `tier-drop` commands, the MCP tools and the record formats; harness-specific instructions are confined to its Install section.

## dsh adapter

The adapter targets dsh `0.1.6-alpha.2` (Node 24 or newer, pnpm). Newer dsh releases refuse it as incompatible until its pins are updated, so pin the version in every `npx` call:

```sh
pnpm --dir adapters/dsh install && pnpm --dir adapters/dsh build
npx -y @deepseek-ai/dsh@0.1.6-alpha.2 --profile <profile> --from-default-profile headless --dump-config > /dev/null   # creates the profile if it does not exist
npx -y @deepseek-ai/dsh@0.1.6-alpha.2 plugin --profile <profile> add "$PWD/adapters/dsh"
mkdir -p ~/.dsh/skills && ln -s "$PWD/skill" ~/.dsh/skills/model-tier-drop
```

`plugin add` applies the package's bundle patch, which mounts the five local tools (`tier_drop_plan`, `tier_drop_qos`, `tier_drop_record_attempt`, `tier_drop_decide`, `tier_drop_ledger_show`) and the hosted MCP (`mcp__tier-drop__*`). The ledger directory, registry and policy paths come from the profile's configuration, never from tool arguments. [`adapters/dsh/README.md`](adapters/dsh/README.md) has the configuration rows, the trust boundary and the verification record; [`adapters/README.md`](adapters/README.md) describes the Pi route (an extension that calls `tier-drop`, `mcp-remote` for the server, a skill symlink), which is described and not built.

## Eval harness and datasets

`tier-drop eval split --items items.json` assigns whole source families to the held-out split deterministically (`sha256(seed || source_family)`, first eight bytes as a fraction), so no family leaks between train and holdout. Eval attempts are ordinary attempts with `eval_item_id` set; recording them through `attempt record` is what accumulates disqualification evidence per family and tier. `eval summarise` writes an `EvalRun` (mean metric vector, pass rate, n) for a family, tier and candidate. `dataset export --purpose train|process|send_remote|export` emits only the attempts whose recorded rights allow that purpose. A family becomes a candidate for a tier-5 model when it is high-volume, stable, has a measured pass rate on held-out families and its rows carry training rights. The details are in [`skill/references/eval-harness.md`](skill/references/eval-harness.md) and [`skill/references/dataset.md`](skill/references/dataset.md).

## Configuration

`registry/registry.example.json` lists deployments (with measured quality-of-service values), models by tier (open-weight flag, reasoning levels, shapes, bound families, cost, speed) and embedding helpers. Copy it and edit it for your own equipment; `tier-drop registry validate` checks it. `registry/policy.default.json` holds the routing policy: `tailored_first`, `prefer_open_weight`, `prefer_local`, the required floors, the ranking bands, the disqualification threshold with `require_full_coverage`, panel and cascade sizes, and the held-out seed and fraction. `tier-drop schemas` prints JSON Schema for every record so other languages can validate the same files.

## Development

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo fmt --all --check --manifest-path crates/worker/Cargo.toml
cargo clippy --manifest-path crates/worker/Cargo.toml --target wasm32-unknown-unknown -- -D warnings
(cd crates/worker && cargo test && worker-build --release)        # cargo install worker-build --locked
cargo build -p tier-drop-cli && pnpm --dir adapters/dsh install && pnpm --dir adapters/dsh check
gitleaks detect --redact --source .
```

CI (`.github/workflows/ci.yml`) runs the same checks on every push and pull request; gitleaks runs there through its GitHub action. The adapter needs Node 24 or newer and pnpm. Rules for contributors and agents are in [`AGENTS.md`](AGENTS.md). The plan under `plan/` is the contract the code was built against; change it before changing what it binds.

## Status

Version 0.1.0 (2026-10-19). Built and verified: the engine (70 tests), the CLI (12 tests, the walkthrough above runs as an integration test), the Worker (22 host tests, live on the URL above), the dsh adapter (20 tests, composed and booted in an isolated dsh profile), the plan graph (compiles deterministically with the REVENG core). Not built: a Pi adapter (described only), a shared remote ledger, model execution inside the engine, automatic quality-of-service measurement (values are measured by the operator or harness and written into the registry).

## Licence

Apache-2.0. See [`LICENSE`](LICENSE).
