import assert from 'node:assert/strict'
import { existsSync } from 'node:fs'
import { mkdtemp, rm, writeFile } from 'node:fs/promises'
import os from 'node:os'
import path from 'node:path'
import { after, before, describe, test } from 'node:test'
import { fileURLToPath } from 'node:url'
import type { Context } from '@deepseek-ai/cordis'
import type { ToolDefinition, ToolRunContext } from '@deepseek-ai/dsh-tools'

import { apply, describeTools, runCli, trustedConfig, type Config } from '../src/index.ts'

const here = path.dirname(fileURLToPath(import.meta.url))
const BINARY = path.resolve(here, '../../../target/debug/tier-drop')
const HAVE_BINARY = existsSync(BINARY)
const SKIP_BINARY = HAVE_BINARY ? false : `tier-drop binary absent at ${BINARY}; run \`cargo build -p tier-drop-cli\` from the repository root`

/** A fake Cordis context exposing only what `apply` uses: `ctx.effect` and `ctx.tools.register`. */
function fakeContext(): { ctx: Context; tools: Map<string, ToolDefinition> } {
  const tools = new Map<string, ToolDefinition>()
  const ctx = {
    tools: {
      register(definition: ToolDefinition): () => void {
        assert.ok(!tools.has(definition.name), `duplicate registration ${definition.name}`)
        tools.set(definition.name, definition)
        return () => { tools.delete(definition.name) }
      },
    },
    effect(body: () => Iterable<() => void> | (() => void)): () => void {
      const result = body()
      const disposers = typeof result === 'function' ? [result] : [...result]
      return () => { for (const dispose of disposers.reverse()) dispose() }
    },
  }
  return { ctx: ctx as unknown as Context, tools }
}

function toolContext(): ToolRunContext {
  return { signal: new AbortController().signal } as unknown as ToolRunContext
}

function tool(tools: Map<string, ToolDefinition>, toolName: string): ToolDefinition {
  const found = tools.get(toolName)
  assert.ok(found, `tool ${toolName} registered`)
  return found
}

type Obj = Record<string, unknown>

function obj(value: unknown): Obj {
  assert.ok(typeof value === 'object' && value !== null && !Array.isArray(value), `expected an object, got ${JSON.stringify(value)}`)
  return value as Obj
}

// The README walkthrough task and its two attempts, verbatim.
const TASK = {
  task_id: 'ticket-4711',
  family: 'ticket.priority',
  shape: 'classify',
  input_sha256: '6708cfa2dde6152d5035c74ab58c9e623324ec1abf2435431c2e4296a945e4ef',
  rights: { process: true, train: true, send_remote: false, export: true },
  floors: [
    { metric: 'quality', min: 0.8 },
    { metric: 'accuracy', min: 0.9 },
    { metric: 'completeness', min: 0.9 },
  ],
  qos: { availability_min: 0.99, p95_latency_max_ms: 5000, error_rate_max: 0.02 },
  review: 'strict',
  metadata: {},
  created_at: '2026-10-19T09:00:00Z',
}

const ATTEMPT_PASS = {
  task_id: 'ticket-4711',
  family: 'ticket.priority',
  tier: 'system_one',
  candidate: { kind: 'single', model_id: 'jev', reasoning_level: 'none' },
  outcome: 'answered',
  metrics: { quality: 0.92, accuracy: 1.0, completeness: 1.0 },
  judge: { kind: 'deterministic', ref: 'label-exact-match' },
  cost_usd: 0.0,
  latency_ms: 42,
  tokens: { input: 180, output: 3 },
  input_sha256: '6708cfa2dde6152d5035c74ab58c9e623324ec1abf2435431c2e4296a945e4ef',
  output_sha256: '3946ca64ff78d93ca61090a437cbb6b3d2ca0d488f5f9ccf3059608368b27693',
  rights: { process: true, train: true, send_remote: false, export: true },
  eval_item_id: null,
}

const ATTEMPT_ABSTAIN = {
  ...ATTEMPT_PASS,
  candidate: { kind: 'single', model_id: 'laya', reasoning_level: 'none' },
  outcome: 'abstained',
  metrics: {},
  latency_ms: 55,
  tokens: { input: 180, output: 0 },
  output_sha256: null,
}

