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

Deployment is a separate step (slice S-06), from `crates/worker`, with a wrangler login:

```sh
wrangler deploy      # runs build.command (worker-build --release), then uploads
wrangler rollback    # returns to the previous deployment
```

`wrangler.jsonc` holds no account id (wrangler takes it from the login), sets
`workers_dev: false` and `preview_urls: false`, and routes the custom domain
`model-tier-drop-mcp.bradheitmann.ai`. No secrets or bindings exist, so there is nothing to
configure with `wrangler secret`.
