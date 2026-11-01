# State — model-tier-drop — 2026-11-01T02:45Z, written by PM-MODEL-TIER-DROP (Claude Opus 5.5)
Proven (with the proof names):
- s10 ACCEPTED (S-10 adjacent escalation moves): code by MTD/DEV (Pi, deepseek-flash); V-10 by MTD/QA (Pi, xiaomi/mimo-v2.6-flash, a third family) on frozen head bfae923 (manifest 93fbe7a2…), no defects. Receipt charter/receipts/s10-v10.json is dispatch-cited; `gate --accept --receipt` passed; reopen attestation charter/.freeze/reopen/s10.json covers proofs 1-7; `receipt --verify-tree` passes against the tip that carries the freeze record. s10 is NOT merged to main: S-11 (deploying the Worker that accepts `lineage`) must come first, and it needs A/EXEC-PM's go-ahead.
- plan-adjacent ACCEPTED (the plan amendment for adjacent escalation moves). Independent QA by MTD/QA (Pi, xiaomi/mimo-v2.6-flash): attempt 1 rejected with D1-D4 (charter/receipts/plan-adjacent-attempt1-rejected.json); one repair cycle (27b72c9); attempt 2 accepted on frozen head 27b72c9 (manifest 26a2ac33…), receipt charter/receipts/plan-adjacent-attempt2.json, dispatch-cited. `charter receipt` is valid, `gate --accept --receipt` passed, and reopen evidence is recorded with `charter reopen` (charter/.freeze/reopen/plan-adjacent.json).
- slice-zero ACCEPTED (QA-9, MiMo, frozen tip 6ee4810): the local CLI and the live MCP plan identically.
- G1, G2, G3: proven by the named tests; CI on main is green.
- Merged slices S-01 to S-09 with V-01 to V-09.
Not proven / known gaps:
- S-11 (deploy of the adjacent tool) is not done: deploying is a production change that waits on A/EXEC-PM. Until then s10 stays unmerged, because merging it would turn the live Slice Zero proof red (the live Worker rejects `lineage`).
- QA findings carried as obligations from plan-adjacent: D5 plan/sources.md still states the pre-amendment counts and hashes; D6 plan/implementation-plan.md omits S-10 and S-11.
- slice-zero's receipt has no frozen manifest (`charter receipt` refuses). Next: re-freeze slice-zero with the fixed charter (freeze-scope-2), record its reopens with `charter reopen`, and have its receipt re-issued.
- charter-ci is held: the charter repository is private, so public CI cannot install it (owner decision). The pending CI pin moves to the fixed charter rev.
- Common standard obligations O-STD-1 to O-STD-8 (charter/STANDARD-ADOPTION.md).
- No COST.jsonl rows: real token counts are not available, and the ledger forbids invented values.
- INTENT.md is pending owner ratification.
Exact last-green command(s):
- `charter check --strict` and `charter gate plan-adjacent --accept --receipt charter/receipts/plan-adjacent-attempt2.json`
- `cargo test --workspace` (main: 82 passed; s10: 106 passed plus the worker's 23)
- `cargo test -p tier-drop-cli --test slice_zero -- --ignored --nocapture` (main)
Exact last-red command(s) and why:
- `cargo test -p tier-drop-cli --test slice_zero -- --ignored` on s10: expected until S-11 deploys the Worker that accepts `lineage`.
- `charter receipt charter/receipts/slice-zero-qa9.json`: refused, no frozen manifest (re-freeze queued).
Next three bounded items (in order):
1. S-11: deploy the Worker from s10 with A/EXEC-PM's go-ahead, run the live checks, V-11, then merge s10 to main.
2. A docs slice for D5 and D6.
3. slice-zero re-freeze and receipt re-issue; the CI pin moves to the fixed charter rev.
Open owner decisions:
- Ratify INTENT.md.
- Make the charter repository public (blocks charter-ci).
- Adjacent moves: does a passed-over tier (no fitting model, or disqualified) count as one step? (plan/amendments/2026-10-31-adjacent-escalation.md.)
- Branch deletions (merged branches) and scratch deletions are listed with A/EXEC-PM.
Where everything is (branches, lanes, receipts):
- main; branch s10 (accepted, awaiting S-11; owner PM-MODEL-TIER-DROP; tag archive/s10); branch charter-ci (held for the charter-public decision; owner PM-MODEL-TIER-DROP; tag archive/charter-ci). plan-adjacent is merged. Worktree lifecycle records are in the reveng WAVES ledger.
- Receipts: charter/receipts/. Charter binary: charter 0.1.0 built from charter commit 8610d51 (includes freeze-scope-2, reopen attestation, accepted_unix_s).
