# Plan graph sources and decisions (S-08)

`plan/graph.json` (`reveng.graph/1`, `graph_id: mtd-plan`, revision 1) cites the sources below. Each one is a `source` node with the same id. `content_sha256` is the SHA-256 of the file's bytes, all accessed 2026-10-19 with `read_status: full`.

## Sources

| Source id | URI | Kind | SHA-256 |
|---|---|---|---|
| `SRC-commission` | `repo:plan/commission.md` | user_instruction | `9040ab2bf8f536a350b443502f9f87ff7fc00a4cc5eea5c651326655f1c3ec5d` |
| `SRC-owner-req` | `owner:REQ-MODEL-TIER-DROP-2026-10-19.md` | user_instruction | `a326303ff98abd1bcab19ad88f3d968bae42cd8e408ea1714dd6e1ea1ab344f8` |
| `SRC-owner-review-profile` | `owner:DECISION-D-REVIEW-PROFILE-2026-10-19.md` | user_instruction | `b6cafd734ffa5374423ba3c539a989955545eaa57d5f960175e9b983077102b1` |
| `SRC-owner-python-adapter` | `owner:DECISION-D-PYTHON-ADAPTER-2026-10-19.md` | user_instruction | `60ce24f625895a50fe713337cf0f6bb348d73cd17601f1af277432b9ffcda03f` |
| `SRC-plan-requirements` | `repo:plan/requirements.md` | generated | `7ea8f36c977fa6196132b69d7da1745b0dff28b7a4b6877d1a6ed836fec36206` |
| `SRC-plan-contracts` | `repo:plan/contracts.md` | generated | `75c093918b47d60c1b22a64c0628a8cb479329b0457f38cf29eac7058a288362` |
| `SRC-plan-technology` | `repo:plan/technology-decisions.md` | generated | `e46697197f255be61f16a84e6c9075435af85caab05fb9aea868a48efc0e29be` |
| `SRC-plan-implementation` | `repo:plan/implementation-plan.md` | generated | `a25551e27186f99582e36bf6521b409e9770dc85595887598ff39ed4da6204e8` |
| `SRC-plan-coverage` | `repo:plan/slice-coverage.md` | generated | `f8c0fb566e4e95a55e7fea32fdf9d51abc90edebfaf41a390ddb4a3a2775201c` |

### How to recompute

- `repo:` sources: from the repository root, `shasum -a 256 plan/<file>.md`. The hashes match the files as of commit `4b2161d` (`plan/commission.md` is unchanged since `ead44c6`). If a plan document changes, its hash in `plan/graph.json` is out of date and the graph needs a new revision.
- `owner:` sources: these are the owner's documents. The repository refers to them only by file name and hash and does not copy their text or location. Anyone holding a copy can check it with `shasum -a 256 <path to the owner document>` and compare the result with the table.

The plan documents are recorded as `generated` sources with delegated authority. They can show that a planning choice was made, but they carry no owner authority. Owner authority comes from `plan/commission.md` and the three `owner:` documents.

## How the build was compiled

- Compiler: the REVENG Rust core (`reveng-core` 1.0.1), copied to a scratch directory and built with `cargo +1.88.0 build --release`. The skill directory itself was not modified.
- `reveng graph-check plan/graph.json` passes.
- `reveng compile plan/graph.json plan/scope.json <dir>` was run twice into two new directories. `diff -r` showed no differences. `plan/build.json` and `plan/contracts/*.json` are byte copies of the first run's output.
- Result: 18 slices (9 implement, 9 verify) and 35 selected requirements, with none deferred. After the S-10 refresh below: 22 slices (11 implement, 11 verify) and 38 selected requirements, still none deferred.

## Decisions

