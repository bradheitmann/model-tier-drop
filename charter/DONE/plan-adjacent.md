# Done list — plan-adjacent (written BEFORE dispatch; does not grow during it)
Base bytes: main 30ffe92 plus branch plan-adjacent (plan/ only).
Proofs (each: name, what it guarantees, reopen step, fail-without mutant):
1. owner-rule-fidelity: plan/amendments/2026-10-31-adjacent-escalation.md names all four move kinds from the owner decision (reveng charter/owner-decisions/2026-10-31-model-routing-and-escalation.md §3). No rule lets a move span two tiers, except the passed-over reading, which is stated as an open owner decision point. Nothing makes frontier use opt-in. Reopen: reread the amendment against §2-§4 on the frozen bytes. Fail-without mutant: delete the "Owner decision point" section; the review must report the passed-over rule as an unstated multi-tier jump.
2. purity-and-seam: `adjacent` is specified with inputs and outputs only (no I/O, clock or model call), and choosing among moves belongs to Mister Clean. Reopen: reread "The seam" and "Surfaces". Fail-without mutant: add "tier-drop picks the cheapest move" to the amendment; the review must flag a seam breach.
3. testable-requirements: R-07, R-23 and R-24 each carry a test (T-07, T-23, T-24) whose observation can fail. R-20, R-21 and R-32 are amended consistently (adjacent command, 11 schemas, 13 tools). Reopen: `git grep -n -E 'R-0?7|R-2[34]|T-2[34]|T-07|eleven|thirteen|is 11' plan/requirements.md` on the frozen bytes. Fail-without mutant: delete the T-23 line; the review must report R-23 untested.
4. contract-consistency: technology-decisions.md (`Model.lineage`, AdjacentMoves, the CLI line, the MCP row, the schema list), requirements.md, the amendment and contracts.md S-10/V-10/S-11/V-11 agree. S-11 deploys before S-10 merges. slice-coverage.md counts 38 requirements. Reopen: count `^- \*\*R-` in requirements.md (38) and compare field names across the three files. Fail-without mutant: rename `passed_over` to `skipped` in technology-decisions.md only; the review must report the mismatch.
Gates (exact commands):
- `charter check --strict`
- `git grep -nE 'O[K]OA|needs[ ]you' -- plan charter` prints nothing
- `gitleaks protect --staged --redact`
Out of scope (recorded obligations, not fixed here):
- Implementing adjacent() (slice S-10), deploying it (S-11), and recompiling plan/graph.json and plan/build.json (inside S-10).
Owner decision points:
- Do passed-over tiers (no fitting model, or disqualified for the family) count as one step? (amendment, "Owner decision point").
