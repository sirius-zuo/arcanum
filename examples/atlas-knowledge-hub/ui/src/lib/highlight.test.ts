import { describe, expect, it } from 'vitest'
import { segmentText, termRanges, type Range } from './highlight'

const join = (segs: { text: string }[]) => segs.map((s) => s.text).join('')

describe('segmentText', () => {
  it('returns one plain segment with no ranges', () => {
    expect(segmentText('hello', [])).toEqual([{ text: 'hello', kinds: [], ids: [] }])
  })
  it('returns no segments for empty text', () => {
    expect(segmentText('', [{ start: 0, end: 3, kind: 'term' }])).toEqual([])
  })
  it('splits into plain, marked and plain parts', () => {
    const segs = segmentText('hello world', [{ start: 6, end: 11, kind: 'term' }])
    expect(segs).toEqual([
      { text: 'hello ', kinds: [], ids: [] },
      { text: 'world', kinds: ['term'], ids: [] },
    ])
  })
  it('merges overlapping term and evidence ranges into one segment with both kinds', () => {
    const text = 'abcdefghij'
    const segs = segmentText(text, [
      { start: 2, end: 6, kind: 'term' },
      { start: 4, end: 8, kind: 'evidence', id: 'e1' },
    ])
    expect(segs).toEqual([
      { text: 'ab', kinds: [], ids: [] },
      { text: 'cdefgh', kinds: ['term', 'evidence'], ids: ['e1'] },
      { text: 'ij', kinds: [], ids: [] },
    ])
  })
  it('merges transitively overlapping ranges and dedupes kinds and ids', () => {
    const segs = segmentText('abcdefghij', [
      { start: 0, end: 3, kind: 'term', id: 'a' },
      { start: 2, end: 5, kind: 'term', id: 'a' },
      { start: 4, end: 6, kind: 'evidence', id: 'b' },
    ])
    expect(segs).toHaveLength(2)
    expect(segs[0]).toEqual({ text: 'abcdef', kinds: ['term', 'evidence'], ids: ['a', 'b'] })
  })
  it('keeps merely touching ranges separate', () => {
    const segs = segmentText('abcd', [
      { start: 0, end: 2, kind: 'term' },
      { start: 2, end: 4, kind: 'evidence' },
    ])
    expect(segs.map((s) => s.kinds)).toEqual([['term'], ['evidence']])
  })
  it('clamps ranges to the text', () => {
    const segs = segmentText('abcd', [{ start: -5, end: 99, kind: 'term' }])
    expect(segs).toEqual([{ text: 'abcd', kinds: ['term'], ids: [] }])
  })
  it('drops empty and inverted ranges', () => {
    const ranges: Range[] = [
      { start: 2, end: 2, kind: 'term' },
      { start: 3, end: 1, kind: 'term' },
      { start: 10, end: 20, kind: 'term' },
    ]
    expect(segmentText('abcd', ranges)).toEqual([{ text: 'abcd', kinds: [], ids: [] }])
  })
  it('concatenates back to the original text and is ordered', () => {
    const text = 'The quick brown fox jumps over the lazy dog'
    const segs = segmentText(text, [
      { start: 30, end: 40, kind: 'evidence' },
      { start: 4, end: 9, kind: 'term' },
      { start: 8, end: 12, kind: 'term' },
    ])
    expect(join(segs)).toBe(text)
  })
  it('never splits a surrogate pair', () => {
    const text = 'a😀b'
    // boundary at 2 is between the halves of the pair
    const segs = segmentText(text, [{ start: 0, end: 2, kind: 'term' }])
    expect(join(segs)).toBe(text)
    for (const s of segs) {
      expect(s.text).not.toMatch(/^[\uDC00-\uDFFF]/)
      expect(s.text).not.toMatch(/[\uD800-\uDBFF]$/)
    }
    const segs2 = segmentText(text, [{ start: 2, end: 3, kind: 'term' }])
    expect(join(segs2)).toBe(text)
    for (const s of segs2) {
      expect(s.text).not.toMatch(/^[\uDC00-\uDFFF]/)
      expect(s.text).not.toMatch(/[\uD800-\uDBFF]$/)
    }
    expect(segs2.find((s) => s.kinds.includes('term'))?.text).toBe('😀')
  })
})

describe('termRanges', () => {
  it('finds case-insensitive matches', () => {
    const r = termRanges('Refund policy and REFUND window', 'refund')
    expect(r).toEqual([
      { start: 0, end: 6, kind: 'term' },
      { start: 18, end: 24, kind: 'term' },
    ])
  })
  it('ignores one-character terms', () => {
    expect(termRanges('a b c', 'a b')).toEqual([])
    expect(termRanges('aa b', 'aa b')).toEqual([{ start: 0, end: 2, kind: 'term' }])
  })
  it('escapes regex metacharacters', () => {
    const r = termRanges('cost (usd) and a.b', '(usd) a.b')
    expect(r.map((x) => [x.start, x.end])).toEqual([
      [5, 10],
      [15, 18],
    ])
  })
  it('does not produce overlapping duplicates', () => {
    const r = termRanges('refunds', 'refund refunds refund')
    expect(r).toEqual([{ start: 0, end: 7, kind: 'term' }])
  })
  it('returns nothing for an empty query', () => {
    expect(termRanges('abc', '   ')).toEqual([])
  })
})
