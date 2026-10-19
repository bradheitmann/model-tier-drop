# dsh-tier-drop

A plugin for dsh, the DeepSeek harness (`@deepseek-ai/dsh`). It gives a dsh agent five local tools that run the `tier-drop` command line tool and write your ledger. `cordis.patch.yml` mounts the plugin and the hosted MCP server in one patch. Pair it with the `model-tier-drop` skill, which tells the agent when and how to call the tools.

Tested with dsh `0.1.6-alpha.2`, Cordis `4.0.2`, `@deepseek-ai/dsh-tools` `0.1.6-alpha.2`, `@deepseek-ai/schemastery` `3.18.2`, TypeScript `6.0.3`, Node `26.11.1` and pnpm `12.11.2`. The package needs Node 24 or newer. The tests run TypeScript files directly through Node's built-in type stripping, which needs Node 23.6 or newer.

## Install

From a clone of the repository (`<repo>` below is its absolute path, `<p>` your dsh profile):

1. Install the CLI: `cargo install --path crates/cli`. Check with `tier-drop schemas routing_decision`.
2. Build the plugin: `pnpm --dir adapters/dsh install && pnpm --dir adapters/dsh build`.
3. Add it to the profile: `npx -y @deepseek-ai/dsh plugin --profile <p> add <repo>/adapters/dsh`. This runs `pnpm add` in `~/.dsh/profiles/<p>`. Because `package.json` declares `dsh.bundle.patch`, it also adds `dsh-tier-drop` to the profile's `dsh.profile.bundles`. Both rows of `cordis.patch.yml` then load on every boot of that profile, so you do not have to copy them.
4. Point the plugin at your files. The shipped rows use placeholder paths (`.tier-drop/ledger`, `.tier-drop/registry.json`, relative to the directory dsh starts in). Override them by id in `~/.dsh/profiles/<p>/cordis.patch.yml`:
   ```yaml
   - id: tier-drop
     config:
       binary: /absolute/path/to/tier-drop      # or a bare name on PATH
       ledgerDir: /absolute/path/to/ledger
       registryPath: /absolute/path/to/registry.json
       # policyPath: /absolute/path/to/policy.json
       timeoutMs: 30000
   ```
   Write the registry first: `tier-drop registry example > registry.json`, then edit it to match your own deployments.
   If the package is in the profile but not in its bundle list (for example after a plain `pnpm add`), append the two rows from `cordis.patch.yml` to the profile patch, or pass `--patch <repo>/adapters/dsh/cordis.patch.yml` when you start dsh. Repeating the rows when the bundle is already listed does no harm, because the loader keys entries by id and the last row with a given id wins. It is redundant, though.
5. Install the skill: `ln -s <repo>/skill ~/.dsh/skills/model-tier-drop`. dsh also reads `~/.agents/skills`.
6. Verify: `npx -y @deepseek-ai/dsh --profile <p> --dump-config | grep -A8 tier-drop` shows both rows and your overrides. The dump composes the patch layers without importing any plugin. The first real boot imports the plugin.

To use only the hosted tools, skip steps 1, 2 and 4, add the `mcp-tier-drop` row alone to the profile patch, and delete the `tier-drop` row.

`pnpm add` installs a local directory as a `link:` dependency and warns that a linked package does not take its peer dependencies from the profile. The plugin then loads `@deepseek-ai/dsh-tools` and `@deepseek-ai/schemastery` from `adapters/dsh/node_modules`, at the same pinned versions. It uses only `defineTool`, which returns a plain definition object, and the registry is reached through `ctx.tools`. A real boot registered all five tools (see Verified).

## Tools

