# Done list — worker-tokio (RUSTSEC-2026-0384: tokio 1.54.0 in crates/worker/Cargo.lock; written BEFORE dispatch; does not grow during it)
Base bytes: main at dispatch. Found by the ci-v2 audit step (`cargo audit --file crates/worker/Cargo.lock`). tokio enters through `worker` 0.8.7 with no features enabled, and Interval needs `time`, so the advisory is not reachable in the deployed Worker. 1.54.0 is also yanked, and the scheduled audit would fail, so the lock moves to the patched 1.54.1.
Proofs (each: name, what it guarantees, reopen step, fail-without mutant):
1. audit-clean: `cargo audit --file crates/worker/Cargo.lock` exits 0, and the lockfile diff against main is only the tokio version and checksum lines. Reopen: rerun both. Fail-without mutant: restore the 1.54.0 version and checksum lines; the audit exits 1 naming RUSTSEC-2026-0384.
2. worker-unchanged-behaviour: `cargo test --manifest-path crates/worker/Cargo.toml` passes, `cargo clippy --manifest-path crates/worker/Cargo.toml --target wasm32-unknown-unknown -- -D warnings` passes, and `worker-build --release` succeeds in crates/worker. Reopen: rerun all three. Fail-without mutant: n/a (no code change); the tests are the regression guard.
Gates (exact commands):
- the three commands of proof 2
- `charter check --strict` (pinned binary 7b12eea)
- `gitleaks protect --staged --redact`
Out of scope (recorded obligations, not fixed here):
- redeploying the Worker; the bump ships with the next deploy (S-11), since the advisory is not reachable.
Owner decision points:
- none
