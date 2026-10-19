# Product requirements (REVENG P08)

Every requirement has a stable ID, an owning epic, a kind, a criticality flag and at least one falsifiable acceptance test. The same IDs appear in `plan/graph.json`; this file is the readable projection.

## Epics

| Epic | Outcome for the beneficiary |
|---|---|
| E1 Skill | An agent in any harness can follow one written procedure to route a task by tier drop and leave the required records. |
| E2 Engine | A typed Rust engine plans, gates, checks floors, disqualifies, decides, splits held-out sets and keeps an append-only ledger, without ever calling a model. |
| E3 Hosted MCP | The skill and the engine's pure operations are reachable over MCP from a Cloudflare Worker on the owner's account. |
| E4 dsh adapter | A dsh profile can mount the skill, the hosted tools and local ledger-writing tools with one patch; a Pi adapter is described. |
| E5 Delivery | The repository is public, licensed, documented for strangers, tested in CI, secret-free and planned with typed REVENG artifacts. |

## Functional requirements

### E1 Skill

- **R-01** (critical, functional) `skill/SKILL.md` is harness-neutral: it names no harness-specific tool and instructs the agent in terms of the CLI, the hosted MCP tools and the record formats. Its frontmatter carries `name`, `description` and `whenToUse` so dsh, Pi, Claude Code and Codex all discover it.
  - T-01 Setup: read SKILL.md. Action: grep for `mcp__`, `Bash(`, `dsh`, `Claude Code`, `Codex` in the procedure sections. Observation: harness names appear only in the "Install" section. Fails if any procedure step depends on a harness-specific tool name.
- **R-02** (critical, functional) The skill's procedure is the tier-drop rule: start at the cheapest tier (tier 5 first only when a tailored model is bound to the family), try every provisioned model × reasoning level, then the composition patterns, and move up only on a recorded disqualification.
  - T-02 Setup: a reviewer with the commission open. Action: walk the procedure against each sentence of "The rule", "The tiers", "Routing criteria", "Composition patterns" and "What must exist from the beginning". Observation: each sentence maps to a numbered step or a reference section. Fails on any unmapped sentence.
- **R-03** (critical, functional) The skill specifies the routing record per task: tiers tried, why each was disqualified, the winner and its evidence, by naming the `RoutingDecision` fields and the ledger file.
  - T-03 Action: compare `skill/references/routing-record.md` with the `RoutingDecision` type in `crates/core`. Fails if a field is missing on either side.
- **R-04** (critical, functional) The skill covers the eval harness: the metric vector, held-out sets split by source family, disqualification evidence per tier, and the strict review default with the sampled-profile exception.
  - T-04 Action: read `skill/references/eval-harness.md`. Observation: it states how `split_holdout` keeps whole source families out of training, how an `EvalRun` becomes a `Disqualification`, and that review is strict unless a recorded decision names a sampled class. Fails if any of the three is absent.
- **R-05** (critical, functional) The skill covers dataset creation: every attempt recorded (including failures and abstentions) with rights by purpose (process, train, send_remote, export) and how `dataset export --purpose` filters on them.
  - T-05 Action: read `skill/references/dataset.md`. Fails if any of the four purposes or the failure/abstention rule is missing.
- **R-06** (functional) The skill says model names are configuration data: it points to `registry/registry.example.json`, tells the agent to copy and edit it, and never lists model names as instructions.
  - T-06 Action: grep SKILL.md and references for the indicative model names. Observation: they appear only in the registry example and in a sentence that labels them indicative as of 2026-10-19.

### E2 Engine

- **R-10** (critical, functional) `crates/core` defines the typed registry: `Tier` (five ordered variants), `Deployment` with `locality` and observed QoS, `Model` with tier, deployment, open-weight flag, reasoning levels, task shapes, bound families, cost, speed and context window, and `EmbeddingModel` as a helper type that cannot be a routing candidate.
  - T-10 Action: `cargo test -p tier-drop-core registry`. Observation: a tier-1 model with a non-decision shape, a tier-2 or tier-5 model with no bound family, a model with an unknown deployment, and an embedding model offered as a candidate are each rejected by `validate_registry`. Fails if any passes.
- **R-11** (critical, functional) `qos_gate` removes deployments below the task's `QosRequirement` before scoring, and removes remote deployments when the task's rights forbid `send_remote`; each exclusion carries a reason.
  - T-11 Action: `cargo test -p tier-drop-core qos`. Observation: a deployment with availability 0.9 against a 0.99 requirement is excluded with `availability`; unmeasured QoS is excluded unless `policy.allow_unmeasured_qos`; a remote model is excluded with `rights_forbid_remote` when `send_remote` is false.
