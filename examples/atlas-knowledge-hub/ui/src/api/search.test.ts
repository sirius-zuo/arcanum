import { describe, expect, it, vi } from 'vitest'
import { createClient } from './client'
import { searchChunks, stripVectors } from './search'
import type { RawSearchResponse } from './search'

function raw(): RawSearchResponse {
  return {
    chunks: [
      {
        indexed_chunk: {
          chunk: {
            id: 'c1',
            text: 'Remote work policy',
            document_id: 'd1',
            collection_id: 'halcyon',
            position: { start: 0, end: 18, index: 0 },
            metadata: {},
            provenance: {
              document_version: 2,
              source_uri: 'handbook.md',
              snapshot_uri: 's',
              canonical_uri: null,
              page: 3,
              section: 'Remote',
              block_ids: ['b1'],
            },
          },
          vector: new Array(768).fill(0.1),
          token_vectors: [new Array(128).fill(0.2)],
          store_id: 'st',
        },
        score: 0.03,
        strategy: 'Vector',
        kind: 'Source',
      },
    ],
    confidence: 0.5,
    strategy_scores: { Vector: 0.03 },
  }
}

describe('stripVectors', () => {
  it('strips_vectors_and_keeps_provenance', () => {
    const input = raw()
    const before = JSON.stringify(input)
    const out = stripVectors(input)
    const ic = out.chunks[0].indexed_chunk
    expect('vector' in ic).toBe(false)
    expect('token_vectors' in ic).toBe(false)
    expect(ic.chunk.provenance.source_uri).toBe('handbook.md')
    expect(ic.chunk.provenance.document_version).toBe(2)
    expect(ic.chunk.provenance.page).toBe(3)
    expect(ic.store_id).toBe('st')
    expect(out.chunks[0].score).toBe(0.03)
    expect(JSON.stringify(input)).toBe(before)
    expect(input.chunks[0].indexed_chunk.vector).toHaveLength(768)
  })
})

describe('searchChunks', () => {
  it('posts_the_query_and_returns_a_stripped_result', async () => {
    const fetchMock = vi.fn(async () => new Response(JSON.stringify(raw()), { status: 200 }))
    vi.stubGlobal('fetch', fetchMock)
    const res = await searchChunks(createClient(() => 'k'), { query: 'remote', top_k: 5, collection_id: 'halcyon' })
    const [url, init] = fetchMock.mock.calls[0] as unknown as [string, RequestInit]
    expect(url).toBe('/api/v1/search')
    expect(JSON.parse(init.body as string)).toEqual({ query: 'remote', collection_id: 'halcyon', top_k: 5 })
    expect('vector' in res.chunks[0].indexed_chunk).toBe(false)
    vi.unstubAllGlobals()
  })
})
