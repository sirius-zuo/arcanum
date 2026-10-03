import { describe, expect, it } from 'vitest'
import { idempotencyKey } from './idempotency'

const bytes = (...n: number[]) => new Uint8Array(n).buffer

describe('idempotencyKey', () => {
  it('same_content_same_key', async () => {
    const a = await idempotencyKey('halcyon', 'a.md', bytes(1, 2, 3))
    const b = await idempotencyKey('halcyon', 'a.md', bytes(1, 2, 3))
    expect(a).toBe(b)
  })

  it('one_byte_change_changes_key', async () => {
    const a = await idempotencyKey('halcyon', 'a.md', bytes(1, 2, 3))
    const b = await idempotencyKey('halcyon', 'a.md', bytes(1, 2, 4))
    expect(a).not.toBe(b)
  })

  it('starts_with_collection_prefix_and_is_hex', async () => {
    const k = await idempotencyKey('halcyon', 'a.md', bytes(9))
    expect(k).toMatch(/^halcyon:[0-9a-f]{64}$/)
  })

  it('source_uri_is_part_of_the_key', async () => {
    const a = await idempotencyKey('halcyon', 'a.md', bytes(1))
    const b = await idempotencyKey('halcyon', 'b.md', bytes(1))
    expect(a).not.toBe(b)
  })
})