- **R-12** (critical, functional) `plan_next` returns the lowest non-disqualified tier on the task's ladder and an ordered list of untried candidates: singles (model × reasoning level) first, then a panel (up to `policy.panel.max_members`, judge = cheapest QoS-qualified model of the next tier up), a cascade with audited pass-through (next-tier auditor, `policy.cascade.audit_sample_rate`) and a draft composition (next-tier finisher). It skips tiers with structural reasons (shape mismatch for tier 1, no bound model for tiers 2 and 5, nothing QoS-qualified, an in-force disqualification) and reports `exhausted` when no tier remains. The next tier up is fixed: system_one→specialised→cheap_generalist→frontier, tailored→frontier, frontier has none.
  - T-12 Action: `cargo test -p tier-drop-core plan`. Observation: for a `generate` task, tier 1 is skipped with `shape_mismatch`; for a family with a tailored model, the first tier is `tailored`; after all singles of tier 3 are tried and failed, the plan offers a tier-3 panel with a tier-4 judge, then a cascade whose `audit_sample_rate` equals the policy value, then a draft; with every tier disqualified the result is `exhausted`.
- **R-13** (critical, functional) `check_floors` compares an attempt's metric vector with the task's floors (set before results) and reports each failure with the observed value; a missing required metric is a failure, not a pass.
  - T-13 Action: `cargo test -p tier-drop-core floors`. Observation: an attempt lacking `completeness` fails on `completeness` with `observed: null`.
- **R-14** (critical, functional) `assess_disqualification` produces a `Disqualification` for (family, tier) only when the tier's candidates are covered (every single and every offered composition has at least one attempt in the family, unless `policy.disqualify.require_full_coverage` is false) and the evidence meets the policy (`min_attempts` reached and pass rate at or below `max_pass_rate`); structural reasons are reported as plan skips and in the routing decision, not written by the engine as records; the record binds the registry `as_of` date so a newer registry invalidates it; expiry is applied by the caller that owns the clock.
  - T-14 Action: `cargo test -p tier-drop-core disqualify`. Observation: four failing attempts with `min_attempts: 5` yield none; five yield `floors_not_met` with the five attempt ids as evidence when every candidate is covered, and none when one single is untried; a disqualification whose `registry_as_of` predates the registry is ignored by `plan_next`.
- **R-15** (critical, functional) `decide` produces the `RoutingDecision`: ladder, per-tier status with references, the winner (candidate, attempt, floor result, score) and the ranking among qualified candidates by quality band, then cost band, then latency; `winner: null` when exhausted.
  - T-15 Action: `cargo test -p tier-drop-core decide`. Observation: two candidates within the quality band rank by cost; two within both bands rank by latency; a candidate below a floor never ranks above one that clears it.
- **R-16** (critical, data) The ledger is append-only JSONL under one directory (`tasks`, `attempts`, `decisions`, `disqualifications`, `eval-runs`); every record carries `schema_version` and a `record_sha256` over its canonical JSON; `ledger verify` detects any altered line.
  - T-16 Action: `cargo test -p tier-drop-cli ledger`. Observation: after editing one byte of an attempt line, `tier-drop ledger verify` exits non-zero and names the line.
- **R-17** (critical, functional) `split_holdout` assigns whole source families to the held-out split deterministically from `policy.holdout.seed` and `fraction`; no source family appears in both splits.
  - T-17 Action: `cargo test -p tier-drop-core holdout`. Observation: two runs with the same seed give identical splits; every item of a source family shares its split.
- **R-18** (functional) `summarise_eval` aggregates attempts for (family, tier, candidate) into an `EvalRun` with mean metrics, pass rate against the floors and `n`.
  - T-18 Action: `cargo test -p tier-drop-core eval`. Observation: three attempts, two passing, give `pass_rate: 0.6667` (rounded in the test) and `n: 3`.
- **R-19** (critical, data) Every attempt carries the task's `Rights` at attempt time; `dataset_filter(purpose)` returns only attempts whose rights allow the purpose.
  - T-19 Action: `cargo test -p tier-drop-core dataset`. Observation: with `train: false` on one attempt, `dataset export --purpose train` omits it and `--purpose process` includes it.
- **R-20** (functional) The CLI `tier-drop` exposes: `registry validate|example`, `policy default`, `plan`, `qos`, `attempt record`, `decide`, `disqualify`, `ledger verify|show`, `eval split|summarise`, `dataset export`, `schemas`. All input and output is JSON; exit codes are 0 success, 1 validation failure, 2 usage error. `attempt record` accepts every outcome (`answered`, `abstained`, `failed`, `timed_out`); `attempt record` and `decide` append the task to `tasks.jsonl` when its id is new; the CLI drops expired disqualifications using its clock before calling the engine.
  - T-20 Action: `cargo test -p tier-drop-cli` plus the README walkthrough on a temporary directory. Observation: the walkthrough ends with a `decisions.jsonl` line whose `winner` is non-null; an `abstained` attempt is accepted and appears in `dataset export --purpose process`; `tasks.jsonl` holds the task once after two ledger-writing commands.