| Tool | Arguments | CLI call | Returns |
|---|---|---|---|
| `tier_drop_plan` | `task` (Task JSON) | `tier-drop plan --task … --registry … --ledger … [--policy …]` | Plan: `{ kind: "try", tier, candidates, skipped, detail }` or `{ kind: "exhausted", skipped }` |
| `tier_drop_qos` | `task` | `tier-drop qos --task … --registry … [--policy …]` | `{ qualified, excluded }` |
| `tier_drop_record_attempt` | `task`, `attempt` (Attempt JSON) | `tier-drop attempt record --ledger … --task … --registry … [--policy …]`, attempt on stdin | `{ floor_result, disqualification }` |
| `tier_drop_decide` | `task` | `tier-drop decide --task … --registry … --ledger … [--policy …]` | RoutingDecision, also appended to the ledger |
| `tier_drop_ledger_show` | `family?`, `kind?` (`attempts`, `decisions`, `disqualifications`, `eval-runs`, `eval-splits`, `tasks`) | `tier-drop ledger show --ledger … [--family …] [--kind …]` | records keyed by kind |

`tier_drop_plan`, `tier_drop_qos` and `tier_drop_ledger_show` only read, so they may run in parallel with other tools. `tier_drop_record_attempt` and `tier_drop_decide` append to the ledger, so dsh runs them one at a time.

When the CLI exits 0, a tool returns the CLI's JSON unchanged. Anything else returns `{ ok: false, exit, error: { code, message }, output }`. `exit` is the CLI exit code, or `null` when the process did not exit normally. `output` is the CLI's JSON when it printed any. A floor failure exits 1 and is still a valid, recorded result. Its `floor_result` is in `output` and `error.code` is `floors_not_met`. Other codes: the CLI's own error code when it printed one, `validation_failed` (the CLI listed issues), `usage_error` (exit 2), `binary_not_found`, `spawn_failed`, `timeout`, `aborted`, `output_too_large` (over 8 MiB) and `invalid_output` (exit 0 without JSON).

The hosted row adds the MCP tools as `mcp__tier-drop__<name>`: `skill_read`, `registry_example`, `registry_validate`, `qos_gate`, `plan_next`, `check_floors`, `assess_disqualification`, `validate_attempt`, `decide`, `split_holdout`, `summarise_eval` and `schemas`. They are stateless and store nothing. You pass the registry and the ledger records with each call.

## Trust boundary

- The ledger directory, registry, policy and executable come only from the plugin config in the profile patch. Tool arguments never name a path. A call whose arguments contain any of `ledgerDir`, `registryPath`, `policyPath`, `binary`, `ledger`, `registry` or `policy` is refused with `error.code: "path_argument_rejected"` before anything runs.
- Config is validated when the plugin loads. Empty strings are refused. Paths are resolved to absolute paths against the dsh process working directory. A `binary` without a path separator stays a bare name and is looked up on `PATH`.
- The CLI runs through `execFile` without a shell, with a timeout (`timeoutMs`, default 30000) and an 8 MiB output limit. Values from tool arguments go to the CLI as `--flag=value`, so a value starting with `-` cannot pass as another flag.
- The task JSON is written to a private file (mode 0600) in a new directory under the system temporary directory. The directory is removed when the call ends. The attempt goes to the CLI on stdin.
- The plugin never calls a model. The engine plans and records. The agent runs each model call through dsh and reports the attempt back.

## Development

`pnpm --dir adapters/dsh check` runs `tsc --noEmit` over `src/` (and `tests/` through `tsconfig.tests.json`), builds `dist/` and runs `node --test tests/*.test.ts`. The tests use a fake `ctx` that captures `ctx.tools.register` calls. The integration tests use the real binary at `<repo>/target/debug/tier-drop` (`cargo build -p tier-drop-cli`) and are skipped, with a message, when it is absent. `skipLibCheck` is on because the dsh declaration files import packages this plugin does not install (for example the dsh agent and session packages).

## Verified

Run on 2026-10-19 from the `s07` worktree, macOS, Node 26.11.1, pnpm 12.11.2. `$DSH_HOME` stands for a fresh `mktemp -d` directory, and `$A` stands for the absolute path to `adapters/dsh`. The live dsh home `~/.dsh` was never touched: `stat -f %m ~/.dsh/profiles/headless/cordis.patch.yml` printed `1792305139` before and after.

Composition (R-41):

