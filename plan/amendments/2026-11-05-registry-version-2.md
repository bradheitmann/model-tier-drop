# Amendment 2026-11-05: registry version 2 for lineage-bearing registries

Status: proposed by PM-MODEL-TIER-DROP; ACCEPTED in principle by PM-MISTER-CLEAN (the router consumer), 2026-11-05. Binds slice S-14 / V-14 and amends S-11.

## Need

S-10 (R-24) adds an optional `lineage` to `Model` but keeps the label `tier-drop.registry/1`. The engine refuses unknown fields on `Model`, so the hosted MCP running pre-S-10 code refuses a `/1` registry that carries `lineage`, while a `/1` registry without it silently loses the `same_tier_other_lineage` move. A sender cannot tell from the label which shape a server accepts (polyswarm R14, Postel's law). mister-clean's router is the first sender that needs lineage.

## Rules

- A registry that carries `lineage` on any model says `tier-drop.registry/2`. A registry without it may say `/1` or `/2`.
- The engine accepts `/1` and `/2`. A `/1` registry that carries `lineage` is refused with an issue that names `tier-drop.registry/2`.
- Any refusal of `schema_version` lists the versions this build accepts.
- `registry/registry.example.json` says `/2`, because it carries lineage.
- The Worker and the CLI use the engine's validator; no second check.

## Order

S-14 is built on the V-10-accepted S-10 tip, before S-11, so that one deployment carries both and no deployed build ever accepts lineage under `/1`. S-11 deploys the V-14-accepted tip.
