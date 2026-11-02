# Done list — ci-v2 (CI change: charter pattern-C template v2 at the fleet pin, Actions pinned by SHA, scheduled dependency audit; written BEFORE dispatch; does not grow during it)
Base bytes: main at dispatch. Obligations paid: O-STD-6 (Actions pinned to commit SHAs), O-STD-5 (scheduled dependency audit), and the pattern-C v2 adoption planned in charter/STANDARD-ADOPTION.md (removes the internal ruling name from the public ci.yml comment).
Proofs (each: name, what it guarantees, reopen step, fail-without mutant):
1. charter-job-v2: the `charter` job in .github/workflows/ci.yml is byte for byte the v2 template (~/.local/share/charter/ci/charter-job-pattern-C-v2.yml, sha256 86c7a3b196f58115…) except CHARTER_REV = 7b12eeaf8c4d734386a863dc01c06da6c7ac4cb8, and no internal name remains (`grep -n 'A/EXEC-PM' .github/workflows/*.yml` prints nothing). Reopen: extract the job and diff it against the template; the only difference is the CHARTER_REV line. Fail-without mutant: restore the v1 comment line; the diff shows extra lines and the grep finds A/EXEC-PM.
2. actions-pinned: every `uses:` line in .github/workflows/*.yml names a 40-hex commit SHA (`grep -nE 'uses: [^@]+@' .github/workflows/*.yml | grep -vE '@[0-9a-f]{40}'` prints nothing). Reopen: rerun the grep. Fail-without mutant: change one pin back to `@v4`; the grep prints it.
3. audit-workflow: .github/workflows/audit.yml runs on a weekly schedule and on workflow_dispatch with `permissions: contents: read`. It runs `cargo audit` for the root workspace and crates/worker, and `pnpm audit --prod --audit-level high` for adapters/dsh. The same three commands pass locally on the candidate. Reopen: after merge, `gh workflow run audit.yml` on main and check that the run passes. Fail-without mutant: n/a for a scheduled workflow; actionlint (gate) covers its syntax.
4. ci-green: on the pushed branch, CI passes, and the charter job logs `charter not run: charter source unreachable from this runner` (the v2 typed line). Reopen: `gh run view <id> --log --job <charter job id> | grep 'charter not run'`. Fail-without mutant: n/a for a live CI run; proof 1 covers the job's content.
Gates (exact commands):
- `actionlint .github/workflows/*.yml`
- `cargo fmt --all --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace`
- `charter check --strict` (pinned binary 7b12eea)
- `gitleaks protect --staged --redact`
Out of scope (recorded obligations, not fixed here):
- making the charter job required (owner decision: charter public).
Owner decision points:
- none
