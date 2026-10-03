import { describe, expect, it } from 'vitest'
import type { Verdict } from '../api/types'
import { summarize, verdictMeta } from './verdicts'

const ALL: Verdict[] = ['supported', 'miscited', 'uncited_supported', 'partial', 'unsupported', 'no_claim']

describe('verdictMeta', () => {
  it('gives each verdict a distinct label and icon, never color alone', () => {
    const metas = ALL.map(verdictMeta)
    expect(new Set(metas.map((m) => m.label)).size).toBe(6)
    expect(new Set(metas.map((m) => m.icon)).size).toBe(6)
    for (const m of metas) expect(m.tone).toBeTruthy()
  })
  it('maps tones to the --v-* variable names', () => {
    expect(verdictMeta('supported').tone).toBe('supported')
    expect(verdictMeta('uncited_supported').tone).toBe('uncited')
    expect(verdictMeta('no_claim').tone).toBe('noclaim')
  })
})

describe('summarize', () => {
  it('counts total and passing (supported plus uncited_supported)', () => {
    const s = summarize({
      supported: 2,
      unsupported: 1,
      miscited: 0,
      uncited_supported: 1,
      partial: 0,
      no_claim: 0,
    })
    expect(s.total).toBe(4)
    expect(s.passing).toBe(3)
    expect(s.label).toBe('3 of 4 claims supported')
  })
  it('handles zero sentences', () => {
    const s = summarize({
      supported: 0,
      unsupported: 0,
      miscited: 0,
      uncited_supported: 0,
      partial: 0,
      no_claim: 0,
    })
    expect(s).toEqual({ total: 0, passing: 0, label: 'Nothing to verify' })
  })
  it('excludes no_claim sentences from the total', () => {
    const s = summarize({
      supported: 1,
      unsupported: 0,
      miscited: 0,
      uncited_supported: 0,
      partial: 0,
      no_claim: 3,
    })
    expect(s.total).toBe(1)
    expect(s.label).toBe('1 of 1 claims supported')
  })
})