- **R-21** (functional) `tier-drop schemas` prints JSON Schema for every record type (`registry`, `policy`, `task`, `attempt`, `disqualification`, `routing_decision`, `eval_item`, `eval_split`, `eval_run`, `plan`) so other languages can validate the same files.
  - T-21 Action: `tier-drop schemas routing_decision | jq .title`. Observation: `RoutingDecision`; `tier-drop schemas | jq 'keys | length'` is 10.
- **R-22** (critical, compliance) The engine's `Task.review` defaults to `strict`; a `sampled` value requires a `class` and a `decision_ref`, and the engine refuses `sampled` for tasks whose shape is `extract` of numbers or whose metadata flags `financial` or `regulated`.
  - T-22 Action: `cargo test -p tier-drop-core review`. Observation: a task with `metadata.financial: true` and `review: sampled` fails validation.

### E3 Hosted MCP

- **R-30** (critical, functional) `crates/worker` is a workers-rs Worker that answers MCP over Streamable HTTP with JSON responses at `POST /mcp`: `initialize` (versions 2025-03-26, 2025-06-18, 2025-11-25 and the stateless `_meta` form 2026-07-28), notifications (202, no body), `ping`, `tools/list`, `tools/call`; batches rejected; unknown methods -32601.
  - T-30 Action: `curl` initialize and tools/list against `wrangler dev`. Observation: initialize returns `serverInfo.name: "model-tier-drop"` and the requested protocol version; tools/list returns `tools`, `ttlMs` and `cacheScope`.
- **R-31** (critical, functional) `tools/list` includes `ttlMs` and `cacheScope` (Claude Code rejects lists without them) and every tool has `inputSchema` with `additionalProperties: false`.
  - T-31 Action: `curl` tools/list, `jq '.result | has("ttlMs") and has("cacheScope")'`. Observation: `true`.
- **R-32** (critical, functional) The Worker exposes the engine's pure operations as tools: `skill_read`, `registry_example`, `registry_validate`, `qos_gate`, `plan_next`, `check_floors`, `assess_disqualification`, `decide`, `split_holdout`, `summarise_eval`, `schemas`; each tool returns `content[0].text` and `structuredContent`, with `isError: true` on validation failure.
  - T-32 Action: tools/call `plan_next` with the registry example and a `classify` task. Observation: `structuredContent.tier == "system_one"` and candidates non-empty.
- **R-33** (critical, functional) The Worker serves the skill at `GET /skill/SKILL.md`, `GET /skill/references/<name>.md`, the registry example at `GET /registry/example.json` and schemas at `GET /schemas/<name>.json`, from bytes embedded at compile time so the served skill equals the repository's.
  - T-33 Action: `curl -s $URL/skill/SKILL.md | sha256sum` versus `sha256sum skill/SKILL.md`. Observation: equal.
- **R-34** (critical, security) Requests from a browser `Origin` other than the Worker's own are refused (403); bodies above 256 KiB are refused (413); `Content-Type` must be `application/json` (415); responses carry `X-Robots-Tag: noindex`, `X-Content-Type-Options: nosniff`, `Cache-Control: no-store` on `/mcp`.
  - T-34 Action: `curl -H 'Origin: https://evil.example' …`. Observation: 403.
- **R-35** (critical, delivery) The Worker is deployed on the owner's Cloudflare account from `crates/worker/wrangler.jsonc` with `observability.enabled`, and verified live with a real `initialize`, `tools/list` and `tools/call`.
  - T-35 Action: the three live calls against the deployed URL, transcript redacted of any account identifier. Observation: HTTP 200 for each and the `tools/list` fields of T-31.

### E4 dsh adapter

- **R-40** (critical, functional) `adapters/dsh` is a Cordis plugin package (strict TypeScript, ES modules) exporting `name`, `inject`, `Config` and `apply`, registering tools `tier_drop_plan`, `tier_drop_qos`, `tier_drop_record_attempt`, `tier_drop_decide`, `tier_drop_ledger_show` that call the `tier-drop` CLI with JSON over stdin/stdout; the ledger directory, registry and policy paths come from trusted plugin config, never from tool arguments.
  - T-40 Action: `pnpm --dir adapters/dsh check` (tsc --noEmit strict and node --test). Observation: a fake `ctx.tools` receives five registrations; a tool argument that tries to supply `ledgerDir` is rejected.
- **R-41** (critical, functional) `adapters/dsh/cordis.patch.yml` inserts the native plugin row and a `@deepseek-ai/dsh-mcp-client` row with `transport: streamable-http` pointing at the hosted MCP under `serverName: tier-drop`.
  - T-41 Action: with `DSH_HOME` set to a temporary directory, create a headless profile, add the adapter package, apply the patch and run `dsh --profile <p> --patch … --dump-config`. Observation: the dump contains both rows with resolved config. Fails on a loader error.
