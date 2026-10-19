# Commission (REVENG P00)

Run: `mtd-2026-10-19`. Date: 2026-10-19. Horizon: **full** (every obligation below gets an executable implementation contract and a paired verification contract). Graph storage: portable files under `plan/`, no database.

## Highest-level job

Give any agent, in any harness, one procedure and one engine for choosing a model by **tier drop**: start at the cheapest tier, move up only after that tier is disqualified for the activity on recorded evidence, and leave a routing record, an eval record and a dataset record behind every task. Publish it as an open-source skill, a Rust engine, a hosted MCP server and a dsh adapter.

## The owner's rule (restated; the original is the authority)

Every model-using task is routed by tier, cheapest first. A task may move up a tier only after the lower tier has been disqualified **for that activity**, on evidence. Before a tier is left behind, the task must show that no provisioned model and reasoning-level combination in the tier, nor a panel of them, can do the job.

Tiers (model names indicative as of 2026-10-19; tiers are compute categories, not a fixed list):

1. **System-one** decision-shaped models (Jev, Laya and similar): classify, choose among fixed options, route, yes/no.
2. **Task-specialised small models** (ScaleDown models, Docling, Qwen OCR models): small, fast, trained for exactly the job.
3. **Frontier-adjacent inexpensive generalists** (DeepSeek 4.1 Flash, Qwen 3.8 27B, GLM-5.3-flash): smart enough for the job, preferably runnable on our own equipment.
4. **Frontier and frontier-lite**: top open-weight models first (GLM-5.3, Qwen 3.8 Max, Mistral 4, Kimi K3), then Opus 5.5 and GPT-6.1-Sol.
5. **Tailored models**: anything we fine-tune, prune (REAP), train with reinforcement learning or otherwise modify to cover jobs that tiers 1 to 4 satisfy; built per high-volume task that repeats constantly and reliably.

Embedding models are not a tier. They are a helper tool (retrieval, deduplication, clustering, routing features) usable by any tier.

Routing criteria: QoS is a cutoff gate, not a weight (availability, latency, error rate below the required level removes a deployment before scoring). Among qualified candidates, routing balances (1) quality, accuracy and completeness, (2) cost per task, (3) speed. Quality floors are set per task before results are seen; cost and speed rank only candidates that clear them.

Composition patterns a tier may use before it is disqualified: panels judged by a model from the next tier up; cascades with audited pass-through; lower tiers drafting for higher tiers; embedding helpers.

What must exist from the beginning: an eval harness (per task a metric vector, held-out sets split by source family, recorded disqualification evidence per tier); dataset creation (every attempt recorded, including failures and abstentions, with rights by purpose: process, train, send to a remote model, export); a routing record per task (tiers tried, why each was disqualified, which candidate won and on what evidence).

## Related owner decisions that bind this project

- **Strict review is the default** (DECISION-D-REVIEW-PROFILE, 2026-10-19). Every assertion is judged under the strict rule. A sampled profile exists only for named low-risk classes after a measured false-clear rate within 2 % on a sealed, human-labelled set, re-measured continuously, reverting to strict automatically. Numbers, absence claims, modality changes and anything feeding a financial figure or regulated disclosure are always strict.
- **Python only inside a justified adapter** (DECISION-D-PYTHON-ADAPTER, 2026-10-19). This project has no such adapter. Everything is Rust; the dsh adapter is strict TypeScript because dsh plugins are Node modules.

## Scope boundary

In scope: the repository `model-tier-drop` (skill, engine, CLI, Worker, dsh adapter, plan, docs, CI, publication). Out of scope: calling any model from the engine (the engine plans and records; harnesses execute), changing any machine or inference server, modifying other repositories, the lw-engine-reveng amendment and the Intake roadmap placement (owned elsewhere).

## Host and capability facts (discovered, not assumed)

| Capability | Fact |
|---|---|
| Rust | toolchain pinned to 1.98.1 in `rust-toolchain.toml` (1.88.0 is the machine default but `worker` 0.8.7 requires 1.91); MSRV declared 1.91; `wasm32-unknown-unknown` target added 2026-10-19 |
| workers-rs | crate `worker` 0.8.7 and `worker-build` 0.8.7 on crates.io (checked 2026-10-19); rust-version 1.75 |
| wrangler | 4.98.0, OAuth-authenticated (scopes include workers write, workers_routes write, d1 write) |
| gitleaks | 8.30.1 |
| gh | 2.102.0, active account `bradheitmann` |
| dsh | `@deepseek-ai/dsh` 0.1.6-alpha.2 installed in the user's profiles; plugins are Cordis 4.0.2 plugin modules; profile patch layer is `cordis.patch.yml`; skills are discovered from `~/.dsh/skills/<name>/SKILL.md` and `~/.agents/skills` by `@deepseek-ai/dsh-skill-filesystem`; `@deepseek-ai/dsh-mcp-client` supports `stdio` and `streamable-http` transports |
| Pi | skills in `~/.pi/agent/skills`; tools via extensions in `~/.pi/agent/extensions/*.ts` (`pi.registerTool`); MCP via `~/.pi/mcp.json` stdio entries (`mcp-remote` for HTTP servers) |
| Reference deployment | `intake-design` Worker: stateless MCP over Streamable HTTP, JSON responses; `tools/list` carries `ttlMs` and `cacheScope` (Claude Code rejects lists without them); custom domain route; `observability.enabled` |
| Agents | Fable lead; Opus 5.5 workers; Sonnet 5.5 QA; Haiku 5.5 for narrow lookups. Workers and QA are separate contexts; a worker never certifies its own slice. Three attempts per slice gate. |
| Licences in sibling repos | mixed MIT and Apache-2.0; no consistent choice, so Apache-2.0 (the commission's default) |

## Defaults recorded instead of asked

1. **Tier 5 ordering.** A tailored model is bound to a task family. When one is provisioned for the task's family it is tried first (it is, by construction, the cheapest qualified compute for that family); otherwise the ladder is tiers 1 to 4 in order. Configurable by `policy.tailored_first`. Reason: the owner describes tier-5 models as covering jobs that tiers 1 to 4 satisfy, built per high-volume task; a literal last-position reading would make them unreachable in practice.
2. **Disqualification is per activity (task family), on evidence, and reusable.** A task instance does not re-run every lower tier live; it consults the family's standing disqualifications, which come from eval runs and recorded attempts. Evidence has a registry `as_of` binding and may expire.
3. **The engine never calls a model.** It plans the next candidates, checks floors, assesses disqualification, decides, splits held-out sets and records. Harnesses execute. This keeps the engine pure, WASM-safe and harness-neutral, and keeps inference on the served models' APIs.
4. **Hosted MCP is stateless and unauthenticated.** It serves the skill text and pure computations over caller-supplied records. Nothing is stored, so there is nothing to protect beyond body limits and origin checks. Ledger writes happen locally through the CLI or the dsh adapter.
5. **Ranking among qualified candidates** is lexicographic with indifference bands: quality (mean of the required floors' metrics) first, within `quality_band` cost per task decides, within `cost_band` latency decides. Bands live in the policy file.
6. **Licence** Apache-2.0. **Repository** public, `bradheitmann/model-tier-drop`.
7. **Worker domain** `model-tier-drop-mcp.bradheitmann.ai` (custom domain, same pattern as the reference deployment); fall back to the `workers.dev` subdomain if the custom domain cannot be attached.

## Attempt budget and verification stance

Three total submissions per slice gate. QA is a separate Sonnet context that runs the slice's checks and reads the diff. A fourth attempt is escalated in the final report, never taken silently. Nothing is reported as done without the command output that proves it.
