import { afterEach, describe, expect, it, vi } from 'vitest'
import { ApiError, createClient } from './client'
import { getAudit, getMetrics, getReady, rotateKeys, runGc } from './admin'

afterEach(() => vi.unstubAllGlobals())

const client = () => createClient(() => 'k')
const json = (body: unknown, status = 200) => new Response(JSON.stringify(body), { status })

describe('admin api', () => {
  it('gc_503_maps_to_unavailable_state', async () => {
    vi.stubGlobal('fetch', vi.fn(async (_url: string) => json({ error: 'GC worker not configured' }, 503)))
    const res = await runGc(client())
    expect(res).toEqual({ kind: 'unavailable', message: 'GC worker not configured' })
  })

  it('gc_ok_returns_the_report', async () => {
    const report = { versions_deleted: 1, snapshots_removed: 2, chunks_removed: 3, errors: [] }
    vi.stubGlobal('fetch', vi.fn(async (_url: string) => json(report)))
    expect(await runGc(client())).toEqual({ kind: 'ok', report })
  })

  it('gc_other_errors_still_throw', async () => {
    vi.stubGlobal('fetch', vi.fn(async (_url: string) => json({ error: 'boom' }, 500)))
    await expect(runGc(client())).rejects.toBeInstanceOf(ApiError)
  })

  it('metrics_503_maps_to_empty_state', async () => {
    vi.stubGlobal('fetch', vi.fn(async (_url: string) => json({ error: 'the engine returned no metrics' }, 503)))
    expect(await getMetrics(client())).toEqual({ kind: 'unavailable', message: 'the engine returned no metrics' })
  })

  it('audit_unwraps_logs_and_caps_at_100', async () => {
    const logs = Array.from({ length: 120 }, (_, i) => ({
      entry: { operation: `op${i}`, user_id: 'u', collection_id: 'halcyon', result: 'ok' },
      timestamp: '2026-10-03T10:00:00Z',
    }))
    const fn = vi.fn(async (_url: string) => json({ logs }))
    vi.stubGlobal('fetch', fn)
    const out = await getAudit(client())
    expect(out).toHaveLength(100)
    expect(out[0].entry.operation).toBe('op0')
    expect(fn.mock.calls[0][0]).toBe('/admin/audit')
  })

  it('rotate_posts_and_ready_is_unauthenticated', async () => {
    const fn = vi.fn(async (_url: string) => json({ status: 'rotated' }))
    vi.stubGlobal('fetch', fn)
    expect(await rotateKeys(client())).toEqual({ status: 'rotated' })
    expect(fn.mock.calls[0][0]).toBe('/admin/rotate-keys')
    const ready = vi.fn(async (_url: string) => json({ status: 'ready' }))
    vi.stubGlobal('fetch', ready)
    expect(await getReady()).toEqual({ status: 'ready' })
    expect(ready.mock.calls[0]).toEqual(['/ready'])
  })
})