1. **S-06 depends on S-03 and S-05.** Both S-03 and S-06 write `README.md`, and the compiler rejects two unordered slices with overlapping write scopes (`unordered slices have overlapping write scopes`). The first graph added the S-03 dependency itself. Since `21a8d54`, `plan/contracts.md` and `plan/implementation-plan.md` list it too, so the `prerequisite` edges now match `depends_on` in `plan/contracts.md` exactly.
2. **35 requirements.** The build selects the 35 R- ids in `plan/requirements.md`. Since `21a8d54`, `plan/slice-coverage.md` also gives 35; it used to say 38.
3. **Glob write prefixes were expanded to concrete paths.** The compiler matches write scopes by literal path prefix, so S-01's `crates/*/Cargo.toml`, `crates/*/src/{lib,main}.rs` and `plan/*.md` became the concrete files (`crates/{core,cli,worker}/Cargo.toml`, `crates/core/src/lib.rs`, `crates/cli/src/main.rs`, `crates/worker/src/lib.rs`, and the six plan documents). Trailing slashes were removed (`crates/core/` became `crates/core`), because the compiler's prefix test treats a prefix ending in `/` as never containing a file.
4. **Verifier write scope.** `plan/contracts.md` gives no write prefixes for the V- slices, but the compiler requires each one to have a write scope and an output. Each verifier writes only `qa/V-nn/verdict.md`, under its own prefix `qa/V-nn`, so no verifier's writes overlap with any other slice's.
5. **Forbidden strings are written as patterns.** The two banned strings appear only as the grep patterns `O[K]OA` and `needs[ ]you`, which match the strings without containing them. Since `21a8d54` the plan documents use the same patterns, so R-55, T-55, V-01.C10, S-04.C5 and V-04.C5 copy the plan wording.
6. **Checks use logical tool names.** Command checks name a logical program id, not a machine path: `rust.cargo`, `rust.worker_build`, `git.gitleaks`, `shell.grep`, `shell.bash`, `shell.diff`, `shell.cmp`, `yaml.yq`, `node.pnpm`, `cloudflare.wrangler` and `reveng.cli`. Every check has `cwd` "." except the Worker build and fmt checks, which run in `crates/worker` as the contract says, and `wrangler deployments list`. Checks with pipes run through `shell.bash -c`. The S-05 and V-05 HTTP checks expect `wrangler dev --port 8787` to be running already, and the S-06, V-06 and V-09 checks call the live URL.
7. **Test statements have all four parts.** Several T- entries in `plan/requirements.md` give only an action and an observation. In the graph, each test also states a setup and a failure condition, written as the plan's stated precondition and the negation of its observation. No observation was changed.
8. **Stories.** There is one story per slice pair (ST-01…ST-09), in the epic named by its slices. The stories are marked `derived` and cite S-08 step 2.
9. **Decisions in the graph.** The seven defaults the commission recorded instead of asking are decision nodes D-01…D-07 with delegated authority. The two owner decisions are D-REVIEW-PROFILE and D-PYTHON-ADAPTER, with user authority and declared by their owner sources. Each decision links through `r.constrains` to the requirements it shapes.
10. **Jobs.** The commission names eight jobs: the five deliverables in the highest-level job, plus the three things that "must exist from the beginning" (eval harness, dataset creation, routing record). Epics link to these jobs through `r.delivers`, and jobs link to their requirements through `r.requires_obligation`.
11. **Refresh against plan `21a8d54`.** `main` (`21a8d54`) was merged into `s08`, and the graph was updated to match the amended plan documents:
    - Hashes: the five `repo:plan/*` hashes were recomputed. `plan/commission.md` did not change, so the decision nodes did not change.
    - Requirements and tests: R-12, R-13, R-14, R-16, R-20, R-21, R-32, R-55 and T-12, T-13, T-14, T-20, T-21, T-32, T-51, T-55 were rewritten to the new text.
    - S-02: step 2 adds `validate_attempt` and the `&Registry` parameter of `assess_disqualification`. Step 3 has the larger deployment list and the top-level `notes`.
    - S-03: steps 3, 4 and 4a cover any outcome, `validate_attempt`, task append, CLI-clock expiry, and operator `disqualify`.
    - S-04 and V-04: the wording rule changed, and the harness-name check now allows the frontmatter `harnesses:` line. The phrase check uses `grep -rnE`.
    - S-05 and V-05: twelve tools; the `tools/list` checks expect 12.
    - V-01: six plan files; the check is `grep -rnE "O[K]OA" .`.
    - V-06: the digest search uses `\b` word boundaries and is now a typed check (V-06.C4, exit 1 when empty).
    - All slices: the shared wording rule changed.
    - `prerequisite` edges: they now cite `plan/contracts.md` "Depends on" and the dependency list in `plan/implementation-plan.md`.

