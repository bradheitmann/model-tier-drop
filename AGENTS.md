<!-- charter v1: harness-independent contract. Do not remove. -->
# Charter (binding for every agent, every harness)
1. Before designing, dispatching, or claiming anything done, read `charter/` in this repo: INTENT.md, GUARANTEES.yaml, LOOPS.yaml, STOP-RULES.md, DONE/<your slice>.md, STATE.md. If any is missing or stale, your first task is to produce it with the owner; do not proceed past it.
2. Done means every proof in `charter/DONE/<slice>.md` executed and its run reopened and compared. It never means "no findings". New findings become recorded obligations on the next bounded slice unless they break a guarantee listed in GUARANTEES.yaml.
3. The stop rules in `charter/STOP-RULES.md` bind you. You may not relax them. A change to them exists only as `charter/owner-decisions/<date>-<slug>.md`.
4. Every guarantee you add gets an owner, a proof and a cost. Every loop you add gets a bound, a cost per trip and a stop condition. No exceptions for "small".
5. Nothing scales before `charter.toml [slice_zero]` has `proven_on` set from a real run.
6. Before you stop, hand off, or approach a context limit: rewrite `charter/STATE.md` (proven, not proven, exact last-green and last-red commands, next three bounded items, open owner decisions). No decision may live only in a pane or transcript.
7. Run `charter check` (or `hooks/charter-check.sh`) before any commit or "ready" claim. Rust only; no Python.
<!-- end charter -->

# Agent instructions for model-tier-drop

- Rust by default. Strict TypeScript only in `adapters/dsh` (dsh plugins are Node modules). No Python. No authored `.js/.mjs/.cjs` files; `crates/worker/build/` and `adapters/dsh/dist/` are generated and ignored.
- The engine (`crates/core`) never calls a model and never touches the filesystem, the clock or the network. Harnesses execute; the engine plans and records.
- Model names are configuration (`registry/registry.example.json`), never code.
- Secrets never enter the repository. Run `gitleaks protect --staged --redact` before every commit and `gitleaks detect --redact` before every push. Check that a key exists by its length, never by printing it.
- `main` receives only slices accepted by a separate QA context against `plan/contracts.md`.
- Plain, specific prose. Never write the forbidden phrase (grep pattern `needs[ ]you`) or the owner's forbidden organisation name (grep pattern `O[K]OA`).
- The plan lives in `plan/`; change the contracts there before changing the code they bind.
- The common engineering standard (COMMON-STANDARD.md v0.2 in the public mister-clean repository, github.com/bradheitmann/mister-clean) applies; this repository's adoption, gaps and exceptions are in `charter/STANDARD-ADOPTION.md`.

## Landing a slice here

1. Change the contracts in `plan/` first. Then write `charter/DONE/<slice>.md`, numbered proofs, each with a reopen step and a fail-without mutant, and run `charter gate <slice> --dispatch [--brief <file>]` before any work starts. The DONE list never changes after dispatch.
2. The implementer works in its own worktree and branch outside the primary checkout, and reports the commit it considers done.
3. The PM reruns the implementer's claims (write prefixes, the core's purity, the gates), then runs `charter freeze <slice>` and commits the freeze record.
4. The QA brief names the frozen head and manifest, the dispatch brief hash and base commit (from `charter/.dispatch/<slice>.json`), and the receipt path. The verifier is from a different model family than the author, works in its own detached worktree, applies every mutant, and writes a `reveng.qa-receipt/1` receipt with `frozen` and `dispatch` blocks and no local paths in free text.
5. Accept: copy the receipt to `charter/receipts/`, run `charter reopen <slice> --proof all --run-id <id> --evidence <receipt>`, then `charter receipt <r>` and `charter gate <slice> --accept --receipt <r>`. Rewrite `charter/STATE.md` in the same commit. A receipt that names local paths stays in the private evidence root; only its hash-bound attestation is committed.
6. Merge with `--no-ff`, run the gates and `charter check --strict` with the repository's pinned charter binary, push, and confirm CI on the pushed commit. Remove the worktree with a plain `git worktree remove` and delete the merged branch.
