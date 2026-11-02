# State — model-tier-drop — 2026-11-03T00:00Z, written by PM-MODEL-TIER-DROP (Claude Opus 5.5) at the owner wind-down closeout
Proven (with the proof names):
- seam-sync-1 ACCEPTED (MTD/QA mimo, no defects): the atlas customer test is pinned to model-atlas 2026.11.02-3 (PINS.tsv current, sha 77879250…), with a new route test for qwen3.8-27b; CI CHARTER_REV = b9323cd (fleet pin). SEAM-SYNC pairs: 7/7 peer rows signed (reports/STATE-OF-PROJECT/PAIRS.md).
- s13 ACCEPTED (S-13: re-pin to model-atlas.export/1 2026.11.01-4, sha256 227b0398…, verified against model-atlas 7435bbd): code by MTD/DEV, V-13 by MTD/QA (mimo), no defects. Route costs carry cost_basis and is_free, an implied price of 0 is the atlas_implied_price_zero issue, and lineage comes from maker_id (architecture Unknown kept separate). 9 atlas_export tests.
- ci-v2 ACCEPTED (MTD/QA mimo): CI carries the charter pattern-C template v2 at CHARTER_REV 7b12eea (generic comment, typed not-run line), every Action is pinned by commit SHA (O-STD-6 paid), and a weekly audit.yml runs cargo audit (root and worker) and pnpm audit (O-STD-5 paid).
- worker-tokio ACCEPTED (MTD/QA mimo, no defects): crates/worker/Cargo.lock moves tokio 1.54.0 → 1.54.1 (RUSTSEC-2026-0384, yanked). The advisory is not reachable, because tokio has no features in the wasm Worker; the bump ships with the next deploy (S-11).
- s12 ACCEPTED (S-12, registry facts from a pinned model-atlas export; the polyswarm MODEL-ATLAS customer test): code by MTD/DEV (deepseek-flash); V-12 by MTD/QA (mimo, third family) on frozen head 321388d, no defects; receipt charter/receipts/s12-v12.json; reopen attestation. Pin: model-atlas.export/1 2026.11.01-2, sha256 e1262847…, verified against model-atlas 8519e20 and 33477fc. `cargo test -p tier-drop-cli --test atlas_export` runs 6 tests (unpriced and stale on derived copies).
- charter-ci-c ACCEPTED (MTD/QA mimo, no defects; receipt charter/receipts/charter-ci-c.json, reopen attestation): CI carries the fleet pattern-C charter job at CHARTER_REV 8610d51. While the charter repository is private it logs the typed line "charter not run: repo private (owner decision pending)" and passes (CI 41280424818); once the repository is reachable it enforces `check --strict` with no edit. It supersedes the held branch charter-ci (archive/charter-ci).
- s10 ACCEPTED (S-10 adjacent escalation moves): code by MTD/DEV (Pi, deepseek-flash); V-10 by MTD/QA (Pi, xiaomi/mimo-v2.6-flash, a third family) on frozen head bfae923 (manifest 93fbe7a2…), no defects. On branch s10 (tag archive/s10, 5840a73): receipt charter/receipts/s10-v10.json is dispatch-cited; `gate --accept --receipt` passed; reopen attestation charter/.freeze/reopen/s10.json covers proofs 1-7; `receipt --verify-tree` passes against the tip that carries the freeze record. s10 is NOT merged to main: S-11 (deploying the Worker that accepts `lineage`) must come first, and it needs A/EXEC-PM's go-ahead.
- plan-adjacent ACCEPTED (the plan amendment for adjacent escalation moves). Independent QA by MTD/QA (Pi, xiaomi/mimo-v2.6-flash): attempt 1 rejected with D1-D4 (charter/receipts/plan-adjacent-attempt1-rejected.json); one repair cycle (27b72c9); attempt 2 accepted on frozen head 27b72c9 (manifest 26a2ac33…), receipt charter/receipts/plan-adjacent-attempt2.json, dispatch-cited. `charter receipt` is valid, `gate --accept --receipt` passed, and reopen evidence is recorded with `charter reopen` (charter/.freeze/reopen/plan-adjacent.json).
- slice-zero ACCEPTED (QA-9, MiMo, frozen tip 6ee4810): the local CLI and the live MCP plan identically.
- G1, G2, G3: proven by the named tests; CI on main is green.
- Merged slices S-01 to S-09 with V-01 to V-09.
Not proven / known gaps:
- S-11 (deploy of the adjacent tool) is not done: deploying is a production change that waits on A/EXEC-PM. Until then s10 stays unmerged, because merging it would turn the live Slice Zero proof red (the live Worker rejects `lineage`).
- QA findings carried as obligations from plan-adjacent: D5 plan/sources.md still states the pre-amendment counts and hashes; D6 plan/implementation-plan.md omits S-10 and S-11.
- slice-zero receipt RE-ISSUED and valid: MTD/QA (mimo) re-verified proofs 1-3 with mutants against the charter.freeze/2 record (6ee4810, 99 entries, manifest 8ec3369d…); `charter receipt` valid; `gate --accept --receipt` passed; reopen attestation in charter/.freeze/reopen/slice-zero.json (evidence sha256 082bb3ae…). The DONE list is restored to its dispatched bytes (the hand-added reopened markers are replaced by the attestation). The receipt is kept in the private evidence root, because its free-text details name local paths; the committed attestation binds it by hash.
- CI cannot enforce charter while the charter repository is private (owner decision). The job records that as a typed limitation; local hooks and the pre-push `check --strict` enforce meanwhile.
- Common standard obligations O-STD-1 to O-STD-8 (charter/STANDARD-ADOPTION.md). O-STD-2 is paid: the Captain handoff is in the private root (`$AGENT_PRIVATE_ROOT/model-tier-drop/handoffs/`), outside the checkout.
- No COST.jsonl rows: real token counts are not available, and the ledger forbids invented values.
- INTENT.md is pending owner ratification.
Exact last-green command(s):
- `charter check --strict` and `charter gate plan-adjacent --accept --receipt charter/receipts/plan-adjacent-attempt2.json`
- `cargo test --workspace` (main: 82 passed; s10: 106 passed plus the worker's 23)
- `cargo test -p tier-drop-cli --test slice_zero -- --ignored --nocapture` (main)
Exact last-red command(s) and why:
- `cargo test -p tier-drop-cli --test slice_zero -- --ignored` on s10: expected until S-11 deploys the Worker that accepts `lineage`.
Next three bounded items (in order):
1. S-11 (owner GO and a Cloudflare token with Workers Scripts: Edit): deploy the Worker from branch s10 (runbook in crates/worker/README.md "Next deploy"), V-11, then merge s10 (accepted; tag archive/s10).
2. atlas_ref in registry models (deferred: needs a Model field that the engine and the deployed Worker reject until S-11). Bind card qwen3.8-27b ↔ registry qwen-3.8-27b and card deepseek-v4.1-flash ↔ registry deepseek-4.1-flash (model-atlas flash-identity-1), using the first pin after 2026.11.02-4.
3. Optional, requested by PM-MISTER-CLEAN for RC-002: one more v4-pro falsifier run per arm (k=3) for ROUND14; not started at the wind-down.
Open owner decisions:
- Ratify INTENT.md.
- Make the charter repository public (blocks charter-ci).
- Adjacent moves: does a passed-over tier (no fitting model, or disqualified) count as one step? (plan/amendments/2026-10-31-adjacent-escalation.md.)
- Branch deletions (merged branches) and scratch deletions are listed with A/EXEC-PM.
Where everything is (branches, lanes, receipts):
- charter/SPEC.md refreshed with `charter init --refresh-spec` (135eb9c8… to 387276a3…, then to 56f03db2… at pin c923a80; one line; PM-CHARTER confirmed the command only rewrites a known historical SPEC.md).
- main; branch s10 (accepted, awaiting S-11; owner PM-MODEL-TIER-DROP; tag archive/s10); branch charter-ci (held for the charter-public decision; owner PM-MODEL-TIER-DROP; tag archive/charter-ci). plan-adjacent is merged. Worktree lifecycle records are in the reveng WAVES ledger.
- Receipts: charter/receipts/. Charter pin: b9323cd119f2ef2fe83db21b8ed8f844215e8852 (fleet pin, 2026-11-02; per-repo binary under ~/.local/share/charter/bin/<sha>, hooks reinstalled, config-defined lang-guard also runs on pre-commit). CI CHARTER_REV is still 7b12eea and moves to b9323cd at the next CI change (no standalone push, per PM-CHARTER).
