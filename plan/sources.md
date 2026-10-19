# Plan graph sources and decisions (S-08)

`plan/graph.json` (`reveng.graph/1`, `graph_id: mtd-plan`, revision 1) cites the sources below. Each one is a `source` node with the same id. `content_sha256` is the SHA-256 of the file's bytes, all accessed 2026-10-19 with `read_status: full`.

## Sources

| Source id | URI | Kind | SHA-256 |
|---|---|---|---|
| `SRC-commission` | `repo:plan/commission.md` | user_instruction | `9040ab2bf8f536a350b443502f9f87ff7fc00a4cc5eea5c651326655f1c3ec5d` |
| `SRC-owner-req` | `owner:REQ-MODEL-TIER-DROP-2026-10-19.md` | user_instruction | `a326303ff98abd1bcab19ad88f3d968bae42cd8e408ea1714dd6e1ea1ab344f8` |
| `SRC-owner-review-profile` | `owner:DECISION-D-REVIEW-PROFILE-2026-10-19.md` | user_instruction | `b6cafd734ffa5374423ba3c539a989955545eaa57d5f960175e9b983077102b1` |
| `SRC-owner-python-adapter` | `owner:DECISION-D-PYTHON-ADAPTER-2026-10-19.md` | user_instruction | `60ce24f625895a50fe713337cf0f6bb348d73cd17601f1af277432b9ffcda03f` |
| `SRC-plan-requirements` | `repo:plan/requirements.md` | generated | `8597e55ed35f657b14b13c6d0e8a38675b754b8245922df6fbe90f69f4cb145b` |
| `SRC-plan-contracts` | `repo:plan/contracts.md` | generated | `a55b952293ce21ca339d19fd4f0d3a9b467a3fe0b32d4b262991d44c2d02bdd1` |
| `SRC-plan-technology` | `repo:plan/technology-decisions.md` | generated | `1b83828cc69e78e555011591ed28b01581e3fc6b479262bb539ae7417d261865` |
| `SRC-plan-implementation` | `repo:plan/implementation-plan.md` | generated | `6f80c07125ae8f12e244006baa420dcda5f1f7478e604dbc16eacfb6a4b0cc58` |
| `SRC-plan-coverage` | `repo:plan/slice-coverage.md` | generated | `d898fab01aef1cd1c8860c22797994d2206e30994a3aabaace59e66ee4e911fb` |

### How to recompute

- `repo:` sources: from the repository root, `shasum -a 256 plan/<file>.md`. The hashes match the files as of commit `ead44c6`. If a plan document changes, its hash in `plan/graph.json` is out of date and the graph needs a new revision.
- `owner:` sources: these are the owner's documents. The repository refers to them only by file name and hash and does not copy their text or location. Anyone holding a copy can check it with `shasum -a 256 <path to the owner document>` and compare the result with the table.

The plan documents are recorded as `generated` sources with delegated authority. They can show that a planning choice was made, but they carry no owner authority. Owner authority comes from `plan/commission.md` and the three `owner:` documents.

## How the build was compiled

- Compiler: the REVENG Rust core (`reveng-core` 1.0.1), copied to a scratch directory and built with `cargo +1.88.0 build --release`. The skill directory itself was not modified.
- `reveng graph-check plan/graph.json` passes.
- `reveng compile plan/graph.json plan/scope.json <dir>` was run twice into two new directories. `diff -r` showed no differences. `plan/build.json` and `plan/contracts/*.json` are byte copies of the first run's output.
- Result: 18 slices (9 implement, 9 verify) and 35 selected requirements, with none deferred.

## Decisions

