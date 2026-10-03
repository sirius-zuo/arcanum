import { afterEach, describe, expect, it, vi } from 'vitest'
import { createClient } from './client'
import { buildContext } from './context'

afterEach(() => vi.unstubAllGlobals())

describe('buildContext', () => {
  it('posts_the_request_body_to_the_context_route', async () => {
    const fetchMock = vi.fn(async () => new Response(JSON.stringify({ passages: [] }), { status: 200 }))
    vi.stubGlobal('fetch', fetchMock)
    const req = { collection_id: 'halcyon', query: 'q', token_budget: 1000, background_share: 0.1, candidate_k: 20, render: 'xml' as const }
    const res = await buildContext(createClient(() => 'k'), req)
    const [url, init] = fetchMock.mock.calls[0] as unknown as [string, RequestInit]
    expect(url).toBe('/api/v1/context')
    expect(init.method).toBe('POST')
    expect(JSON.parse(init.body as string)).toEqual(req)
    expect(res.passages).toEqual([])
  })
})
