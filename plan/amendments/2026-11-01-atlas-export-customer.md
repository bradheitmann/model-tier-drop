# Amendment 2026-11-01: reading registry facts from a pinned model-atlas export

Status: proposed by PM-MODEL-TIER-DROP. Binds slice S-12 / V-12.

## Need

model-atlas owns model facts; tier-drop owns tiers (seam agreed 2026-10-31 with PM-MODEL-ATLAS: model-atlas `docs/SCHEMAS.md` §8). Today `registry/registry.example.json` carries `context_window`, `cost`, `open_weight`, `reasoning_levels` and, since S-10, `lineage` (R-24) as configuration copied by hand. The agreed binding is a ref (`atlas_ref: "model-atlas:<id>@<export_version>"`) resolved by a CLI step from a pinned export file, never inside the pure core. S-12 builds the mapping and proves it against a real published export. It is also tier-drop's customer test in the polyswarm's MODEL-ATLAS focus (polyswarm protocol v1.2, "Scoring" §3).

## Rules

- The input is one `model-atlas.export/1` file (model-atlas `docs/SCHEMAS.md` §7), vendored as a test fixture under `crates/cli/tests/fixtures/` with its sha256 asserted by the test. No live dependency on model-atlas code, and no network.
- The mapping is a pure function in `crates/cli` (`atlas` module): `resolve(card, route?) -> ResolvedFacts`, where each field is either `Known(value, source_id, last_verified)` or `Unknown(reason)`:
  - `context_window` from the route's `context_offered_tokens` when a route is given, else the card's `limits.context_tokens`;
  - `open_weight` from the card;
  - `reasoning_levels` from the card;
  - `lineage` from the card's `lineage.maker_id` (model-atlas schema v1.4, a closed stable slug) and `lineage.architecture`, formatted `"<maker>/<architecture>"`. Until an export carries `maker_id`, the free-text `lineage.maker` is used, and the resolved fact carries `maker_free_text: true`. A lineage that model-atlas marks `unknown` resolves to Unknown and is never invented as a distinct family; S-10's `adjacent` already treats it as `lineage_known: false`;
  - `cost` from the route's `tariff` (`input_usd_per_mtok`, `output_usd_per_mtok`). When the export has no route for the card, the result is `Unknown("no route in export")`, never zero; `context_window` then falls back to the card.
- An `unknown` fact stays `Unknown` with its reason. It never becomes zero. This matters most for an unpriced tariff, because a zero cost would rank the model first.
- A fact that is missing entirely (neither `value` nor `unknown`) is an issue `atlas_fact_missing`, naming the record id and the field.
- A record with `"stale": true` resolves with a `stale` flag on every fact (model-atlas G2). It is never reported as current.
- Every `Known` fact's `source` id resolves inside the export's `sources`; otherwise the issue is `atlas_source_unresolved`.
- Paths the pinned export does not exercise are tested on DERIVED copies built inside the test from the pinned bytes, so the pinned file and its sha256 stay untouched. Export #1 (lane public-export-1) has one card, three providers, no routes and nothing stale, so the unpriced-route and stale-record paths use derived copies. When a later export carries a genuinely unpriced route, the next pin refresh tests it on the real file. (PM-MODEL-ATLAS, 2026-11-01.)
- Out of scope: writing a registry file (`tier-drop registry resolve` as a command) and choosing tiers.

## Pin refresh

The pin is refreshed once per focus window from PM-MODEL-ATLAS's announced export (`export_version`, sha256, commit). A refresh replaces the fixture and the asserted sha256 in one commit.

## Addendum 2026-11-02: pin refresh to export 2026.11.01-4 (binds S-13 / V-13)

Export 2026.11.01-4 (sha256 227b03988104c7f386a810a9d40e1deb5e635226c1de2b5b5108765cd41e75df, producing commit 7435bbd) carries 4 cards, 4 providers and 5 routes with list tariffs, one free route with an implied price, and `lineage.maker_id` slugs. The refresh adds these rules:
- **Routes:** a card's cost comes from its route(s) in the export (matched by `route.card`). When there are several routes, all are returned, each with its route id, `cost_basis` (`list` / `implied` / `actual`) and `is_free`. Routes with no card (`card` absent or `card_unknown`) are resolved by route id only.
- **Implied price:** a route with `cost_basis = implied` resolves to `Known(implied price)` with `is_free = true` and `cost_basis = implied`. An implied price of 0 is the issue `atlas_implied_price_zero` (model-atlas D-A, fixed in -4). It must not be taken as a real zero.
- **Lineage:** `lineage.maker_id` is the family. When `lineage.architecture` is unknown (every card in -4), lineage resolves to `Known(<maker_id>)` with `architecture: Unknown(<reason>)`. This replaces the `"<maker>/<architecture>"` format, which would have thrown away the known maker. When both are known: `"<maker_id>/<architecture>"`.
- **Renamed route ids:** `route.deepseek.deepseek-v4-flash` is now `route.deepseek.deepseek-flash`. Atlas refs use the new id.
- Unpriced-route and stale-record paths stay on derived copies, because -4 has neither (PM-MODEL-ATLAS: never invented).
