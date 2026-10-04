import { afterEach, describe, expect, it, vi } from 'vitest'
import { ApiError, createClient } from './client'
import { buildExperimentSamples, runBenchmark, startExperiment } from './lab'
import type { PerBackendChunkConfig } from './types'

afterEach(() => vi.unstubAllGlobals())

const config: PerBackendChunkConfig = {
  vector: { strategy: 'semantic', params: { max_chars: 800 } },
  lexical: null,
  graph: null,
  tree: null,
}

describe('lab api', () => {
  it('benchmark_encodes_content_as_byte_array', async () => {
    const fn = vi.fn(async () => new Response('{"metrics":[]}', { status: 200 }))
    vi.stubGlobal('fetch', fn)
    const text = 'Café, 日本語 😀'
    await runBenchmark(
      createClient(() => 'k'),
      [{ source_uri: 'a.md', text }],
      [{ query: 'q', relevant_source_uri: 'a.md' }],
      [{ strategy: 'fixed', params: { chunk_size: 512, overlap: 64 } }],
    )
    const [url, init] = fn.mock.calls[0] as unknown as [string, RequestInit]
    expect(url).toBe('/api/v1/chunk/benchmark')
    const body = JSON.parse(init.body as string)
    const doc = body.corpus[0]
    expect(Array.isArray(doc.content)).toBe(true)
    expect(new TextDecoder().decode(new Uint8Array(doc.content))).toBe(text)
    expect(doc.source_uri).toBe('a.md')
    expect(doc.mime_type).toBe('text/plain')
    expect(doc.id).toMatch(/^[0-9a-f-]{36}$/)
    expect(body.queries[0]).toEqual({ text: 'q', expected_doc_ids: [doc.id] })
  })

  it('start_experiment_409_maps_to_friendly_error', async () => {
    vi.stubGlobal('fetch', vi.fn(async () => new Response('{"error":"collection has an active experiment"}', { status: 409 })))
    const err = await startExperiment(createClient(() => 'k'), 'halcyon', config).catch((e: unknown) => e)
    expect(err).toBeInstanceOf(ApiError)
    expect((err as ApiError).status).toBe(409)
    expect((err as ApiError).message).toMatch(/an experiment is already active/i)
  })

  it('start_experiment_parses_the_201_json_body', async () => {
    vi.stubGlobal('fetch', vi.fn(async () => new Response('{"experiment_id":"e1","status":"active"}', { status: 201 })))
    const exp = await startExperiment(createClient(() => 'k'), 'halcyon', config)
    expect(exp.experiment_id).toBe('e1')
  })

  it('experiment_samples_keep_only_chunks_of_the_relevant_source', async () => {
    const hit = (id: string, src: string) => ({ indexed_chunk: { chunk: { id, provenance: { source_uri: src } }, store_id: 's' } })
    const fn = vi.fn(async (_u: string, init: RequestInit) => {
      const q = JSON.parse(init.body as string).query as string
      const chunks = q === 'one' ? [hit('c1', 'a.md'), hit('c2', 'b.md')] : [hit('c3', 'b.md')]
      return new Response(JSON.stringify({ chunks }), { status: 200 })
    })
    vi.stubGlobal('fetch', fn)
    const out = await buildExperimentSamples(
      createClient(() => 'k'),
      'halcyon',
      [
        { query: 'one', relevant_source_uri: 'a.md' },
        { query: 'two', relevant_source_uri: 'a.md' },
      ],
    )
    expect(out.samples).toEqual([{ query: 'one', relevant_chunk_ids: ['c1'] }])
    expect(out.skipped).toBe(1)
  })
})
