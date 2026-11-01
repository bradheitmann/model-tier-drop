# Done list — slice-zero (written BEFORE dispatch; does not grow during it)
Base bytes: main aa4765c plus branch charter-init (charter/ and crates/cli/tests/slice_zero.rs).
Proofs (each: name, what it guarantees, reopen step, fail-without mutant):
1. slice-zero-live: `cargo test -p tier-drop-cli --test slice_zero -- --ignored --nocapture` passes. The README walkthrough task, planned by the local CLI over the example registry and by the live hosted MCP's plan_next over the same task and registry, gives identical plans (G2). Reopen: rerun the command from a fresh clone of the frozen bytes and compare the printed `slice-zero ok:` line (tier and candidate count) with the recorded one. Fail-without mutant: reverse the registry-order tie-break in `crates/core/src/plan.rs` (`index(a).cmp(&index(b))` to `index(b).cmp(&index(a))`); the test fails with "local CLI and live MCP disagree" (observed 2026-10-31). Note: changing registry data alone is not a mutant, because the same registry is sent to both sides (observed: the test stays green).
2. ladder-evidence: `cargo test -p tier-drop-core plan_exhausted_tier_waits_for_disqualification_instead_of_moving_up` and `cargo test -p tier-drop-core disqualify_coverage_all_candidates_tried_and_threshold_met_yields_some` pass (G1). Reopen: rerun both on the frozen bytes; same pass counts. Fail-without mutant: make plan_next move to the next tier when a tier is exhausted without a disqualification; the first test fails.
3. sealed-records: `cargo test -p tier-drop-cli --test cli ledger_verify_names_the_tampered_attempt_line` and `cargo test -p tier-drop-core dataset_filter_train_omits_attempt_without_train_right` pass (G3). Reopen: rerun both on the frozen bytes; same pass counts. Fail-without mutant: drop the `rights.train` check in dataset_filter; the second test fails.
Gates (exact commands):
- `cargo fmt --all --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace`
- `charter check --strict`
- `gitleaks protect --staged --redact`
Out of scope (recorded obligations, not fixed here):
- The adjacent-escalation seam (`adjacent()` in core, MCP/CLI exposure); this needs a plan/ contract amendment first.
- Adding the charter check to CI and installing the charter hooks (adoption steps 3 and 5) wait on PM-CHARTER's install recipe; until then, `charter check` is run by hand at each landing.
Owner decision points:
- Ratify the intent card (INTENT.md is marked pending-owner).
