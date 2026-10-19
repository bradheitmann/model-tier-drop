/**
 * dsh-tier-drop: a Cordis plugin for dsh that exposes the `tier-drop` CLI as five local tools.
 * The tools write the task (and attempt) JSON to private temporary files, run the CLI without a
 * shell and return its JSON. The ledger directory, registry and policy paths come only from the
 * trusted plugin config set in a profile patch; tool arguments can never name a path.
 */
import { mkdtemp, rm, writeFile } from 'node:fs/promises'
import os from 'node:os'
import path from 'node:path'
import type { Context } from '@deepseek-ai/cordis'
import { defineTool, type ToolDefinition } from '@deepseek-ai/dsh-tools'
import Schema from '@deepseek-ai/schemastery'

import { runCli, type CliErrorInfo, type CliResult, type Json } from './cli.ts'

export { runCli, MAX_BUFFER, type CliResult, type CliFailure, type CliSuccess, type Json } from './cli.ts'

export const name = 'dsh-tier-drop'
export const inject = ['tools']

/** Loader-owned settings. Set them in the profile's cordis.patch.yml row, never from tool input. */
export interface Config {
  /** The `tier-drop` executable: a bare name looked up on PATH, or a path. */
  binary: string
  /** Ledger directory passed as `--ledger`. */
  ledgerDir: string
  /** Registry file passed as `--registry`. */
  registryPath: string
  /** Optional policy file passed as `--policy`; without it the CLI's embedded default applies. */
  policyPath?: string
  /** Per-call timeout in milliseconds. Default 30000. */
  timeoutMs?: number
}

export const DEFAULT_TIMEOUT_MS = 30000

export const Config: Schema<Config> = Schema.object({
  binary: Schema.string().required().description('The tier-drop executable: a bare name on PATH, or a path.'),
  ledgerDir: Schema.string().required().description('Ledger directory (--ledger).'),
  registryPath: Schema.string().required().description('Registry JSON file (--registry).'),
  policyPath: Schema.string().description('Optional policy JSON file (--policy).'),
  timeoutMs: Schema.natural().min(1).default(DEFAULT_TIMEOUT_MS).description('Per-call timeout in milliseconds.'),
}) as Schema<Config>

/** Config after validation: paths resolved, timeout filled. */
export interface TrustedConfig {
  binary: string
  ledgerDir: string
  registryPath: string
  policyPath?: string
  timeoutMs: number
}

function nonEmpty(value: unknown, field: string): string {
  if (typeof value !== 'string' || !value.trim()) {
    throw new TypeError(`dsh-tier-drop requires a non-empty ${field} in its trusted config`)
  }
  return value
}

/**
 * Validate and normalise the loader-supplied config. Paths are resolved against the dsh process
 * working directory.
 */
export function trustedConfig(value: Config): TrustedConfig {
  if (!value || typeof value !== 'object') throw new TypeError('dsh-tier-drop requires a config object')
  const binary = nonEmpty(value.binary, 'binary')
  const ledgerDir = path.resolve(nonEmpty(value.ledgerDir, 'ledgerDir'))
  const registryPath = path.resolve(nonEmpty(value.registryPath, 'registryPath'))
  const timeoutMs = value.timeoutMs ?? DEFAULT_TIMEOUT_MS
  if (!Number.isSafeInteger(timeoutMs) || timeoutMs < 1) {
    throw new TypeError('dsh-tier-drop timeoutMs must be a positive integer')
  }
  let policyPath: string | undefined
  if (value.policyPath !== undefined) policyPath = path.resolve(nonEmpty(value.policyPath, 'policyPath'))
  return {
    // decision: a bare command name (no path separator) stays as is so execFile looks it up on
    // PATH, as `tier-drop` installed by `cargo install` is; anything with a separator is resolved.
    binary: binary.includes('/') || binary.includes(path.sep) ? path.resolve(binary) : binary,
    ledgerDir,
    registryPath,
    ...(policyPath === undefined ? {} : { policyPath }),
    timeoutMs,
  }
}

/** Argument keys that would let a model choose a path or executable. */
export const FORBIDDEN_ARGUMENT_KEYS = ['ledgerDir', 'registryPath', 'policyPath', 'binary', 'ledger', 'registry', 'policy'] as const

