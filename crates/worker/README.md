# tier-drop-worker: the model-tier-drop MCP server

A Cloudflare Worker written in Rust (workers-rs, compiled to WebAssembly). It serves the
model-tier-drop skill and the engine's pure operations over MCP: Streamable HTTP, JSON
responses, stateless. It calls no model and stores nothing. There are no secrets, no bindings,
no storage and no auth; every tool is a pure function of the records the caller passes in.

## Routes

| Route | Answer |
|---|---|
| `POST /mcp` | JSON-RPC 2.0: `initialize`, `server/discover`, `ping`, `tools/list`, `tools/call`; notifications get 202 with no body |
| `GET /` | `{ name, version, mcp, skill }` |
| `GET /skill/SKILL.md` | the skill, byte for byte as in `skill/SKILL.md` |
| `GET /skill/references/<name>.md` | `composition`, `dataset`, `eval-harness`, `registry`, `routing-record`, `tiers` |
| `GET /registry/example.json` | `registry/registry.example.json` |
| `GET /policy/default.json` | `registry/policy.default.json` |
| `GET /schemas/<name>.json` | `attempt`, `disqualification`, `eval_item`, `eval_run`, `eval_split`, `plan`, `policy`, `registry`, `routing_decision`, `task` |
| `GET /robots.txt` | `Disallow: /` |

Anything else is a JSON 404. The files are embedded at compile time with `include_str!`.

Tools (`tools/list` returns `{ tools, ttlMs: 3600000, cacheScope: "public" }`; every
`inputSchema` has `additionalProperties: false`): `skill_read`, `registry_example`,
`registry_validate`, `qos_gate`, `plan_next`, `check_floors`, `validate_attempt`,
`assess_disqualification`, `decide`, `split_holdout`, `summarise_eval`, `schemas`.
A tool result carries `content[0].text` (the JSON text) and `structuredContent`. Bad arguments
(unknown fields, wrong types, an invalid task or registry for the routing tools) give a JSON-RPC
200 with `isError: true` and `structuredContent: { ok: false, error: { code, message } }`.
Validation outcomes (`registry_validate`, `validate_attempt`, `check_floors`) are results, not
errors.

Refusals on `/mcp`: a browser `Origin` other than the Worker's own (403), an `Accept` without
`application/json` or `*/*` (406), a `Content-Type` other than `application/json` (415), a body
over 262144 bytes (413), a batch (400, -32600), an unknown method (404, -32601), a non-POST
method (405). Every response carries `X-Robots-Tag: noindex, nofollow`,
`X-Content-Type-Options: nosniff` and `Referrer-Policy: no-referrer`; `/mcp` answers with
`Cache-Control: no-store`, the text routes with `public, max-age=300`.

## Build and test

From `crates/worker`:

```sh
cargo fmt --all --check
cargo clippy --target wasm32-unknown-unknown -- -D warnings
cargo test                       # host-target unit tests of routing, JSON-RPC and tools
worker-build --release           # writes build/ (generated, ignored by git)
```

## Run locally

```sh
wrangler dev --port 8787 --compatibility-date 2026-06-10
```

`wrangler.jsonc` sets `compatibility_date: 2026-10-16`. The local workerd bundled with wrangler
4.98.0 supports dates up to 2026-06-10 only, so local runs override the date on the command line;
the deployed Worker uses the file's date. Under `wrangler dev` the request URL carries the
custom-domain host from `routes`, so `GET /` names `http://model-tier-drop-mcp.bradheitmann.ai`
and the Origin check compares against that host.

## Curl transcript (`wrangler dev`, local)

