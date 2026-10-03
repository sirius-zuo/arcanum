import { afterEach, describe, expect, it, vi } from 'vitest'
import { safeGet, safeRemove, safeSet } from './storage'

describe('storage', () => {
  afterEach(() => {
    vi.unstubAllGlobals()
  })

  it('safe_storage_never_throws', () => {
    const boom = () => {
      throw new Error('blocked')
    }
    vi.stubGlobal('localStorage', { getItem: boom, setItem: boom, removeItem: boom })
    expect(safeGet('k')).toBeNull()
    expect(() => safeSet('k', 'v')).not.toThrow()
    expect(() => safeRemove('k')).not.toThrow()
  })

  it('round_trips_when_available', () => {
    safeSet('atlas.test', 'x')
    expect(safeGet('atlas.test')).toBe('x')
    safeRemove('atlas.test')
    expect(safeGet('atlas.test')).toBeNull()
  })
})
