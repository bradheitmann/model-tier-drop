# model-tier-drop

Tiered model routing with evidence-based disqualification. A harness-neutral skill, a Rust engine and CLI, a hosted MCP server and a dsh adapter.

This README is completed in slice S-09. The plan is in [`plan/`](plan/).

Hosted MCP (live since 2026-10-19): `https://model-tier-drop-mcp.bradheitmann.ai/mcp`. It also serves the skill at `https://model-tier-drop-mcp.bradheitmann.ai/skill/SKILL.md`. See [`crates/worker/README.md`](crates/worker/README.md).

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
