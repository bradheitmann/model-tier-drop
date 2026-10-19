# Slice contracts (REVENG P10)

Every implementation slice `S-nn` has exactly one paired verification slice `V-nn`, run by a separate QA context that never implemented the slice. Checks are commands with expected exit codes; "evidence required" is what the worker must paste into its completion report. Write prefixes are exclusive: two concurrent slices never write the same prefix. Paths are relative to the repository root.

Common rules for every slice: Rust by default, strict TypeScript only in `adapters/dsh`; no Python, no `.js/.mjs/.cjs` authored files; no secret, phone number, account identifier or internal hostname in any file; `gitleaks protect --staged --redact` before each commit; never write the owner's forbidden organisation name (grep pattern `O[K]OA`) or the forbidden phrase (grep pattern `needs[ ]you`); plain, specific prose; work only in the slice's worktree and branch; never touch `main`.

---

## S-01 Scaffold and plan — kind: implement — epic E5

Requirements: R-50 (workflow file), R-51 (ignore rules), R-53 (licence), R-54 (plan prose), R-55.
Depends on: none. Write prefixes: `LICENSE`, `.gitignore`, `rust-toolchain.toml`, `Cargo.toml`, `crates/*/Cargo.toml`, `crates/*/src/{lib,main}.rs` (stubs), `plan/*.md`, `.github/`, `AGENTS.md`, `README.md` (skeleton), `registry/README.md`.

Steps: 1 licence, ignore rules, toolchain pin; 2 workspace with three crate stubs that build (`core` lib, `cli` bin, `worker` detached workspace); 3 plan documents (commission, requirements, technology decisions, contracts, implementation plan, slice coverage); 4 CI workflow; 5 AGENTS.md with the repository rules.

Checks: `cargo build --workspace` exit 0; `cargo build --manifest-path crates/worker/Cargo.toml --target wasm32-unknown-unknown` exit 0 (stub); `gitleaks protect --staged --redact` exit 0.
Rollback: delete the branch. Evidence: command outputs.

## V-01 Verify scaffold and plan — kind: verify — verifies S-01

Checks: the six plan files exist and each requirement in `plan/requirements.md` has at least one `T-` test; every commission sentence in "What must exist from the beginning" maps to a requirement; `cargo build --workspace` exit 0; `gitleaks detect --redact --source .` exit 0; `grep -rnE "O[K]OA" .` empty; `.github/workflows/ci.yml` parses as YAML and names fmt, clippy, test, wasm build, adapter check and gitleaks.

---

## S-02 Core engine library — kind: implement — epic E2

Requirements: R-10 R-11 R-12 R-13 R-14 R-15 R-17 R-18 R-19 R-22.
Depends on: S-01. Write prefixes: `crates/core/`, `registry/registry.example.json`, `registry/policy.default.json`, `Cargo.lock`.

