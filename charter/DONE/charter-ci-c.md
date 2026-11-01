# Done list — charter-ci-c (Charter CI job, fleet pattern C; written BEFORE dispatch; does not grow during it)
Base bytes: main 26fffec. It supersedes the held slice charter-ci (tag archive/charter-ci at 0aa87a9), whose CI step could not run while the charter repository is private.
Proofs (each: name, what it guarantees, reopen step, fail-without mutant):
1. job-is-pattern-C: the `charter` job in .github/workflows/ci.yml is byte for byte the fleet job ~/.local/share/charter/ci/charter-job-pattern-C.yml (sha256 a09faba36e5def26…), except that CHARTER_REV is set to 8610d51bed10fc3645efcbc38d3f2465fe04174a, the commit `charter --version` names locally. Reopen: extract the job and diff it against the fleet file; the only difference is the CHARTER_REV line. Fail-without mutant: change one character in the job; the diff shows a second difference.
2. probe-behaviour: actionlint passes on the workflow, and the job's probe command, run locally with no credentials (`GIT_TERMINAL_PROMPT=0 git ls-remote --exit-code <url> HEAD`), returns non-zero for https://github.com/bradheitmann/charter (private) and zero for https://github.com/bradheitmann/model-tier-drop (public control). Reopen: rerun both probes and actionlint. Fail-without mutant: point the probe at the public control URL; it reports reachable, so the install step would run.
3. ci-green-typed-limitation: on the pushed branch, CI passes, and the charter job's log contains `charter not run: repo private (owner decision pending)`. Reopen: `gh run view <id> --log --job <charter job id> | grep 'charter not run'`. Fail-without mutant: n/a for a live CI run; proof 2 covers the probe.
4. enforcement-kept: local hooks still refuse a STOP-RULES change without an owner decision. Reopen: append `S8. mutant` to charter/STOP-RULES.md in a scratch worktree; `charter check --strict` exits 1; restore it; exit 0.
Gates (exact commands):
- `actionlint .github/workflows/ci.yml`
- `cargo fmt --all --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace`
- `charter check --strict`
- `gitleaks protect --staged --redact`
Out of scope (recorded obligations, not fixed here):
- Bumping CHARTER_REV and the local per-repo pin (charter TODO 67) when PM-CHARTER announces the cadence-staged-1 pin.
- Making the job required, after the owner decides to make charter public.
Owner decision points:
- Make the charter repository public (owner list).
