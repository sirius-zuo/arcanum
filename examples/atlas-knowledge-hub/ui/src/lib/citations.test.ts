import { describe, expect, it } from 'vitest'
import { splitAnswer } from './citations'

describe('splitAnswer', () => {
  it('splits text and cite parts in order', () => {
    expect(splitAnswer('A [P1]. B [P1, P2] [S3].')).toEqual([
      { kind: 'text', text: 'A ' },
      { kind: 'cite', ids: ['P1'], raw: '[P1]' },
      { kind: 'text', text: '. B ' },
      { kind: 'cite', ids: ['P1', 'P2'], raw: '[P1, P2]' },
      { kind: 'text', text: ' ' },
      { kind: 'cite', ids: ['S3'], raw: '[S3]' },
      { kind: 'text', text: '.' },
    ])
  })

  it('leaves non-markers as text', () => {
    for (const s of ['x [P1234] y', 'x [p1] y', 'x [P1-P3] y', 'x [P] y', 'plain']) {
      expect(splitAnswer(s)).toEqual([{ kind: 'text', text: s }])
    }
  })

  it('treats adjacent groups as separate cites', () => {
    expect(splitAnswer('[P1][P2]')).toEqual([
      { kind: 'cite', ids: ['P1'], raw: '[P1]' },
      { kind: 'cite', ids: ['P2'], raw: '[P2]' },
    ])
  })

  it('allows whitespace inside the brackets and around commas', () => {
    expect(splitAnswer('[ P1 ,P2 ,  S10 ]')).toEqual([{ kind: 'cite', ids: ['P1', 'P2', 'S10'], raw: '[ P1 ,P2 ,  S10 ]' }])
  })

  it('returns nothing for the empty string', () => {
    expect(splitAnswer('')).toEqual([])
  })

  it('keeps a half-streamed marker as text', () => {
    expect(splitAnswer('Notice is 60 days [P')).toEqual([{ kind: 'text', text: 'Notice is 60 days [P' }])
  })
})
