import { describe, expect, it } from 'vitest'
import { byteRangeToIndexRange, byteToIndex, utf8Length } from './offsets'

// C a f e-acute space | 日 本 語 | space | emoji | space x
const TEXT = 'Café 日本語 😀 x'
// bytes: C0 a1 f2 é3-4 ' '5 日6-8 本9-11 語12-14 ' '15 😀16-19 ' '20 x21 ; total 22

describe('utf8Length', () => {
  it('counts bytes for 1, 2, 3 and 4 byte characters', () => {
    expect(utf8Length('a')).toBe(1)
    expect(utf8Length('é')).toBe(2)
    expect(utf8Length('日')).toBe(3)
    expect(utf8Length('😀')).toBe(4)
    expect(utf8Length(TEXT)).toBe(22)
    expect(utf8Length('')).toBe(0)
  })
})

describe('byteToIndex', () => {
  it('maps ASCII offsets one to one', () => {
    expect(byteToIndex(TEXT, 0)).toBe(0)
    expect(byteToIndex(TEXT, 2)).toBe(2)
  })
  it('maps the start of 2, 3 and 4 byte characters', () => {
    expect(byteToIndex(TEXT, 3)).toBe(3) // é
    expect(byteToIndex(TEXT, 5)).toBe(4) // space after é
    expect(byteToIndex(TEXT, 6)).toBe(5) // 日
    expect(byteToIndex(TEXT, 9)).toBe(6) // 本
    expect(byteToIndex(TEXT, 15)).toBe(8) // space
    expect(byteToIndex(TEXT, 16)).toBe(9) // 😀 (surrogate pair)
    expect(byteToIndex(TEXT, 20)).toBe(11) // space after the pair
    expect(byteToIndex(TEXT, 21)).toBe(12) // x
  })
  it('snaps offsets inside a multibyte character down to its start', () => {
    expect(byteToIndex(TEXT, 4)).toBe(3)
    expect(byteToIndex(TEXT, 7)).toBe(5)
    expect(byteToIndex(TEXT, 8)).toBe(5)
    expect(byteToIndex(TEXT, 17)).toBe(9)
    expect(byteToIndex(TEXT, 18)).toBe(9)
    expect(byteToIndex(TEXT, 19)).toBe(9)
  })
  it('returns text.length at or past the end', () => {
    expect(byteToIndex(TEXT, 22)).toBe(TEXT.length)
    expect(byteToIndex(TEXT, 999)).toBe(TEXT.length)
    expect(byteToIndex('', 0)).toBe(0)
  })
  it('returns 0 for negative offsets', () => {
    expect(byteToIndex(TEXT, -5)).toBe(0)
  })
  it('handles a 100 KB text quickly', () => {
    const big = 'é日😀a'.repeat(15000)
    const start = Date.now()
    for (let i = 0; i < 50; i++) byteToIndex(big, utf8Length(big) - 1)
    expect(Date.now() - start).toBeLessThan(2000)
    expect(byteToIndex(big, utf8Length(big))).toBe(big.length)
  })
})

describe('byteRangeToIndexRange', () => {
  it('maps a range ending at the end of text', () => {
    const text = 'Café, 日本語 [P1]'
    const bytes = utf8Length(text)
    const [s, e] = byteRangeToIndexRange(text, utf8Length('Café, '), bytes)
    expect(text.slice(s, e)).toBe('日本語 [P1]')
  })
  it('maps a 4 byte character range', () => {
    const [s, e] = byteRangeToIndexRange(TEXT, 16, 20)
    expect(TEXT.slice(s, e)).toBe('😀')
  })
  it('snaps an end inside a character down, like the start', () => {
    const [s, e] = byteRangeToIndexRange(TEXT, 6, 8)
    expect([s, e]).toEqual([5, 5])
  })
})