Steps:
1. Implement the types of `plan/technology-decisions.md` "Data contracts" in `crates/core/src/` as modules: `tier`, `registry`, `task`, `candidate`, `attempt`, `disqualification`, `decision`, `plan`, `eval`, `policy`, `canonical` (canonical JSON + `record_sha256`), `rights`. `#[serde(deny_unknown_fields)]`, `schemars::JsonSchema` derives on every record type.
2. Implement functions with exactly these signatures (module paths free):
   - `validate_registry(&Registry) -> Vec<Issue>` (`Issue { code: String, message: String }`)
   - `validate_task(&Task, &Policy) -> Vec<Issue>` (floors cover `required_floors`; strict-review rule R-22)
   - `qos_gate(&Registry, &Task, &Policy) -> QosGateResult`
   - `ladder(&Task, &Registry, &Policy) -> Vec<Tier>`
   - `plan_next(&Task, &Registry, &[Disqualification], &[Attempt], &Policy) -> Plan`
   - `check_floors(&Task, &Attempt, &Policy) -> FloorResult`
   - `validate_attempt(&Task, &Attempt, &Registry) -> Vec<Issue>` (issues: `task_id_mismatch`, `family_mismatch`, `tier_mismatch` when the candidate's tier differs from `attempt.tier`, `unknown_model` for any model id in the candidate, `recorded_before_task_created` when `attempt.recorded_at < task.created_at`, so floors provably predate results)
   - `assess_disqualification(family: &str, tier: Tier, &[Attempt], &Task, &Registry, registry_as_of: &str, &Policy) -> Option<Disqualification>` (the registry is needed to enumerate the tier's candidates for the coverage rule)
   - `decide(&Task, &Registry, &[Disqualification], &[Attempt], &Policy) -> RoutingDecision`
   - `split_holdout(&[EvalItem], &Policy) -> EvalSplit`
   - `summarise_eval` taking `(family, tier, &Candidate, &[Attempt], &[Floor])` and returning `EvalRun`
   - `dataset_filter<'a>(&'a [Attempt], Purpose) -> Vec<&'a Attempt>` (`Purpose::{Process, Train, SendRemote, Export}`)
   - `canonical_json(&serde_json::Value) -> String`, `record_sha256(&serde_json::Value) -> String` (excludes the `record_sha256` key)
   - `schemas() -> BTreeMap<&'static str, schemars::Schema>` for the names listed under the CLI `schemas` command
   - `Policy::default()` equals `registry/policy.default.json`; `Registry::example()` parses `registry/registry.example.json` (both via `include_str!`).
3. Write `registry/registry.example.json`: the commission's indicative models as of 2026-10-19 across tiers 1–4 (Jev, Laya; a Docling model, a Qwen OCR model, a ScaleDown model; DeepSeek 4.1 Flash, Qwen 3.8 27B, GLM-5.3-flash; GLM-5.3, Qwen 3.8 Max, Mistral 4, Kimi K3, Opus 5.5, GPT-6.1-Sol), one tailored example bound to a family, two embedding helpers, deployments with ids only (`local-mbp`, `local-mini`, `zai`, `anthropic`, `openai`, `deepseek`, `mistral`, `moonshot`, `alibaba`), plausible placeholder costs and QoS marked as example values in the registry's optional top-level `notes`. No hostnames, no keys.
4. Unit tests per requirement, named so `cargo test -p tier-drop-core registry|qos|plan|floors|disqualify|decide|holdout|eval|dataset|review` each select at least one test. Property-style tests for `split_holdout` determinism and whole-family assignment.
5. `cargo fmt`, `cargo clippy -p tier-drop-core --all-targets -- -D warnings`, `cargo test -p tier-drop-core`.

Outputs: `crates/core/src/**/*.rs`, `crates/core/Cargo.toml` (deps: serde, serde_json, sha2, schemars, hex or manual hex), `registry/registry.example.json`, `registry/policy.default.json`.
Checks: `cargo test -p tier-drop-core` exit 0 with ≥ 25 tests; `cargo clippy -p tier-drop-core --all-targets -- -D warnings` exit 0; `cargo fmt --check` exit 0; `cargo build -p tier-drop-core --target wasm32-unknown-unknown` exit 0 (no I/O or time dependencies in the library; timestamps are inputs).
Risk controls: the library has no `std::fs`, `std::time`, `rand` or network use; all ids and timestamps are parameters.
Rollback: revert the branch. Evidence: the four command outputs, the test count, the `wc -l` of `crates/core/src`.

## V-02 Verify core engine — kind: verify — verifies S-02

QA runs the S-02 checks, then: reads every test named in T-10…T-19 and T-22 and confirms each assertion in `plan/requirements.md` is actually asserted (not merely a test with that name); writes one extra adversarial test in a scratch file (not committed) for each of: a tier-5 model placed first by `ladder`; a disqualification with a stale `registry_as_of` ignored by `plan_next`; a candidate below a floor never ranked above one that clears it; and confirms they pass. Confirms `grep -rn "std::fs\|std::time\|SystemTime\|rand" crates/core/src` is empty. Verdict: accept or reject with the failing check and defect list.

---

## S-03 CLI — kind: implement — epic E2

Requirements: R-16 R-20 R-21.
Depends on: S-02. Write prefixes: `crates/cli/`, `Cargo.lock`, `README.md` (the "Walkthrough" section only; keep the rest).

Steps:
1. `clap` derive CLI exactly as the "CLI surface" table in `plan/technology-decisions.md`.
2. Ledger module: append-only JSONL writes with `O_APPEND`, canonical JSON lines, `record_sha256` filled before write, `verify` recomputing hashes and reporting `file:line`, `show` filtering.
3. `attempt record` reads the attempt from stdin (any `outcome`), validates it with `validate_attempt` (exit 1 with the issues on failure), fills `schema_version` and `record_sha256`, appends, prints the `FloorResult`, then calls `assess_disqualification` over the family's attempts for that tier and appends a `Disqualification` when returned (print it too). It appends the task to `tasks.jsonl` when its `task_id` is not yet present (so does `decide`).
4. `decide` loads the ledger's disqualifications, drops those whose `expires_at` is non-null and earlier than the CLI's clock (`SystemTime::now()` formatted as RFC 3339 UTC; the engine has no clock), takes the task's attempts, calls `decide`, appends and prints. `plan` filters the same way.
4a. `disqualify --ledger DIR --registry r.json --family F --tier T --reason <shape_mismatch|no_provisioned_candidates|no_qos_qualified_candidates|rights_forbid_remote|floors_not_met|operator> --evidence id[,id…] --detail "…" [--expires-at RFC3339]` writes an operator-authored record with `registry_as_of` from the registry and `decided_at` from the clock.
5. Integration tests in `crates/cli/tests/` using `std::process::Command` on the built binary (`env!("CARGO_BIN_EXE_tier-drop")`) over a temporary directory: the README walkthrough end to end; ledger tamper detection (T-16); `schemas routing_decision` title (T-21); exit codes (T-20).
6. Write the README "Walkthrough" section: the exact commands a stranger runs (create registry from the example, write a task, plan, record two attempts, decide, verify ledger, export dataset).

Checks: `cargo test -p tier-drop-cli` exit 0; `cargo clippy -p tier-drop-cli --all-targets -- -D warnings` exit 0; `cargo fmt --check` exit 0; the walkthrough commands pasted from README run in a temporary directory and end with a decision whose `winner` is non-null.
Rollback: revert the branch. Evidence: outputs of the checks and the walkthrough transcript.

## V-03 Verify CLI — kind: verify — verifies S-03

QA runs the S-03 checks; executes the README walkthrough by copying it verbatim into a fresh temporary directory; tampers one byte in `attempts.jsonl` and confirms `ledger verify` exits 1 naming the line; confirms `dataset export --purpose train` omits an attempt whose rights have `train: false`; confirms `tier-drop schemas` lists all ten names.

---

## S-04 Skill — kind: implement — epic E1

Requirements: R-01 R-02 R-03 R-04 R-05 R-06.
Depends on: S-01 (uses the data contracts in `plan/technology-decisions.md`; does not need S-02 merged). Write prefixes: `skill/`.

Steps:
1. `skill/SKILL.md` with frontmatter `name: model-tier-drop`, `description` (one sentence, when to use), `whenToUse`, `metadata: { version: "0.1.0", harnesses: [claude-code, codex, dsh, pi, any] }`. Body sections, in this order: The rule (five tiers, embeddings are helpers, QoS gate, ranking order, strict review default); Before you route (write the task record with floors first, copy the registry, check rights); Procedure (numbered: load in-force disqualifications; `plan` → candidates; execute each candidate through your harness; record every attempt including failures and abstentions; when the plan says the tier is done, let `attempt record` assess disqualification; `decide`; append nothing by hand); Composition patterns (panel, cascade, draft, embedding helpers, with when to use each); Records you leave behind (routing decision, attempts, disqualifications, eval runs, with the ledger paths); Eval harness and datasets (pointer to references); Install (CLI via `cargo install --path crates/cli`; hosted MCP URL and how to add it in Claude Code, Codex, dsh, Pi; skill symlink paths per harness). Keep SKILL.md under 300 lines; detail goes in references.
2. References: `references/tiers.md` (what each tier is for, what disqualifies it, examples of structural skips), `references/routing-record.md` (every `RoutingDecision` field with meaning), `references/composition.md`, `references/eval-harness.md` (metric vector, held-out split by source family with the hash rule, EvalRun → Disqualification, strict review default and the sampled exception with its conditions), `references/dataset.md` (attempt record as dataset row, rights by purpose, export, what a tier-5 candidate family looks like: high volume, stable, pass rate, sample size), `references/registry.md` (registry fields, how to add a deployment, QoS fields and that they are measured values, model names are configuration).
3. Wording rules: plain and specific; never the forbidden phrase (`needs[ ]you`); harness names only in the frontmatter `harnesses` list and under Install; model names only when labelled indicative as of 2026-10-19.

Checks: `grep -nE "mcp__|Bash\(|Claude Code|Codex|dsh|Pi " skill/SKILL.md` shows matches only inside the YAML frontmatter (the `harnesses:` line) or below the `## Install` heading; frontmatter has the three keys; `wc -l skill/SKILL.md` ≤ 300; every reference file ≥ 40 lines; `grep -rnE "needs[ ]you" skill` empty.
Rollback: revert the branch. Evidence: the grep outputs and line counts.

## V-04 Verify skill — kind: verify — verifies S-04

QA walks T-01…T-06 literally; additionally cross-checks every field named in `references/routing-record.md` and `references/registry.md` against `plan/technology-decisions.md` (and against `crates/core` if S-02 is merged); role-plays the procedure for a `classify` task and a `generate` task and confirms the steps produce the right first tier for each; checks that the procedure never tells the agent to skip a tier without a recorded disqualification.

---

## S-05 MCP Worker — kind: implement — epic E3

Requirements: R-30 R-31 R-32 R-33 R-34.
Depends on: S-02, S-04. Write prefixes: `crates/worker/`.

Steps:
1. `crates/worker/Cargo.toml`: own `[workspace]` table (detached), `worker = "0.8"`, `tier-drop-core = { path = "../core" }`, `serde_json`; `crate-type = ["cdylib"]`.
2. `wrangler.jsonc`: `name: "model-tier-drop-mcp"`, `main: "build/worker/shim.mjs"`, `compatibility_date: "2026-10-16"`, `build.command: "cargo install -q worker-build --locked && worker-build --release"`, `workers_dev: false`, `preview_urls: false`, `routes: [{ pattern: "model-tier-drop-mcp.bradheitmann.ai", custom_domain: true }]`, `observability: { enabled: true }`. No account id in the file (wrangler takes it from the login).
3. `src/lib.rs`: `#[event(fetch)]` handler; `src/mcp.rs`: JSON-RPC parsing and the method switch mirroring the reference (`initialize`, `server/discover`, `ping`, `tools/list` with `ttlMs: 3600000`, `cacheScope: "public"`, `tools/call`, notifications → 202, batches → -32600, unknown → -32601, `_meta` stateless version check); `src/tools.rs`: the eleven tools with `inputSchema` (`additionalProperties: false`), dispatching to `tier_drop_core`; `src/assets.rs`: `include_str!` of `../../skill/SKILL.md`, every `../../skill/references/*.md`, `../../registry/registry.example.json`; GET routes; security headers; body cap 262144; Origin check; Accept and Content-Type checks.
4. `README.md` in the crate: build, `wrangler dev`, deploy, rollback (`wrangler rollback`), the curl transcript.
5. Local verification: `worker-build --release`; `wrangler dev --port 8787` in the background; curl `initialize`, `tools/list`, `tools/call plan_next` (registry example + a `classify` task), `GET /skill/SKILL.md` sha256 equality; Origin 403; 413 on a 300 KiB body.

Checks: `cargo clippy --manifest-path crates/worker/Cargo.toml --target wasm32-unknown-unknown -- -D warnings` exit 0; `worker-build --release` in `crates/worker` exit 0; the five curl checks against `wrangler dev` with the expected status codes and fields; `cargo fmt --check` in the crate.
Risk controls: no secrets, no bindings, no storage; body cap; Origin check.
Rollback: `wrangler rollback` after deploy; revert the branch before. Evidence: command outputs, the curl transcript.

## V-05 Verify MCP Worker — kind: verify — verifies S-05

QA rebuilds (`worker-build --release`), runs `wrangler dev` itself, performs its own curl transcript for T-30…T-34 (including a `tools/list` check that every tool has `additionalProperties: false`, and `tools/call` with an invalid payload returning `isError: true` and a JSON-RPC 200, not a 500), and confirms the served `SKILL.md` hash equals the repository file.

---

## S-06 Deploy and live verification — kind: implement — epic E3

Requirements: R-35.
Depends on: S-03, S-05 (both S-03 and S-06 write `README.md`, so they are ordered). Write prefixes: `crates/worker/README.md` (live URL and transcript section), `README.md` (MCP URL).

Steps: 1 `wrangler whoami | grep -v -iE 'account id|[0-9a-f]{32}'` to confirm login without printing identifiers; 2 `wrangler deploy` from `crates/worker`; if the custom domain fails to attach, switch to `workers_dev: true`, redeploy and record the reason; 3 live curls: `initialize`, `tools/list`, `tools/call plan_next`, `GET /skill/SKILL.md` hash; 4 record the redacted transcript in `crates/worker/README.md`.

Checks: three live HTTP 200s with the fields of T-31/T-32; hash equality; `wrangler deployments list` shows the version (identifiers redacted in the evidence).
Rollback: `wrangler rollback`. Evidence: the redacted transcript.

## V-06 Verify live deployment — kind: verify — verifies S-06

QA performs its own live `initialize`, `tools/list`, `tools/call decide` with a two-attempt payload, and `GET /registry/example.json`, from a fresh shell; confirms no account identifier appears in any committed file (`grep -rEo '\b[0-9a-f]{32}\b' --exclude-dir=target --exclude-dir=node_modules --exclude-dir=.git --exclude=Cargo.lock --exclude=pnpm-lock.yaml .` is empty; the word boundaries keep 64-character sha256 digests in `plan/graph.json`, `plan/build.json` and tests from matching).

---

## S-07 dsh adapter — kind: implement — epic E4

Requirements: R-40 R-41 R-42 R-43.
Depends on: S-03, S-04, S-06 (for the live URL; use the planned URL if S-06 is not yet merged and update after). Write prefixes: `adapters/`.

Steps:
1. `adapters/dsh/package.json` (`name: dsh-tier-drop`, `private: true`, `type: module`, `engines.node >= 24`, `packageManager: pnpm`), peer deps pinned to the installed dsh: `@deepseek-ai/cordis` 4.0.2, `@deepseek-ai/dsh-tools` 0.1.6-alpha.2, `@deepseek-ai/schemastery`; `dsh.bundle.patch: ./cordis.patch.yml`; scripts `build` (tsc), `typecheck` (tsc --noEmit), `test` (node --test tests/*.test.ts), `check`.
2. `tsconfig.json` strict (`strict`, `noUncheckedIndexedAccess`, `exactOptionalPropertyTypes`, `module: NodeNext`), `rootDir: src`, `outDir: dist`.
3. `src/index.ts`: `name`, `inject = ['tools']`, `Config` via schemastery (`binary`, `ledgerDir`, `registryPath`, `policyPath?`, `timeoutMs?`), `apply(ctx, config)` registering the five tools with `defineTool`; a `runCli(args, stdin, signal)` helper with `execFile` (no shell), `timeout`, `maxBuffer 8 MiB`, JSON parse of stdout, error → `{ ok: false, error: { code, message } }`; tool arguments are validated and may not carry paths.
4. `cordis.patch.yml` with the two rows (native plugin and `mcp-tier-drop` streamable-http mount); `README.md` with install steps (CLI install, `pnpm install && pnpm build`, `dsh plugin --profile <p> add <path>`, patch rows, skill symlink) and the **Pi** section (R-43).
5. `tests/plugin.test.ts`: a fake `ctx` with `tools.register` capturing definitions; point `binary` at the real `tier-drop` built by `cargo build -p tier-drop-cli` (`../../target/debug/tier-drop`) and run `tier_drop_plan` over a temporary ledger; assert five registrations and the plan's `kind`.
6. Isolated dsh composition check (R-41): `export DSH_HOME=$(mktemp -d)`; `npx -y @deepseek-ai/dsh@0.1.6-alpha.2 --profile tdcheck --from-default-profile headless --dump-config` to initialise; `npx -y @deepseek-ai/dsh@0.1.6-alpha.2 plugin --profile tdcheck add <absolute path to adapters/dsh>`; `npx … --profile tdcheck --patch <absolute path to adapters/dsh/cordis.patch.yml> --dump-config`; confirm both rows appear resolved. Record the exact commands and output in `adapters/dsh/README.md` under "Verified". Do not touch `~/.dsh`.

Checks: `pnpm --dir adapters/dsh install` exit 0; `pnpm --dir adapters/dsh check` exit 0; the dump-config output contains `dsh-tier-drop` and `mcp-tier-drop`; `ls adapters/dsh/src` contains only `.ts`.
Rollback: revert the branch; the temporary `DSH_HOME` is deleted. Evidence: outputs, the dump-config excerpt.

## V-07 Verify dsh adapter — kind: verify — verifies S-07

QA runs `pnpm --dir adapters/dsh check`, repeats the isolated `DSH_HOME` composition check from scratch, confirms a tool argument carrying `ledgerDir` is rejected, confirms the Pi section has the three differences and does not claim a built Pi adapter, confirms `~/.dsh` was not modified (`stat -f %m ~/.dsh/profiles/headless/cordis.patch.yml` unchanged versus the value recorded before the slice).

---

## S-08 Plan graph and compiled build — kind: implement — epic E5

Requirements: R-54.
Depends on: S-01. Write prefixes: `plan/graph.json`, `plan/scope.json`, `plan/build.json`, `plan/contracts/`, `plan/sources.md`.

Steps:
1. Build the REVENG core in a disposable copy: `cp -R ~/.agents/skills/reveng/core /tmp/reveng-core && cargo build --release --manifest-path /tmp/reveng-core/Cargo.toml`.
2. Author `plan/graph.json` as `reveng.graph/1` (types in `~/.agents/skills/reveng/core/src/model.rs`, validation in `graph.rs`): sources (this repository's `plan/commission.md` with its sha256, and the two owner decision documents by logical uri `owner:<filename>` with sha256, read_status full); node types for intent, job, epic, story, requirement, test, slice; edge categories and types including the contract roles (`requirement_of_epic`, `accepted_by_test`, `story_of_epic`, `slice_of_story`, `implements_obligation`, `verifies_obligation`, `prerequisite` with `execution_dependency: true`); nodes: the root intent, jobs, the five epics, one story per slice group, every `R-` requirement with `requirement` payload (epic, tests, critical, kind, applicability), every `T-` test, every `S-`/`V-` slice with a full `Slice` payload copied from `plan/contracts.md` (steps, outputs, checks as `command` or `file_exists` primitives, risk controls, rollback, evidence); edges: every relation above, with `prerequisite` edges exactly equal to each slice's `depends_on`.
3. `plan/scope.json` with `mode: full`.
4. `reveng compile plan/graph.json plan/scope.json plan/build` then move `build.json` and `contracts/` under `plan/`; run twice and diff.
5. `plan/sources.md`: the source list with hashes and how to recompute them.

Checks: `/tmp/reveng-core/target/release/reveng compile plan/graph.json plan/scope.json /tmp/out1` exit 0 and again to `/tmp/out2`; `diff -r /tmp/out1 /tmp/out2` empty; `jq '.slices | length' plan/build.json` = 18; `jq -r '.selected_requirement_ids[]' plan/build.json | sort` equals the sorted R-ids in `plan/requirements.md`.
Rollback: revert the branch. Evidence: compile output JSON, diff result, the two jq outputs.

## V-08 Verify plan graph — kind: verify — verifies S-08

QA rebuilds the compiler in its own disposable directory, compiles, compares bytes with the committed `plan/build.json` and `plan/contracts/*.json`, deletes one verifier slice from a scratch copy of the graph and confirms the compiler rejects it, and confirms every `depends_on` in `plan/contracts.md` matches a `prerequisite` edge.

---

## S-09 README, docs, CI green, publication — kind: implement — epic E5

Requirements: R-50 R-52 R-53 R-55.
Depends on: S-03, S-06, S-07, S-08 merged. Write prefixes: `README.md`, `AGENTS.md`, `docs/`, `.github/`.

Steps: 1 README: the rule in one screen, layout, install CLI, walkthrough (from S-03), hosted MCP (URL, add to Claude Code / Codex / dsh / Pi), dsh adapter, eval and dataset summary, development (fmt, clippy, test, wasm build, adapter check, gitleaks), licence; 2 run every CI command locally from a fresh clone in a temporary directory; 3 `gitleaks detect --redact`; 4 `gh repo create bradheitmann/model-tier-drop --public --source . --remote origin --push` (from `main` after the QA gate), set description and topics; 5 confirm CI runs green on GitHub (`gh run list`), fix if not, within the three-attempt budget.

Checks: fresh-clone run of all CI commands exit 0; `gh repo view --json visibility,licenseInfo`; `gh run list --limit 1` shows success (or the exact failure is reported).
Rollback: the repository can be made private with `gh repo edit --visibility private` if a defect is found after publication. Evidence: outputs.

## V-09 Verify publication — kind: verify — verifies S-09

QA clones `https://github.com/bradheitmann/model-tier-drop` into a temporary directory with no prior context, follows the README top to bottom, runs the CI commands and the walkthrough, confirms licence and visibility, and confirms the live MCP URL in the README answers `initialize`.
