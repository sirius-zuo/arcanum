import { describe, expect, it } from 'vitest'
import { render, screen } from '@testing-library/react'
import { buildSpans, locateChunks } from './ChunkSpans'
import { ChunkSpans } from './ChunkSpans'
import { utf8Length } from '../lib/offsets'
import type { AnnotatedChunk } from '../api/types'

const chunk = (text: string, overlap = 0): AnnotatedChunk => ({
  text,
  char_count: [...text].length,
  token_estimate: Math.floor([...text].length / 4),
  overlap_chars: overlap,
})

describe('ChunkSpans', () => {
  it('spans_cover_text_without_gaps_and_mark_overlap', () => {
    const text = 'abcdefghij klmnop'
    // second chunk overlaps the first by 3 bytes; the tail "klmnop" (after a gap char) is uncovered.
    const chunks = [chunk('abcdef'), chunk('defghij', 3)]
    const spans = buildSpans(text, locateChunks(text, chunks))
    expect(spans.map((s) => s.text).join('')).toBe(text)
    const overlap = spans.find((s) => s.chunks.length > 1)
    expect(overlap?.text).toBe('def')
    expect(overlap?.chunks).toEqual([0, 1])
    expect(spans[spans.length - 1].chunks).toEqual([])
    for (let i = 1; i < spans.length; i++) expect(spans[i].start).toBe(spans[i - 1].end)

    render(<ChunkSpans text={text} chunks={chunks} />)
    expect(screen.getByTestId('chunk-text').textContent).toBe(text)
    expect(screen.getByText(/overlap 3 B/)).toBeInTheDocument()
  })

  it('locates_chunks_in_multibyte_text_using_byte_overlap', () => {
    const text = 'Café 日本語 日本語 policy ok'
    const first = 'Café 日本語 日本'
    const second2 = '本語 policy ok'
    expect(utf8Length('本')).toBe(3)
    const located = locateChunks(text, [chunk(first), chunk(second2, 3)])
    expect(located[0]).toEqual({ start: 0, end: first.length })
    // overlap of 3 bytes is the single char 本, so the second chunk starts one char before the first ends
    expect(located[1]?.start).toBe(first.length - 1)
    expect(text.slice(located[1]!.start, located[1]!.end)).toBe(second2)
    const spans = buildSpans(text, located)
    expect(spans.map((s) => s.text).join('')).toBe(text)
  })

  it('leaves_unlocatable_chunks_out_but_still_covers_the_text', () => {
    const text = 'alpha beta gamma'
    const located = locateChunks(text, [chunk('alpha beta'), chunk('generated summary not in source')])
    expect(located[1]).toBeNull()
    expect(buildSpans(text, located).map((s) => s.text).join('')).toBe(text)
  })
})