/** Ledger record kinds accepted by `ledger show --kind`. */
export const LEDGER_KINDS = ['attempts', 'decisions', 'disqualifications', 'eval-runs', 'eval-splits', 'tasks'] as const

export const TOOL_DESCRIPTIONS = {
  tier_drop_plan: 'Plan the next tier for a task: the lowest open tier and its untried candidates, with the lower tiers skipped and why. Reads the trusted ledger and registry.',
  tier_drop_qos: 'Run the QoS and rights gate for a task against the trusted registry: which models qualify and why the rest were removed.',
  tier_drop_record_attempt: 'Record one attempt for a task in the trusted ledger and return its floor result, plus the disqualification it triggered, if any. A floor failure is still recorded.',
  tier_drop_decide: 'Decide the routing for a task from the ledger evidence, append the routing decision to the trusted ledger and return it.',
  tier_drop_ledger_show: 'Show records from the trusted ledger, optionally filtered by family and by kind.',
} as const

export type ToolName = keyof typeof TOOL_DESCRIPTIONS

/** The five tool names and descriptions, in registration order. */
export function describeTools(): Array<{ name: ToolName; description: string }> {
  return (Object.keys(TOOL_DESCRIPTIONS) as ToolName[]).map(toolName => ({ name: toolName, description: TOOL_DESCRIPTIONS[toolName] }))
}

/** What a tool returns when it cannot run or the CLI fails. */
export interface ToolFailure {
  ok: false
  exit: number | null
  error: CliErrorInfo
  output?: Json
}

function failure(code: string, message: string): ToolFailure {
  return { ok: false, exit: null, error: { code, message } }
}

/** Reject an argument object that tries to carry a path or executable. */
export function rejectPathArguments(args: unknown): ToolFailure | undefined {
  if (typeof args !== 'object' || args === null) return undefined
  const found = FORBIDDEN_ARGUMENT_KEYS.filter(key => Object.prototype.hasOwnProperty.call(args, key))
  if (found.length === 0) return undefined
  return failure(
    'path_argument_rejected',
    `Tool arguments may not set ${found.join(', ')}. The ledger directory, registry, policy and binary come from the trusted dsh-tier-drop config only.`,
  )
}

function requireObject(value: unknown, field: string): ToolFailure | undefined {
  if (typeof value === 'object' && value !== null && !Array.isArray(value)) return undefined
  return failure('invalid_argument', `${field} must be a JSON object.`)
}

function toToolValue(result: CliResult): Json {
  if (result.ok) return result.output
  const value: { [key: string]: Json } = { ok: false, exit: result.exit, error: { code: result.error.code, message: result.error.message } }
  if (result.output !== undefined) value['output'] = result.output
  return value
}

/** Write each JSON value to its own file in a fresh private temporary directory, run `body`, then remove the directory. */
async function withTempJson<T>(files: Record<string, unknown>, body: (paths: Record<string, string>) => Promise<T>): Promise<T> {
  const dir = await mkdtemp(path.join(os.tmpdir(), 'tier-drop-'))
  try {
    const paths: Record<string, string> = {}
    for (const [key, value] of Object.entries(files)) {
      const file = path.join(dir, `${key}.json`)
      await writeFile(file, JSON.stringify(value), { mode: 0o600 })
      paths[key] = file
    }
    return await body(paths)
  } finally {
    await rm(dir, { recursive: true, force: true })
  }
}

const canonicalOutput = {
  schema: { type: 'json' as const },
  render: (_args: unknown, value: unknown) => [{ type: 'text' as const, text: JSON.stringify(value) }],
}

const taskParameter = {
  type: 'json' as const,
  required: true as const,
  description: 'A complete tier-drop Task object (task_id, family, shape, input_sha256, rights, floors, qos, review, metadata, created_at). No paths.',
}

