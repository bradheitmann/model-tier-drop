/**
 * Subprocess runner for the `tier-drop` CLI. Pure with respect to dsh: no Cordis or dsh imports,
 * so it is testable on its own. The CLI prints JSON on stdout and exits 0 (ok), 1 (validation or
 * floor failure, JSON still describes it) or 2 (usage error).
 */
import { execFile, type ExecFileException } from 'node:child_process'

/** Largest stdout or stderr accepted from one CLI call. */
export const MAX_BUFFER = 8 * 1024 * 1024

/** Longest stderr excerpt carried into an error message. */
const MAX_MESSAGE = 2000

/** A lossless JSON value as parsed from CLI stdout. */
export type Json = null | boolean | number | string | Json[] | { [key: string]: Json }

/** A coded error. */
export interface CliErrorInfo {
  code: string
  message: string
}

/** Exit 0 with JSON on stdout. */
export interface CliSuccess {
  ok: true
  exit: 0
  output: Json
}

/**
 * Anything else: a non-zero exit, a spawn failure, a timeout, a cancellation, or stdout that is
 * not JSON. `exit` is the process exit code, or null when the process did not exit normally.
 * `output` is the parsed stdout JSON when there was any (a floor failure exits 1 and still
 * describes a valid result there).
 */
export interface CliFailure {
  ok: false
  exit: number | null
  error: CliErrorInfo
  output?: Json
}

export type CliResult = CliSuccess | CliFailure

function trimMessage(text: string): string {
  const t = text.trim()
  return t.length > MAX_MESSAGE ? `${t.slice(0, MAX_MESSAGE)}…` : t
}

function parseJson(stdout: string): { ok: true; value: Json } | { ok: false } {
  if (!stdout.trim()) return { ok: false }
  try {
    return { ok: true, value: JSON.parse(stdout) as Json }
  } catch {
    return { ok: false }
  }
}

function isRecord(value: Json | undefined): value is { [key: string]: Json } {
  return typeof value === 'object' && value !== null && !Array.isArray(value)
}

/** Pick an error code for a non-zero exit from the CLI's own JSON when it carries one. */
function exitErrorCode(exit: number, output: Json | undefined): string {
  if (isRecord(output)) {
    const err = output['error']
    if (isRecord(err) && typeof err['code'] === 'string') return err['code']
    if (Array.isArray(output['issues'])) return 'validation_failed'
    if (isRecord(output['floor_result'])) return 'floors_not_met'
  }
  return exit === 2 ? 'usage_error' : `exit_${exit}`
}

/**
 * Run `binary args…` without a shell, write `stdin` (if any) to its standard input, and parse its
 * stdout as JSON. Never throws: every failure becomes a {@link CliFailure}.
 */
export function runCli(
  binary: string,
  args: readonly string[],
  stdin: string | null,
  timeoutMs: number,
  signal?: AbortSignal,
): Promise<CliResult> {
  return new Promise<CliResult>(resolve => {
    if (signal?.aborted) {
      resolve({ ok: false, exit: null, error: { code: 'aborted', message: 'The call was cancelled before it started.' } })
      return
    }
    let child: ReturnType<typeof execFile>
    try {
      child = execFile(
        binary,
        [...args],
        {
          encoding: 'utf8',
          timeout: timeoutMs,
          maxBuffer: MAX_BUFFER,
          windowsHide: true,
          ...(signal ? { signal } : {}),
        },
        (error: ExecFileException | null, stdout: string, stderr: string) => {
          const parsed = parseJson(stdout)
          const output = parsed.ok ? parsed.value : undefined
          const withOutput = output === undefined ? {} : { output }
          if (!error) {
            if (parsed.ok) resolve({ ok: true, exit: 0, output: parsed.value })
            else resolve({ ok: false, exit: 0, error: { code: 'invalid_output', message: 'tier-drop exited 0 but stdout was not JSON.' } })
            return
          }
          if (signal?.aborted || error.name === 'AbortError') {
            resolve({ ok: false, exit: null, error: { code: 'aborted', message: 'The call was cancelled.' }, ...withOutput })
            return
          }
          if (error.code === 'ERR_CHILD_PROCESS_STDIO_MAXBUFFER') {
            resolve({ ok: false, exit: null, error: { code: 'output_too_large', message: `tier-drop output exceeded ${MAX_BUFFER} bytes.` } })
            return
          }
          if (error.killed && typeof error.code !== 'number') {
            resolve({ ok: false, exit: null, error: { code: 'timeout', message: `tier-drop did not finish within ${timeoutMs} ms.` }, ...withOutput })
            return
          }
          if (typeof error.code === 'number') {
            const exit = error.code
            const message = trimMessage(stderr) || `tier-drop exited ${exit}.`
            resolve({ ok: false, exit, error: { code: exitErrorCode(exit, output), message }, ...withOutput })
            return
          }
          // Spawn failures (ENOENT, EACCES) carry a string code and no exit.
          const code = typeof error.code === 'string' ? error.code : 'spawn_failed'
          resolve({ ok: false, exit: null, error: { code: code === 'ENOENT' ? 'binary_not_found' : 'spawn_failed', message: `Could not run ${binary}: ${error.message}` } })
        },
      )
    } catch (error) {
      resolve({ ok: false, exit: null, error: { code: 'spawn_failed', message: `Could not run ${binary}: ${error instanceof Error ? error.message : String(error)}` } })
      return
    }
    // decision: a failed spawn closes stdin with EPIPE; swallow it here because the exec callback
    // above already reports the real failure.
    child.stdin?.on('error', () => {})
    if (stdin === null) child.stdin?.end()
    else child.stdin?.end(stdin)
  })
}