```sh
U=http://localhost:8787
H=(-H 'Content-Type: application/json' -H 'Accept: application/json, text/event-stream')

curl -s "${H[@]}" $U/mcp -d '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25"}}' \
  | jq -c '{v:.result.protocolVersion,name:.result.serverInfo.name}'
# {"v":"2025-11-25","name":"model-tier-drop"}

curl -s -o /dev/null -w '%{http_code}\n' "${H[@]}" $U/mcp -d '{"jsonrpc":"2.0","method":"notifications/initialized"}'
# 202

curl -s "${H[@]}" $U/mcp -d '{"jsonrpc":"2.0","id":3,"method":"tools/list"}' \
  | jq -c '.result | {n:(.tools|length), ttlMs, cacheScope, closed:([.tools[].inputSchema.additionalProperties==false]|all)}'
# {"n":12,"ttlMs":3600000,"cacheScope":"public","closed":true}

# plan_next with a classify task and the registry from registry_example:
#   {"isError":false,"kind":"try","tier":"system_one","candidates":["jev@none","laya@none"]}
# validate_attempt with recorded_at before the task's created_at:
#   {"isError":false,"valid":false,"codes":["recorded_before_task_created"]}
# decide with garbage arguments: HTTP 200,
#   {"isError":true,"sc":{"ok":false,"error":{"code":"invalid_arguments","message":"unknown field `bogus`, …"}}}

curl -s $U/skill/SKILL.md | shasum -a 256; shasum -a 256 ../../skill/SKILL.md   # equal

curl -s -o /dev/null -w '%{http_code}\n' "${H[@]}" -H 'Origin: https://evil.example' $U/mcp -d '{"jsonrpc":"2.0","id":8,"method":"ping"}'   # 403
head -c 307200 /dev/zero | tr '\0' ' ' > /tmp/big.json
curl -s -o /dev/null -w '%{http_code}\n' "${H[@]}" $U/mcp --data-binary @/tmp/big.json                                              # 413
curl -s -o /dev/null -w '%{http_code}\n' -H 'Content-Type: text/plain' $U/mcp -d '{"jsonrpc":"2.0","id":10,"method":"ping"}'        # 415
curl -s -w ' %{http_code}\n' "${H[@]}" $U/mcp -d '[{"jsonrpc":"2.0","id":11,"method":"ping"}]'                                       # -32600 400
curl -s -w ' %{http_code}\n' "${H[@]}" $U/mcp -d '{"jsonrpc":"2.0","id":12,"method":"tools/frobnicate"}'                             # -32601 404
curl -s $U/schemas/routing_decision.json | jq .title                                                                                  # "RoutingDecision"
```

## Deploy and roll back

From `crates/worker`, with credentials for the Cloudflare account that owns the
`bradheitmann.ai` zone:

```sh
wrangler deploy      # runs build.command (worker-build --release), then uploads
wrangler rollback    # returns to the previous deployment
wrangler deployments list
```

`wrangler.jsonc` holds no account id, sets `workers_dev: false` and `preview_urls: false`, and
routes the custom domain `model-tier-drop-mcp.bradheitmann.ai`. No secrets or bindings exist, so
there is nothing to configure with `wrangler secret`.

Credentials: a `wrangler login` (OAuth) works when that login belongs to the right account. On a
machine where several agents share one wrangler login, the login can change under you; the
release of 2026-10-19 hit exactly that (an OAuth login for another account answered
`Authentication error [code: 10000]`). The dependable way is an account API token with
*Workers Scripts: Edit* (and zone DNS edit for the custom domain), supplied through the
environment and never written to disk or this repository:

```sh
# CLOUDFLARE_API_TOKEN and CLOUDFLARE_ACCOUNT_ID come from a secret manager (for example
# `doppler run --project <project> --config <config> -- wrangler deploy`).
wrangler deploy
```

A token that lacks *Workers Scripts: Edit* fails with `No access to the specified resource` on
`/workers/scripts/<name>/deployments`; that is a permission problem, not a build problem.

## Live deployment (2026-10-19)

`https://model-tier-drop-mcp.bradheitmann.ai` (custom domain, `observability.enabled`). Upload
1148 KiB (gzip 359 KiB), startup 4 ms. Verified from a plain shell right after the deploy
(identifiers redacted; the full commands are the ones in the local transcript above with
`U=https://model-tier-drop-mcp.bradheitmann.ai`):

```text
GET /                       200  {"name":"model-tier-drop","version":"0.1.0","mcp":".../mcp","skill":".../skill/SKILL.md"}
initialize 2025-11-25       200  protocolVersion 2025-11-25, serverInfo.name model-tier-drop, capabilities.tools.listChanged false
tools/list                  200  12 tools, ttlMs 3600000, cacheScope "public", every inputSchema.additionalProperties false
tools/call registry_example 200  as_of 2026-10-19, 15 models
tools/call plan_next        200  isError false, kind "try", tier "system_one", candidates [jev, laya], skipped [tailored:no_provisioned_candidates]
GET /skill/SKILL.md         200  sha256 6627c366…54727c, equal to skill/SKILL.md in the repository
Origin https://evil.example 403  Origin denied
Origin (own)                200
ping with _meta 2026-07-28  200  {"result":{"resultType":"complete"}}
```

Roll back with `wrangler rollback` from this directory using the same credentials.
