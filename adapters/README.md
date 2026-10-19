# Harness adapters

An adapter connects one agent harness to model-tier-drop. Every adapter gives the agent three things:

1. **The skill** (`skill/` in this repository). It tells the agent when to route by tier drop and how.
2. **The hosted MCP tools** at `https://model-tier-drop-mcp.bradheitmann.ai/mcp` (Streamable HTTP, stateless, nothing stored). They plan, gate, check floors and decide over records that the caller sends with each call.
3. **Local ledger tools.** They run the `tier-drop` command line tool (`cargo install --path crates/cli`) so that attempts, decisions and disqualifications are appended to a ledger on your own disk. The hosted server never writes anything.

| Harness | Status | Where |
|---|---|---|
| dsh (DeepSeek harness) | Built and tested | [`dsh/`](dsh/README.md): a native plugin plus `cordis.patch.yml` |
| Pi | Described here, not built | See [Pi](#pi) below |
| Claude Code, Codex, other harnesses | No adapter needed | Install the skill and the MCP server as shown in `skill/SKILL.md`, and run `tier-drop` from a shell for ledger writes |

## dsh

dsh runs an MCP client inside the agent and loads plugins written as Cordis modules. One patch file covers both parts. Row `tier-drop` loads the `dsh-tier-drop` plugin, which registers `tier_drop_plan`, `tier_drop_qos`, `tier_drop_record_attempt`, `tier_drop_decide` and `tier_drop_ledger_show`. Row `mcp-tier-drop` mounts the hosted server through `@deepseek-ai/dsh-mcp-client` under `serverName: tier-drop`. The skill is a symlink: `ln -s <repo>/skill ~/.dsh/skills/model-tier-drop`. Install steps, the tool table, the trust boundary and the recorded verification are in [`dsh/README.md`](dsh/README.md).

## Pi

**No Pi adapter has been built.** This section describes how one would differ from the dsh adapter. Pi needs a different shape in three places:

1. **Local tools come from a Pi extension, not from a plugin package or the MCP client.** Pi has no MCP client inside the agent. Its tools are registered by TypeScript extensions that Pi loads from `~/.pi/agent/extensions/<name>.ts`. A Pi adapter would be one file, for example `~/.pi/agent/extensions/model-tier-drop.ts`. It would default-export a function that receives Pi's extension API and calls `pi.registerTool` once for each of the five tools (`tier_drop_plan`, `tier_drop_qos`, `tier_drop_record_attempt`, `tier_drop_decide`, `tier_drop_ledger_show`). Parameters would be declared as TypeBox schemas. Each tool's `execute(toolCallId, params, signal, …)` would shell out to the `tier-drop` CLI with `execFile`, without a shell, passing Pi's abort signal, a timeout and an output limit, and would return the CLI's JSON. This is the usual shape of a Pi extension that wraps a command line tool. The trust boundary of the dsh plugin would carry over: the ledger, registry and policy paths are fixed in the extension (or read from a config file it owns), never taken from tool parameters.
2. **The hosted MCP server is reached through `mcp-remote`.** Because Pi cannot speak Streamable HTTP MCP from inside the agent, the server is bridged as a local stdio process in `~/.pi/mcp.json`:
   ```json
   { "mcpServers": { "tier-drop": { "command": "npx", "args": ["-y", "mcp-remote", "https://model-tier-drop-mcp.bradheitmann.ai/mcp"] } } }
   ```
   In dsh the same server is a single `@deepseek-ai/dsh-mcp-client` row with `transport: streamable-http` and no bridge process.
3. **The skill is symlinked into Pi's skill directory:** `ln -s <repo>/skill ~/.pi/agent/skills/model-tier-drop`, where dsh uses `~/.dsh/skills/model-tier-drop`.

Building the Pi adapter is out of scope for release 0.1.0 (see `plan/requirements.md`, R-43).
