import { describe, expect, it } from 'vitest'
import { render, screen } from '@testing-library/react'
import { MemoryRouter } from 'react-router-dom'
import { ResultCard } from './ResultCard'
import type { RetrievedChunk } from '../api/types'

const chunk: RetrievedChunk = {
  indexed_chunk: {
    chunk: {
      id: 'chunk-9',
      text: 'Employees may work remotely three days a week.',
      document_id: 'd1',
      collection_id: 'halcyon',
      position: { start: 0, end: 46, index: 0 },
      metadata: {},
      provenance: { document_version: 2, source_uri: 'handbook.md', snapshot_uri: 's', canonical_uri: null, page: 4, section: 'Remote work', block_ids: [] },
    },
    store_id: 's',
  },
  score: 0.0328,
  strategy: 'Bm25',
  kind: { Summary: { level: 2, covers: ['a', 'b', 'c'] } },
}

describe('ResultCard', () => {
  it('labels_score_as_fused_and_highlights_terms', () => {
    const { container } = render(
      <MemoryRouter>
        <ResultCard chunk={chunk} query="remotely work" />
      </MemoryRouter>,
    )
    expect(screen.getByText(/fused score/i)).toBeInTheDocument()
    expect(screen.queryByText(/confidence/i)).not.toBeInTheDocument()
    const marks = Array.from(container.querySelectorAll('mark')).map((m) => m.textContent?.toLowerCase())
    expect(marks).toEqual(expect.arrayContaining(['remotely', 'work']))
    expect(screen.getByText('0.0328')).toBeInTheDocument()
    expect(screen.getByText(/level 2/i)).toBeInTheDocument()
    expect(screen.getByText(/3 chunks/i)).toBeInTheDocument()
    expect(screen.getByText(/handbook\.md/)).toBeInTheDocument()
    expect(screen.getByText(/v2/)).toBeInTheDocument()
    expect(screen.getByText(/page 4/i)).toBeInTheDocument()
    expect(screen.getByText(/Remote work/)).toBeInTheDocument()
    expect(screen.getByRole('link', { name: /open evidence/i })).toHaveAttribute('href', '/evidence?chunk=chunk-9')
  })
})

describe('ResultCard highlighting', () => {
  const withText = (text: string): RetrievedChunk => ({
    ...chunk,
    kind: 'Source',
    indexed_chunk: { ...chunk.indexed_chunk, chunk: { ...chunk.indexed_chunk.chunk, text } },
  })
  const marks = (text: string, query: string) => {
    const { container } = render(
      <MemoryRouter>
        <ResultCard chunk={withText(text)} query={query} />
      </MemoryRouter>,
    )
    return Array.from(container.querySelectorAll('mark')).map((m) => m.textContent)
  }

  it('strips_punctuation_from_query_terms', () => {
    expect(marks('The SLA is 99.9%.', 'SLA?')).toEqual(['SLA'])
  })

  it('word_boundary_check_handles_astral_letters', () => {
    expect(marks('\u{1D49C}dmin and dmin', 'dmin')).toEqual(['dmin'])
  })
})
