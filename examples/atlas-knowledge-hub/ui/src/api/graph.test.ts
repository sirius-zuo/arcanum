import { afterEach, describe, expect, it, vi } from 'vitest'
import { createClient } from './client'
import { getGraph } from './graph'

afterEach(() => vi.unstubAllGlobals())

describe('graph api', () => {
  it('requires_collection_param_in_url', async () => {
    const fn = vi.fn(async (_url: string) => new Response('{"nodes":[],"edges":[]}', { status: 200 }))
    vi.stubGlobal('fetch', fn)
    const g = await getGraph(createClient(() => 'k'), 'hal cyon')
    expect(fn.mock.calls[0][0]).toBe('/api/v1/graph?collection_id=hal%20cyon')
    expect(g).toEqual({ nodes: [], edges: [] })
  })
})
