import { afterEach, describe, expect, it, vi } from 'vitest'
import { createClient } from './client'
import { getChunkProof, getEntityProof, getRelationProof, getTreeNodeProof, isUuid } from './evidence'

afterEach(() => vi.unstubAllGlobals())

const A = '11111111-1111-4111-8111-111111111111'
const B = '22222222-2222-4222-8222-222222222222'

function stub() {
  const fn = vi.fn(async (_url: string) => new Response('{"root":{},"raw_sources":[]}', { status: 200 }))
  vi.stubGlobal('fetch', fn)
  return fn
}

describe('evidence api', () => {
  it('relation_proof_encodes_relation_type', async () => {
    const fn = stub()
    await getRelationProof(createClient(() => 'k'), A, 'works on/with', B)
    expect(fn.mock.calls[0][0]).toBe(`/evidence/relation/${A}/works%20on%2Fwith/${B}`)
  })

  it('reads_each_proof_route', async () => {
    const fn = stub()
    const client = createClient(() => 'k')
    await getChunkProof(client, A)
    await getTreeNodeProof(client, A)
    await getEntityProof(client, A)
    expect(fn.mock.calls.map((c) => c[0])).toEqual([`/evidence/chunk/${A}`, `/evidence/tree-node/${A}`, `/evidence/entity/${A}`])
  })

  it('is_uuid_accepts_only_uuids', () => {
    expect(isUuid(A)).toBe(true)
    expect(isUuid(` ${A.toUpperCase()} `)).toBe(true)
    expect(isUuid('not-a-uuid')).toBe(false)
    expect(isUuid('')).toBe(false)
    expect(isUuid(A + 'x')).toBe(false)
  })
})