const PLACEHOLDER_CONFIG: Config = { binary: 'tier-drop', ledgerDir: 'ledger', registryPath: 'registry.json' }

describe('registration', () => {
  test('apply registers exactly the five tier-drop tools', () => {
    const { ctx, tools } = fakeContext()
    apply(ctx, PLACEHOLDER_CONFIG)
    assert.deepEqual([...tools.keys()], ['tier_drop_plan', 'tier_drop_qos', 'tier_drop_record_attempt', 'tier_drop_decide', 'tier_drop_ledger_show'])
    assert.deepEqual([...tools.keys()], describeTools().map(t => t.name))
    for (const t of describeTools()) assert.equal(tool(tools, t.name).description, t.description)
  })

  test('no tool declares a path parameter', () => {
    const { ctx, tools } = fakeContext()
    apply(ctx, PLACEHOLDER_CONFIG)
    const declared = [...tools.values()].flatMap(t => Object.keys(t.parameters.properties ?? {}))
    assert.deepEqual([...new Set(declared)].sort(), ['attempt', 'family', 'kind', 'task'])
  })

  test('trustedConfig rejects empty paths and resolves relative ones', () => {
    assert.throws(() => trustedConfig({ ...PLACEHOLDER_CONFIG, ledgerDir: '' }), /ledgerDir/)
    assert.throws(() => trustedConfig({ ...PLACEHOLDER_CONFIG, registryPath: '  ' }), /registryPath/)
    assert.throws(() => trustedConfig({ ...PLACEHOLDER_CONFIG, binary: '' }), /binary/)
    assert.throws(() => trustedConfig({ ...PLACEHOLDER_CONFIG, policyPath: '' }), /policyPath/)
    assert.throws(() => trustedConfig({ ...PLACEHOLDER_CONFIG, timeoutMs: 0 }), /timeoutMs/)
    const trusted = trustedConfig(PLACEHOLDER_CONFIG)
    assert.equal(trusted.binary, 'tier-drop')
    assert.equal(trusted.ledgerDir, path.resolve('ledger'))
    assert.equal(trusted.timeoutMs, 30000)
    assert.equal(trustedConfig({ ...PLACEHOLDER_CONFIG, binary: './bin/tier-drop' }).binary, path.resolve('bin/tier-drop'))
  })
})

describe('argument trust boundary', () => {
  test('an argument object carrying ledgerDir is rejected before the CLI runs', async () => {
    const { ctx, tools } = fakeContext()
    // A binary that cannot exist: if the rejection did not happen first, the error would be binary_not_found.
    apply(ctx, { ...PLACEHOLDER_CONFIG, binary: '/nonexistent/tier-drop' })
    const result = obj(await tool(tools, 'tier_drop_plan').execute({ task: TASK, ledgerDir: '/tmp/elsewhere' }, toolContext()))
    assert.equal(result['ok'], false)
    assert.equal(obj(result['error'])['code'], 'path_argument_rejected')
    assert.match(String(obj(result['error'])['message']), /ledgerDir/)
  })

  test('every path-like key is rejected on every tool', async () => {
    const { ctx, tools } = fakeContext()
    apply(ctx, { ...PLACEHOLDER_CONFIG, binary: '/nonexistent/tier-drop' })
    for (const key of ['ledgerDir', 'registryPath', 'policyPath', 'binary', 'ledger', 'registry', 'policy']) {
      for (const t of tools.values()) {
        const args = t.name === 'tier_drop_ledger_show' ? { [key]: 'x' } : { task: TASK, attempt: ATTEMPT_PASS, [key]: 'x' }
        const result = obj(await t.execute(args, toolContext()))
        assert.equal(obj(result['error'])['code'], 'path_argument_rejected', `${t.name} with ${key}`)
      }
    }
  })

  test('a task that is not an object is rejected', async () => {
    const { ctx, tools } = fakeContext()
    apply(ctx, { ...PLACEHOLDER_CONFIG, binary: '/nonexistent/tier-drop' })
    const result = obj(await tool(tools, 'tier_drop_qos').execute({ task: 'task.json' }, toolContext()))
    assert.equal(obj(result['error'])['code'], 'invalid_argument')
  })
})

