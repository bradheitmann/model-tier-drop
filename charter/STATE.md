# State — model-tier-drop — 2026-10-31T03:40Z, written by PM-MODEL-TIER-DROP (Claude Opus 5.5)
Proven (with the proof names):
- slice-zero-live, run 2026-10-31: the local CLI and the live MCP give identical plans; the engine-order mutant turns it red.
- G1, G2, G3 proofs: the named tests are green in `cargo test --workspace`; CI on main aa4765c is green (run 39318608198).
- Merged slices S-01 to S-09 with paired V-01 to V-09 (plan/contracts.md).
Not proven / known gaps:
- The INTENT card is pending owner ratification.
- No charter check in CI and no charter hooks yet (waiting on PM-CHARTER's install recipe).
- The adjacent-escalation seam (owner rule 2026-10-31) is not designed in plan/ yet.
- The model-budget mandate is not mapped yet: separate "cannot fit even after decomposition" from quality disqualification and from account exhaustion.
Exact last-green command(s):
- `cargo test -p tier-drop-cli --test slice_zero -- --ignored --nocapture`
- `cargo test --workspace`
Exact last-red command(s) and why:
- The same Slice Zero command with the tie-break mutant in crates/core/src/plan.rs: "local CLI and live MCP disagree" (intended mutant).
Next three bounded items (in order):
1. Land charter-init on main after independent QA; send the intent card to A/EXEC-PM.
2. Merge or kill (all 11 local branches are already merged; deletion is listed for the owner batch), then a Mister Clean CLOSE pass.
3. Amend plan/ for adjacent(): ladder data and pure move enumeration in core; Mister Clean owns the router and the choice among moves.
Open owner decisions:
- Ratify INTENT.md.
- Delete the 11 merged local branches (plan, plan-notes, s02 to s09, s09-fix), via A/EXEC-PM.
Where everything is (branches, lanes, receipts):
- main aa4765c = origin/main; branch charter-init (this work); no worktrees other than the primary checkout.
- charter binary: built from ~/open_protocols/charter main 4e9017c into ~/.local/bin/charter (0.1.0).