1. **S-06 also depends on S-03, which contracts.md does not list.** S-03 and S-06 both write `README.md` (S-03 the Walkthrough section, S-06 the MCP URL). The compiler rejects two unordered slices that write the same path with `unordered slices have overlapping write scopes`, and it has no way to scope writes to sections of a file. Compiling with the dependencies exactly as `plan/contracts.md` lists them gives that error. The order S-03 before S-06 is already in `plan/implementation-plan.md` (S-03 is in wave 2, S-06 in wave 3, and S-03 merges before S-06), so the graph makes that order explicit. The edge cites the implementation plan as evidence. `plan/contracts.md` was not changed. Its S-06 "Depends on" line should be changed to "S-03, S-05" so the two files agree.
2. **Requirement count is 35, not 38.** `plan/slice-coverage.md` gives a denominator of 38, but its own ranges (R-01…R-06, R-10…R-22, R-30…R-35, R-40…R-43, R-50…R-55) and `plan/requirements.md` add up to 35. The build selects those 35, which matches the S-08 check (`selected_requirement_ids` equals the R-ids in `plan/requirements.md`). The number 38 in `plan/slice-coverage.md` should be corrected.
3. **Glob write prefixes were expanded to concrete paths.** The compiler matches write scopes by literal path prefix, so S-01's `crates/*/Cargo.toml`, `crates/*/src/{lib,main}.rs` and `plan/*.md` became the concrete files (`crates/{core,cli,worker}/Cargo.toml`, `crates/core/src/lib.rs`, `crates/cli/src/main.rs`, `crates/worker/src/lib.rs`, and the six plan documents). Trailing slashes were removed (`crates/core/` became `crates/core`), because the compiler's prefix test treats a prefix ending in `/` as never containing a file.
4. **Verifier write scope.** `plan/contracts.md` gives no write prefixes for the V- slices, but the compiler requires each one to have a write scope and an output. Each verifier writes only `qa/V-nn/verdict.md`, under its own prefix `qa/V-nn`, so no verifier's writes overlap with any other slice's.
5. **Forbidden strings are written as patterns.** The common rules ban two literal strings (the owner's private organisation name and the owner-framing phrase) from every file. Where a requirement, test or check refers to them, the graph uses the regular expressions `O[K]OA` and `needs[ ]you`. These match the same text without containing it. R-55, T-55, V-01.C10, S-04.C5 and V-04.C5 use these patterns.
6. **Checks use logical tool names.** Command checks name a logical program id, not a machine path: `rust.cargo`, `rust.worker_build`, `git.gitleaks`, `shell.grep`, `shell.bash`, `shell.diff`, `shell.cmp`, `yaml.yq`, `node.pnpm`, `cloudflare.wrangler` and `reveng.cli`. Every check has `cwd` "." except the Worker build and fmt checks, which run in `crates/worker` as the contract says, and `wrangler deployments list`. Checks with pipes run through `shell.bash -c`. The S-05 and V-05 HTTP checks expect `wrangler dev --port 8787` to be running already, and the S-06, V-06 and V-09 checks call the live URL.
7. **Test statements have all four parts.** Several T- entries in `plan/requirements.md` give only an action and an observation. In the graph, each test also states a setup and a failure condition, written as the plan's stated precondition and the negation of its observation. No observation was changed.
8. **Stories.** There is one story per slice pair (ST-01…ST-09), in the epic named by its slices. The stories are marked `derived` and cite S-08 step 2.
9. **Decisions in the graph.** The seven defaults the commission recorded instead of asking are decision nodes D-01…D-07 with delegated authority. The two owner decisions are D-REVIEW-PROFILE and D-PYTHON-ADAPTER, with user authority and declared by their owner sources. Each decision links through `r.constrains` to the requirements it shapes.
10. **Jobs.** The commission names eight jobs: the five deliverables in the highest-level job, plus the three things that "must exist from the beginning" (eval harness, dataset creation, routing record). Epics link to these jobs through `r.delivers`, and jobs link to their requirements through `r.requires_obligation`.

## Known conflicts in the plan that the graph records faithfully

- V-01.C10 searches the tree for the private organisation name and expects no match (exit 1). The name appears in `plan/contracts.md` and `plan/requirements.md` today, so this check will fail until those two documents are reworded. The forbidden phrase also appears in `plan/contracts.md`, `plan/requirements.md` and `AGENTS.md`, which affects T-55.
- V-06 searches for 32-character hex strings and expects matches only in record hashes inside tests. `plan/graph.json` and `plan/build.json` contain source digests and the graph root, which also match. V-06 needs to treat these as allowed.