/** Build the five tool definitions for one trusted config. Exported for tests. */
export function createTools(config: TrustedConfig): ToolDefinition[] {
  // decision: user-supplied flag values are passed as `--flag=value` so a value that starts with
  // `-` can never be read as another flag.
  const policy = config.policyPath === undefined ? [] : [`--policy=${config.policyPath}`]
  const ledger = `--ledger=${config.ledgerDir}`
  const registry = `--registry=${config.registryPath}`
  const run = (args: string[], stdin: string | null, signal: AbortSignal): Promise<CliResult> =>
    runCli(config.binary, args, stdin, config.timeoutMs, signal)
  // decision: the declared dsh tool deadline is the CLI timeout plus a 5000 ms margin so the
  // subprocess timeout fires first and its error reaches the model.
  const timeoutMs = config.timeoutMs + 5000

  const taskTool = (toolName: 'tier_drop_plan' | 'tier_drop_qos' | 'tier_drop_decide', command: (task: string) => string[], readOnly: boolean): ToolDefinition =>
    defineTool({
      name: toolName,
      description: TOOL_DESCRIPTIONS[toolName],
      parameters: { task: taskParameter },
      output: canonicalOutput,
      timeoutMs,
      isConcurrencySafe: () => readOnly,
      async execute(args, exec) {
        const rejected = rejectPathArguments(args) ?? requireObject(args.task, 'task')
        if (rejected) return rejected as unknown as Json
        return await withTempJson({ task: args.task }, async paths => toToolValue(await run(command(`--task=${paths['task']}`), null, exec.signal)))
      },
    })

  const plan = taskTool('tier_drop_plan', task => ['plan', task, registry, ledger, ...policy], true)
  const qos = taskTool('tier_drop_qos', task => ['qos', task, registry, ...policy], true)
  const decide = taskTool('tier_drop_decide', task => ['decide', task, registry, ledger, ...policy], false)

  const recordAttempt = defineTool({
    name: 'tier_drop_record_attempt',
    description: TOOL_DESCRIPTIONS.tier_drop_record_attempt,
    parameters: {
      task: taskParameter,
      attempt: {
        type: 'json',
        required: true,
        description: 'One tier-drop Attempt object (task_id, family, tier, candidate, outcome, metrics, judge, cost_usd, latency_ms, tokens, input_sha256, output_sha256, rights, eval_item_id). No paths.',
      },
    },
    output: canonicalOutput,
    timeoutMs,
    isConcurrencySafe: () => false,
    async execute(args, exec) {
      const rejected = rejectPathArguments(args) ?? requireObject(args.task, 'task') ?? requireObject(args.attempt, 'attempt')
      if (rejected) return rejected as unknown as Json
      // decision: the attempt goes to the CLI on stdin, as `attempt record` expects; only the task
      // is written to a temporary file.
      return await withTempJson({ task: args.task }, async paths =>
        toToolValue(await run(['attempt', 'record', ledger, `--task=${paths['task']}`, registry, ...policy], JSON.stringify(args.attempt), exec.signal)))
    },
  })

  const ledgerShow = defineTool({
    name: 'tier_drop_ledger_show',
    description: TOOL_DESCRIPTIONS.tier_drop_ledger_show,
    parameters: {
      family: { type: 'string', description: 'Only records of this task family, for example ticket.priority.' },
      kind: { type: 'string', enum: LEDGER_KINDS, description: 'Only this record kind.' },
    },
    output: canonicalOutput,
    timeoutMs,
    isConcurrencySafe: () => true,
    async execute(args, exec) {
      const rejected = rejectPathArguments(args)
      if (rejected) return rejected as unknown as Json
      if (args.family !== undefined && !args.family.trim()) return failure('invalid_argument', 'family must not be empty.') as unknown as Json
      const filters = [
        ...(args.family === undefined ? [] : [`--family=${args.family}`]),
        ...(args.kind === undefined ? [] : [`--kind=${args.kind}`]),
      ]
      return toToolValue(await run(['ledger', 'show', ledger, ...filters], null, exec.signal))
    },
  })

  return [plan, qos, recordAttempt, decide, ledgerShow]
}

/** Register the five tier-drop tools for the lifetime of this plugin instance. */
export function apply(ctx: Context, rawConfig: Config): void {
  const tools = createTools(trustedConfig(rawConfig))
  ctx.effect(function* () {
    for (const tool of tools) yield ctx.tools.register(tool)
  }, 'dsh-tier-drop tools')
}