describe('runCli', () => {
  test('a bad binary path returns an error object instead of throwing', async () => {
    const result = await runCli('/nonexistent/dir/tier-drop', ['plan'], null, 5000)
    assert.equal(result.ok, false)
    if (!result.ok) {
      assert.equal(result.exit, null)
      assert.equal(result.error.code, 'binary_not_found')
    }
  })

  test('a registered tool with a bad binary returns the same error object', async () => {
    const { ctx, tools } = fakeContext()
    apply(ctx, { ...PLACEHOLDER_CONFIG, binary: '/nonexistent/dir/tier-drop' })
    const result = obj(await tool(tools, 'tier_drop_ledger_show').execute({}, toolContext()))
    assert.equal(result['ok'], false)
    assert.equal(obj(result['error'])['code'], 'binary_not_found')
  })
})

describe('against the real tier-drop binary', { skip: SKIP_BINARY }, () => {
  let dir = ''
  let tools = new Map<string, ToolDefinition>()

  before(async () => {
    dir = await mkdtemp(path.join(os.tmpdir(), 'dsh-tier-drop-test-'))
    const example = await runCli(BINARY, ['registry', 'example'], null, 30000)
    assert.ok(example.ok, `registry example: ${JSON.stringify(example)}`)
    const registryPath = path.join(dir, 'registry.json')
    await writeFile(registryPath, JSON.stringify(example.output))
    const fake = fakeContext()
    apply(fake.ctx, { binary: BINARY, ledgerDir: path.join(dir, 'ledger'), registryPath })
    tools = fake.tools
  })

  after(async () => {
    if (dir) await rm(dir, { recursive: true, force: true })
  })

  test('tier_drop_qos returns the gate result', async () => {
    const result = obj(await tool(tools, 'tier_drop_qos').execute({ task: TASK }, toolContext()))
    assert.ok(Array.isArray(result['qualified']), JSON.stringify(result))
    assert.ok(Array.isArray(result['excluded']))
  })

  test('tier_drop_plan returns kind "try" in system_one', async () => {
    const result = obj(await tool(tools, 'tier_drop_plan').execute({ task: TASK }, toolContext()))
    assert.equal(result['kind'], 'try', JSON.stringify(result))
    assert.equal(result['tier'], 'system_one')
  })

  test('tier_drop_record_attempt with a passing attempt returns floor_result.passed === true', async () => {
    const result = obj(await tool(tools, 'tier_drop_record_attempt').execute({ task: TASK, attempt: ATTEMPT_PASS }, toolContext()))
    assert.equal(obj(result['floor_result'])['passed'], true, JSON.stringify(result))
  })

  test('an abstained attempt exits 1, is still recorded, and its floor result is returned as output', async () => {
    const result = obj(await tool(tools, 'tier_drop_record_attempt').execute({ task: TASK, attempt: ATTEMPT_ABSTAIN }, toolContext()))
    assert.equal(result['ok'], false)
    assert.equal(result['exit'], 1)
    assert.equal(obj(result['error'])['code'], 'floors_not_met')
    assert.equal(obj(obj(result['output'])['floor_result'])['passed'], false)
  })

  test('tier_drop_decide returns a decision with a non-null winner', async () => {
    const result = obj(await tool(tools, 'tier_drop_decide').execute({ task: TASK }, toolContext()))
    assert.notEqual(result['winner'], null, JSON.stringify(result))
    assert.notEqual(result['winner'], undefined, JSON.stringify(result))
  })

  test('tier_drop_ledger_show lists the two recorded attempts for the family', async () => {
    const result = obj(await tool(tools, 'tier_drop_ledger_show').execute({ family: 'ticket.priority', kind: 'attempts' }, toolContext()))
    const attempts = result['attempts']
    assert.ok(Array.isArray(attempts), JSON.stringify(result))
    assert.equal(attempts.length, 2)
  })

  test('tier_drop_ledger_show rejects an unknown kind at the schema', async () => {
    await assert.rejects(tool(tools, 'tier_drop_ledger_show').execute({ kind: 'secrets' }, toolContext()))
  })
})
