# Done list — charter-ci (written BEFORE dispatch; does not grow during it)
Base bytes: main 30ffe92.
Proofs (each: name, what it guarantees, reopen step, fail-without mutant):
1. ci-charter-strict: `.github/workflows/ci.yml` has a `charter` job that checks out full history (`fetch-depth: 0`), installs charter at the pinned rev 4e9017ce5cf1a5fe8d5ff558730c92d4202e1a84 with `cargo install --locked --git https://github.com/bradheitmann/charter --rev <rev> charter`, and runs `charter check --strict`; the job is green on the pushed tip. Reopen: `gh run view <run-id> --job <charter job id> --log` shows `charter: ok` for the frozen commit. Fail-without mutant: locally, add a rule line `S8. mutant` to charter/STOP-RULES.md without an owner decision; `charter check --strict` (the job's exact command) exits 1; restore and it exits 0.
2. local-hooks: `charter hooks install` writes .git/hooks/pre-commit (check) and pre-push (check --strict) in the primary checkout. Reopen: `cat .git/hooks/pre-commit .git/hooks/pre-push` shows the charter commands. Fail-without mutant: with the STOP-RULES mutant from proof 1 staged, `git commit` is refused by pre-commit; restore and the commit hook passes.
Gates (exact commands):
- `cargo fmt --all --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace`
- `charter check --strict`
- `gitleaks protect --staged --redact`
- the CI workflow parses as YAML (`ruby -ryaml -e 'YAML.load_file(".github/workflows/ci.yml")'`)
Out of scope (recorded obligations, not fixed here):
- `charter freeze` before QA: the current charter rev hashes ignored and untracked files (reported to PM-CHARTER), so this slice is bound to its commit sha; freeze resumes at the first fixed charter rev.
- Bumping the charter pin to adopt-1 when PM-CHARTER posts it.
Owner decision points:
- none