12. **Final refresh against plan `4b2161d`.** `main` (`4b2161d`) was merged into `s08`. Commit `97c1ada` changed only `plan/requirements.md` (T-22: metadata values are strings, and the `sampled_review_forbidden` and `floor_out_of_range` issues are named) and `plan/technology-decisions.md` (`registry_validate` output typed `[Issue]`). The graph changes are the T-22 statement and the two source hashes for those files. No slice, edge or decision changed.

## S-10 refresh (adjacent escalation moves)

S-10 adds the amendment of 2026-10-31 to the graph.

- Sources: the five `repo:plan/*` hashes were recomputed after the plan documents were amended. `plan/contracts.md` adds S-10 and S-11, `plan/requirements.md` adds R-07, R-23, R-24 and their tests, `plan/technology-decisions.md` adds `Model.lineage` and `AdjacentMoves`, and `plan/slice-coverage.md` lists the new slices. `plan/commission.md` did not change, so the decision nodes did not change.
- Requirements and tests: R-07, T-07, R-23, T-23, R-24 and T-24 are new. R-20, R-21, R-32, T-21 and T-32 were rewritten to the amended text (the `adjacent` command, eleven schemas, thirteen tools).
- Slices and stories: S-10 and V-10 under the new story ST-10 (epic E2); S-11 and V-11 under the new story ST-11 (epic E3).
- Dependencies: S-10 waits for S-01 to S-09, because it edits prefixes those slices own (`crates/core`, `crates/cli`, `crates/worker/src`, `crates/worker/README.md`, `skill/SKILL.md`, `README.md`, `plan/build.json`, `plan/contracts`, `plan/graph.json`, `Cargo.lock`). V-10 waits for S-10. S-11 waits for V-10, which also orders it after S-10 over `crates/worker/README.md`.
- Revision stays 1. The amendment extends the same plan, the compiler does not read `revision`, and no parent root is recorded, so the graph keeps the identity S-08 gave it.
- Result: 22 slices (11 implement, 11 verify) and 38 selected requirements, none deferred. Two compiles into new directories were byte-identical (`diff -r`), and `plan/build.json` and `plan/contracts/*.json` are byte copies of the first.
- Repair cycle 1 (plan-adjacent `27b72c9`, merged into the S-10 branch as `1b2ac91`). The plan text changed in four places, so the graph follows: T-23 states the three-level `more_reasoning` observation; S-08.C7 compares `.slices | length` with the number of `## S-` and `## V-` headings in `plan/contracts.md` instead of the literal 18; S-10 step 3 names the three-level test fixture; and S-10's checks state their expected results, adding `crates/worker` fmt (C4) and `jq '.slices | length' plan/build.json` = 22 (C8); V-10's mutant (c) names the three-level test of T-23. `plan/amendments/2026-10-31-adjacent-escalation.md` changed its status line only and is not a graph source. `plan/requirements.md`, `plan/contracts.md` and `plan/slice-coverage.md` hashes were refreshed again; `plan/technology-decisions.md` and `plan/implementation-plan.md` did not change.

## Earlier conflicts, now resolved

As of `4b2161d`, all three searches return nothing in the merged tree: `grep -rnE "O[K]OA" .`, `grep -rnE "needs[ ]you" .`, and the V-06 word-bounded 32-hex search. V-01.C10, T-55 and V-06.C4 can therefore pass.
