# State — model-tier-drop — 2026-10-31T04:05Z, written by PM-MODEL-TIER-DROP (Claude Opus 5.5)
Proven (with the proof names):
- slice-zero ACCEPTED: independent QA by B/QA-9 (OpenCode, MiMo-V2.6-Flash, a non-Claude family) on frozen tip 6ee481047559c8972fab0fe228ed1f19f61e1a7e. Receipt: charter/receipts/slice-zero-qa9.json (sha256 ff02ccdb1297ae3f832a7c35e46855e795e2489f4e6ea85ca7339a0871c95d62). `charter gate slice-zero --accept` passed at done-sha256 e0a5cf3f.
- Reopen evidence: QA-9 reran proofs 1-3 from the frozen commit (C02-C04 green), ran the three fail-without mutants red, and restored them green (C05). The DONE list carries `reopened: qa9-C0n <receipt sha256>` markers, added after `gate --accept` recorded acceptance. A repeat `gate --accept` now refuses ("changed after dispatch"): this is Charter ledger 87 (landing-time markers vs the dispatch pin), reported to PM-CHARTER.
- G1, G2, G3: proven by the named tests; CI on main is green.
- Merged slices S-01 to S-09 with paired V-01 to V-09 (plan/contracts.md).
Not proven / known gaps:
- No COST.jsonl rows for slice-zero: real token counts for the authoring session and QA-9 are not available here, and the ledger forbids invented values (charter check warns).
- `charter receipt` refuses the QA-9 receipt because it lacks frozen.manifest_sha256. `charter freeze` was not run before QA. When run after QA, the manifest also covered ignored and untracked files (6922 node_modules files, crates/worker/build, the untracked Captain handoff), so a clean clone cannot reproduce it. It was not committed and has been moved to reveng reports/qa/tierdrop-slice-zero-freeze-DEFECTIVE-includes-ignored.json for PM-CHARTER. The commit sha in the receipt is the reproducible identity.
- INTENT.md is pending owner ratification.
- No charter check in CI and no charter hooks yet: adoption steps 3 and 5 are the next small slice, using PM-CHARTER's recipe pin 4e9017ce.
- The adjacent-escalation amendment is being drafted on branch plan-adjacent (worktree ~/.config/superpowers/worktrees/model-tier-drop/plan-adjacent).
Exact last-green command(s):
- `cargo test -p tier-drop-cli --test slice_zero -- --ignored --nocapture`
- `cargo test --workspace` (82 passed)
- `charter check --strict`
Exact last-red command(s) and why:
- `charter gate slice-zero --accept` after the reopen markers: refused, DONE hash changed after dispatch (Charter ledger 87; acceptance was recorded before the markers).
- `charter receipt charter/receipts/slice-zero-qa9.json`: refused, frozen.manifest_sha256 missing (see the gap above).
Next three bounded items (in order):
1. charter-ci slice: a charter check --strict step in CI and the charter hooks, at the pinned charter rev.
2. Finish the plan/ amendment for adjacent() (S-10/V-10) and request a DEV/QA pair.
3. A registry resolve step reading pinned model-atlas exports (`atlas_ref`); model-atlas schemas v1 is frozen at a0717d8 (lineage = card.lineage.maker/architecture).
Open owner decisions:
- Ratify INTENT.md.
- Delete the 11 merged local branches (plan, plan-notes, s02 to s09, s09-fix) and, after this merge, charter-init (via A/EXEC-PM).
- Adjacent moves: does skipping a tier with no fitting model or a disqualification count as one step? (plan/amendments/2026-10-31-adjacent-escalation.md, owner decision point.)
Where everything is (branches, lanes, receipts):
- main; branch charter-init (merged by this landing); branch plan-adjacent in its worktree (owner PM-MODEL-TIER-DROP).
- charter binary: ~/.local/bin/charter 0.1.0, built from charter 4e9017c (now pushed).
