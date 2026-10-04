import { afterEach, describe, expect, it, vi } from 'vitest'
import { createClient } from './client'
import { passagesForVerify, verifyAnswer } from './verify'
import type { ContextPassage, ContextResponse } from './types'

afterEach(() => vi.unstubAllGlobals())

const passage = (ref_id: string, chunk_ids: string[]) =>
  ({ ref_id, chunk_ids, text: 'ignored', source_uri: 'x.md', score: 1 }) as unknown as ContextPassage

describe('verify api', () => {
  it('passages_for_verify_maps_ref_and_chunk_ids', () => {
    const ctx = { passages: [passage('P1', ['c1']), passage('P2', ['c2', 'c3'])] } as unknown as ContextResponse
    expect(passagesForVerify(ctx)).toEqual([
      { ref_id: 'P1', chunk_ids: ['c1'] },
      { ref_id: 'P2', chunk_ids: ['c2', 'c3'] },
    ])
  })

  it('verify_answer_posts_the_request_without_unset_options', async () => {
    const fn = vi.fn(async () => new Response('{"verdict":"pass"}', { status: 200 }))
    vi.stubGlobal('fetch', fn)
    const client = createClient(() => 'k')
    await verifyAnswer(client, { collection_id: 'halcyon', answer: 'A [P1].', passages: [{ ref_id: 'P1', chunk_ids: ['c1'] }] })
    const [url, init] = fn.mock.calls[0] as unknown as [string, RequestInit]
    expect(url).toBe('/api/v1/verify')
    expect(init.method).toBe('POST')
    expect(JSON.parse(init.body as string)).toEqual({ collection_id: 'halcyon', answer: 'A [P1].', passages: [{ ref_id: 'P1', chunk_ids: ['c1'] }] })
  })
})
