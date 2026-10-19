import assert from 'node:assert/strict'
import { describe, test } from 'node:test'

import { runCli } from '../src/cli.ts'

// These use ordinary POSIX tools (`sleep`, `cat`, `sh`) as stand-ins for the tier-drop binary.
const POSIX = process.platform !== 'win32' ? false : 'POSIX tools only'

describe('runCli process handling', { skip: POSIX }, () => {
  test('a process that outlives timeoutMs returns a timeout error', async () => {
    const started = Date.now()
    const result = await runCli('sleep', ['5'], null, 200)
    assert.ok(Date.now() - started < 4000, 'returned well before the child would have exited')
    assert.equal(result.ok, false)
    if (!result.ok) assert.equal(result.error.code, 'timeout')
  })

  test('an aborted signal returns an aborted error', async () => {
    const controller = new AbortController()
    const pending = runCli('sleep', ['5'], null, 10000, controller.signal)
    setTimeout(() => controller.abort(), 100)
    const result = await pending
    assert.equal(result.ok, false)
    if (!result.ok) assert.equal(result.error.code, 'aborted')
  })

  test('stdin reaches the child and JSON stdout is parsed', async () => {
    const result = await runCli('cat', [], '{"a":[1,2]}', 5000)
    assert.deepEqual(result, { ok: true, exit: 0, output: { a: [1, 2] } })
  })

  test('exit 0 with non-JSON stdout is invalid_output', async () => {
    const result = await runCli('cat', [], 'not json', 5000)
    assert.equal(result.ok, false)
    if (!result.ok) assert.equal(result.error.code, 'invalid_output')
  })

  test('a non-zero exit keeps the parsed stdout and the stderr message', async () => {
    // sh -c here runs a fixed literal script, never tool input.
    const result = await runCli('sh', ['-c', 'echo \'{"ok":false,"error":{"code":"bad_task","message":"m"}}\'; echo boom >&2; exit 1'], null, 5000)
    assert.equal(result.ok, false)
    if (!result.ok) {
      assert.equal(result.exit, 1)
      assert.equal(result.error.code, 'bad_task')
      assert.equal(result.error.message, 'boom')
      assert.deepEqual(result.output, { ok: false, error: { code: 'bad_task', message: 'm' } })
    }
  })
})
