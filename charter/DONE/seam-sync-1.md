# Done list — seam-sync-1 (SEAM-SYNC remediations owed by model-tier-drop; written BEFORE dispatch; does not grow during it)
Base bytes: main at dispatch plus the vendored fixture crates/cli/tests/fixtures/model-atlas-export-2026.11.02-3.json. Pin: model-atlas.export/1 2026.11.02-3, sha256 77879250ea316ba3f203bb3c3b3899eb3eef0b42a5304449f6d8616f6447b7e7, producing commit dfc979e73884d76e12bc175e2fbc695d42346666, marked `current` in model-atlas exports/PINS.tsv. Pairs: model-atlas ↔ model-tier-drop (a), charter ↔ model-tier-drop (a) in reports/STATE-OF-PROJECT/PAIRS.md.
Proofs (each: name, what it guarantees, reopen step, fail-without mutant):
1. repin-current: `atlas_fixture_matches_pinned_sha256` passes against the 2026.11.02-3 fixture, and the 2026.11.01-4 fixture is removed with git rm. Every S-13 test still passes. Reopen: `cargo test -p tier-drop-cli --test atlas_export`. Fail-without mutant: edit one fixture byte; the sha test fails.
2. new-route-resolves: `atlas_qwen_route_cost_resolves`: route.openrouter.qwen-qwen3-8-27b resolves for card qwen3.8-27b with its cost_basis and is_free, as in the export. Reopen: rerun. Fail-without mutant: drop route matching by card id; the test fails.
3. charter-rev-current: `.github/workflows/ci.yml` CHARTER_REV = b9323cd119f2ef2fe83db21b8ed8f844215e8852 (the fleet pin the local hooks use), with no other change to the job; `grep -c 7b12eeaf .github/workflows/*.yml` prints 0. Reopen: rerun the grep; diff the job against template v2. Fail-without mutant: restore 7b12eea; the grep prints 1.
Gates (exact commands):
- `cargo fmt --all --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace`
- `actionlint .github/workflows/*.yml`
- `charter check --strict` (pinned binary b9323cd)
- `gitleaks protect --staged --redact`
Out of scope (recorded obligations, not fixed here):
- atlas_ref in registry models: needs a Model field, which the engine (deny_unknown_fields) and the deployed Worker reject until S-11 deploys. Deferred with both shapes: model-atlas card id `qwen3.8-27b` ↔ registry id `qwen-3.8-27b`. Binding flash waits for model-atlas item (e).
- PlanGraph v5 plan shape (O-STD-8): waits for the planctl validator release.
Owner decision points:
- none