```
$ export DSH_HOME=$(mktemp -d)
$ npx -y @deepseek-ai/dsh@0.1.6-alpha.2 --profile tdcheck --from-default-profile headless --dump-config > /dev/null
exit 0   # creates $DSH_HOME/profiles/tdcheck with bundles @deepseek-ai/dsh-base, @deepseek-ai/dsh-headless
$ npx -y @deepseek-ai/dsh@0.1.6-alpha.2 plugin --profile tdcheck add $A
[WARN] The package dsh-tier-drop, which you have just pnpm linked, has the following peerDependencies ...
dependencies:
+ dsh-tier-drop <- $A
exit 0   # package.json now lists "dsh-tier-drop": "link:$A" and bundles [..., "dsh-tier-drop"]
$ npx -y @deepseek-ai/dsh@0.1.6-alpha.2 --profile tdcheck --patch $A/cordis.patch.yml --dump-config
exit 0, nothing on stderr; excerpt:
# == dsh-tier-drop
- id: tier-drop
  name: dsh-tier-drop
  config:
    binary: !!js process.env.TIER_DROP_BIN ?? 'tier-drop'
    ledgerDir: .tier-drop/ledger
    registryPath: .tier-drop/registry.json
    timeoutMs: 30000
- id: mcp-tier-drop
  name: '@deepseek-ai/dsh-mcp-client'
  config:
    serverName: tier-drop
    transport: streamable-http
    url: https://model-tier-drop-mcp.bradheitmann.ai/mcp
    failOnStartupError: false
# == $A/cordis.patch.yml
(the same two rows again, from the --patch overlay)
```

The bundle layer (`# == dsh-tier-drop`) already supplies both rows after `plugin add`. Without `--patch` the dump is identical except that the overlay layer is missing. An id-targeted override passed with `--patch` is shown as `# == dsh-tier-drop, patched by <file>`, with the overridden `binary`, `ledgerDir` and `registryPath`.

Real boot (the dump does not import plugins). Run with a clean environment and no model credentials, so the turn stops before any model call:

```
$ env -i HOME=$HOME PATH=$PATH DSH_HOME=$DSH_HOME TIER_DROP_BIN=<repo>/target/debug/tier-drop \
    npx -y @deepseek-ai/dsh@0.1.6-alpha.2 --profile tdcheck --json "list your tools"
{"type":"status","phase":"turn_end","turn":1,"reason":{"kind":"error","error":{"message":"llm-deepseek: no API key ...","code":"MISSING_CREDENTIAL"}}}
exit 1; stderr holds only the MISSING_CREDENTIAL line, with no loader error
$ zstd -dc $DSH_HOME/sessions/*/session-*/session.v3.jsonl.zstd | grep -oE '"name":"tier_drop_[a-z_]+"' | sort -u
"name":"tier_drop_decide"
"name":"tier_drop_ledger_show"
"name":"tier_drop_plan"
"name":"tier_drop_qos"
"name":"tier_drop_record_attempt"
```

The real loader imported `dsh-tier-drop` and registered all five tools. No `mcp__tier-drop__*` tools appeared, because the hosted URL did not answer on 2026-10-19 (`curl` timed out; the Worker is deployed in slice S-06). dsh booted anyway because the row sets `failOnStartupError: false`. Repeat this check once the server is live.

Skill discovery (R-42):

```
$ mkdir -p $DSH_HOME/skills && ln -s <repo>/skill $DSH_HOME/skills/model-tier-drop
$ (same boot command, task "list your skills")
$ for each session log: zstd -dc … | grep -c model-tier-drop
session before the symlink: 0
session after the symlink:  1
$ zstd -dc <after> | grep -oE '.{0,60}model-tier-drop.{0,80}'
…{"name":"model-tier-drop","description":"Route any model-using task by tier drop, cheapest tier first, …
…- `model-tier-drop`: Route any model-using task by tier drop, cheapest tier first, moving up only after …
```

dsh 0.1.6-alpha.2 has no command that lists skills. The evidence is the session's recorded skill catalogue, which `@deepseek-ai/dsh-skill-filesystem` builds from `$DSH_HOME/skills`. Before the symlink the catalogue held other skills from `~/.agents/skills` but not `model-tier-drop`. The temporary `$DSH_HOME` was removed afterwards.