- **R-42** (functional) The skill is discoverable by dsh after one symlink: `~/.dsh/skills/model-tier-drop -> <repo>/skill`; the adapter README documents it and the frontmatter satisfies `dsh-skill-filesystem` (`name`, `description`, `whenToUse`).
  - T-42 Action: in the temporary `DSH_HOME`, symlink the skill and dump the catalog (or run the provider's discovery in a node test). Observation: `model-tier-drop` listed.
- **R-43** (functional) `adapters/README.md` states how a Pi adapter differs: Pi has no MCP client inside the agent, so tools come from an extension in `~/.pi/agent/extensions/*.ts` calling the CLI, the hosted MCP is reached through `mcp-remote` in `~/.pi/mcp.json`, and the skill is symlinked into `~/.pi/agent/skills`.
  - T-43 Action: read the section. Fails if any of the three differences is missing or if it claims a Pi adapter was built.

### E5 Delivery

- **R-50** (critical, delivery) `.github/workflows/ci.yml` runs `cargo fmt --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`, the Worker wasm build, `pnpm --dir adapters/dsh check` and `gitleaks detect --redact` on every push and pull request.
  - T-50 Action: run each command locally from a fresh clone; the workflow file lists the same commands. Observation: all exit 0.
- **R-51** (critical, security) No secret, phone number, Cloudflare account identifier or internal hostname is committed; `gitleaks protect --staged --redact` runs before every commit and `gitleaks detect --redact` before every push; the registry example uses deployment ids, not hostnames.
  - T-51 Action: `gitleaks detect --redact --source .` on the final tree and `grep -rniE 'tail[0-9a-f]+\.ts\.ne[t]|account[_]id' --exclude-dir=target --exclude-dir=node_modules .` (the bracketed characters keep the pattern from matching its own text). Observation: no findings.
- **R-52** (critical, delivery) `README.md` lets a stranger: understand the rule in one screen, install the CLI, run the walkthrough, add the MCP to Claude Code/Codex/dsh/Pi, install the dsh adapter, and run the tests.
  - T-52 Action: a QA agent with no prior context follows the README from a fresh clone in a temporary directory. Observation: every command succeeds as written.
- **R-53** (critical, delivery) The repository is public at `github.com/bradheitmann/model-tier-drop` with `LICENSE` (Apache-2.0), and `main` contains only QA-accepted merges.
  - T-53 Action: `gh repo view bradheitmann/model-tier-drop --json visibility,licenseInfo`. Observation: `PUBLIC`, `Apache License 2.0`.
- **R-54** (delivery) `plan/graph.json` is a `reveng.graph/1` graph containing every requirement and test above, the slices of `plan/contracts.md` as typed `Slice` payloads with paired verification, and it compiles with the REVENG core (`reveng compile`) to `plan/build.json` deterministically.
  - T-54 Action: compile twice into two directories and compare bytes. Observation: identical; `build.json` lists every implementation slice and its verifier; selected requirements equal the R-ids in this file.
- **R-55** (compliance) No file contains the owner's forbidden organisation name (grep pattern `O[K]OA`) or the forbidden phrase (grep pattern `needs[ ]you`); no file introduces Python or plain JavaScript (generated `build/` and `dist/` excepted).
  - T-55 Action: `grep -rnE "O[K]OA" .`, `grep -rnE "needs[ ]you" .`, `lang-guard scan .` (excluding `target`, `node_modules`, `build`, `dist`). Observation: no matches.

## Non-functional requirements and cross-cutting dispositions

| Concern | Disposition |
|---|---|
| Security | R-34, R-51. No auth on the hosted MCP by design (stateless, nothing stored). |
| Data lifecycle | R-16, R-19: append-only ledger, rights carried on every attempt, export filtered by purpose. Deletion is a local file operation documented in the skill. |
| Telemetry / observability | Worker `observability.enabled` (Cloudflare logs); the ledger is the engine's own wide-event record. No third-party telemetry. |
| Operations | Deploy and rollback documented in `crates/worker/README.md` (`wrangler deploy`, `wrangler rollback`). |
| Performance | Engine operations are O(models × levels) per plan; the Worker has a 256 KiB body cap. No performance targets are claimed beyond the tests passing. |
| Accessibility / design | Not applicable: no user interface. |
| Compliance | R-22 strict review default; R-55 owner rules. |

## Release boundary

One release, `0.1.0`, containing everything above. Deferred and explicitly out of scope: a Pi adapter implementation (described only, R-43), a shared remote ledger, model execution inside the engine, automatic QoS measurement (QoS values are supplied by the operator or computed by the harness from attempts).
