# Adoption of the common engineering standard

Standard: COMMON-STANDARD.md v0.2-reviewed (mister-clean main 5d99196; ratification pending-owner). Reviewed against model-tier-drop main on 2026-10-31 by PM-MODEL-TIER-DROP. Where the standard and this repository's charter disagree, the charter governs until the owner decides.

## Section map

| § | Standard | Here | Status |
|---|---|---|---|
| 1 | Public by default; private outside at configurable paths | No private data is committed. The CLI ledger has no default path: `--ledger DIR` is always explicit, and the dsh adapter takes `ledgerDir` from config. gitleaks runs in CI and before each commit. | Gaps O-STD-1, O-STD-2 |
| 2 | Cloud-ready; 127.0.0.1; fresh-clone gates | The hosted MCP is a Cloudflare Worker; the CLI starts no service. Every CI gate runs from a fresh clone. The live Slice Zero test needs the network and is ignored by default. | Clean |
| 3 | Languages and toolchain; one package manager and lockfile | Rust everywhere; strict TypeScript only in adapters/dsh (`"strict": true`, `tsc --noEmit` in `pnpm check`). Toolchain pinned by rust-toolchain.toml and CI. | Exception E-STD-1; gap O-STD-3 |
| 4 | Entry points and one current-state surface | README.md, AGENTS.md, charter/; the current-state surface is charter/STATE.md. | Clean |
| 5 | Ten cleanliness dimensions; generated artifacts; scheduled audit; immutable CI pins | Mister Clean CLOSE (2026-10-31) found no tracked hygiene debt. plan/build.json and plan/contracts/ are compiler output, but CI has no drift check. There is no scheduled dependency audit. Actions are pinned by tag, not by commit. | Gaps O-STD-4, O-STD-5, O-STD-6 |
| 6 | Exact gates; fail-without mutants; independent cross-family QA; reopenable; own build dir | Done lists name a mutant per proof. QA runs on a different model family against a frozen commit (QA-9 MiMo for slice-zero; MTD/QA MiMo for plan-adjacent). Verifiers work in their own worktree, so they use their own target directory. | Clean |
| 7 | Charter; pinned/frozen; plan and work-contract schema (PlanGraph v5) | charter/ is in place and `check --strict` is green. The charter pin in CI is blocked: the charter repository is private (owner decision pending). Plans are `reveng.graph/1` (plan/graph.json), not the v5 work-contract shape. | Gaps O-STD-7, O-STD-8 |
| 8 | Workspace hygiene | Worktree lifecycle records are kept in the reveng WAVES ledger and in charter/STATE.md. Deletions are listed for the owner. Merged branches await the owner's deletion batch. | Clean |
| 9 | Link from AGENTS.md; gaps as obligations; exceptions recorded | AGENTS.md links this file and the standard. | Done |

## Obligations (each becomes a bounded slice with its own done list)

- **O-STD-1** Public-safety check, following the denylist design agreed with PM-MISTER-CLEAN for standard v1. The denylist is UTF-8 text, one identifier per line, with `#` comments; matching is a case-insensitive substring match. It is NOT committed, because a committed list publishes what it protects. It lives at `$AGENT_PRIVATE_ROOT/model-tier-drop/maps/public-denylist.txt` and is read by a local pre-push scan of `git ls-files`. CI may use an optional committed form that holds only sha256 hashes of the lowercased identifiers; the scanner hashes candidate tokens. Ignored local files are reported separately. Proof: a fixture identifier turns the scan red.
- **O-STD-2** PAID 2026-11-01: the untracked Captain handoff was moved (hash-verified) from the repository root to `$AGENT_PRIVATE_ROOT/model-tier-drop/handoffs/`. Dispatch briefs are kept in `$AGENT_PRIVATE_ROOT/model-tier-drop/evidence/briefs/`, where they match the brief hashes pinned in charter/.dispatch.
- **O-STD-3** The CI pnpm version (`pnpm/action-setup` `version: 10`) differs from `packageManager: pnpm@12.11.2`. Make CI read the version from `packageManager`.
- **O-STD-4** Generated-artifact drift: CI recompiles plan/graph.json and fails if the result differs from plan/build.json and plan/contracts/. This needs the plan compiler to be installable in CI (today it is a skill-local build), so the obligation is linked to O-STD-8.
- **O-STD-5** Scheduled dependency audit: a weekly workflow runs `cargo audit` (root and crates/worker) and `pnpm audit` (adapters/dsh). A high advisory is fixed or gets a dated, owned exception here.
- **O-STD-6** Pin GitHub Actions to commit SHAs (checkout, setup-node, pnpm/action-setup, rust-toolchain, gitleaks-action).
- **O-STD-7** Charter in CI at a pinned rev: the charter-ci slice is held because the charter repository is private (owner decision pending). Proposed v1 wording (PM-MISTER-CLEAN): where the governance tool cannot be fetched publicly, CI records the check as a typed limitation, the check runs locally before push (the charter pre-push hook here already does this), and the publication decision belongs to the owner.
- **O-STD-8** Plan and work-contract shape: record the PlanGraph v5 schema pin (specification payload root plus validator release) once PM-PLANGRAPH publishes the validator release. New work contracts are then authored in the v5 authoring form. Until then, plan/ stays `reveng.graph/1`, and validation against v5 is an open obligation here, not a home-made check.

## Exceptions (reason, owner, expiry)

- **E-STD-1** More than one lockfile: Cargo.lock (root workspace), crates/worker/Cargo.lock (a detached wasm32 workspace, so the Worker's dependency graph does not leak into the native crates) and adapters/dsh/pnpm-lock.yaml (the strict TypeScript dsh plugin that the dsh harness requires). Owner: PM-MODEL-TIER-DROP. Expiry: close when standard v1 lands with the wording "one lockfile per package manager per independently built workspace" (agreed with PM-MISTER-CLEAN).
