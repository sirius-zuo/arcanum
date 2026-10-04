import { beforeEach, describe, expect, it, vi } from 'vitest'
import { fireEvent, render, screen } from '@testing-library/react'
import { MemoryRouter, Route, Routes } from 'react-router-dom'
import { PassageList } from './PassageList'
import type { ContextPassage } from '../api/types'

const passage: ContextPassage = {
  ref_id: 'P1',
  document_id: 'd1',
  version_num: 2,
  source_uri: 'handbook.md',
  snapshot_uri: 's',
  canonical_uri: null,
  section: 'Remote',
  page: 4,
  offset_start: 100,
  offset_end: 240,
  text: 'Employees may work remotely.',
  chunk_ids: ['c1', 'c2'],
  strategies: ['vector', 'bm25'],
  score: 0.031,
}

describe('PassageList', () => {
  beforeEach(() => sessionStorage.clear())

  it('send_to_ask_stores_prefill', () => {
    render(
      <MemoryRouter initialEntries={['/context']}>
        <Routes>
          <Route path="/context" element={<PassageList passages={[passage]} question="What is the remote policy?" />} />
          <Route path="/ask" element={<p>ask page</p>} />
        </Routes>
      </MemoryRouter>,
    )
    expect(screen.getByText('P1')).toBeInTheDocument()
    expect(screen.getByText('100-240')).toBeInTheDocument()
    expect(screen.getByText(/merged/i)).toBeInTheDocument()
    expect(screen.getByText('Vector')).toBeInTheDocument()
    fireEvent.click(screen.getByRole('button', { name: /send to ask/i }))
    expect(sessionStorage.getItem('atlas.ask.prefill')).toBe('What is the remote policy?')
    expect(screen.getByText('ask page')).toBeInTheDocument()
  })

  it('still_navigates_when_session_storage_throws', () => {
    vi.spyOn(Storage.prototype, 'setItem').mockImplementation(() => {
      throw new Error('blocked')
    })
    render(
      <MemoryRouter initialEntries={['/context']}>
        <Routes>
          <Route path="/context" element={<PassageList passages={[passage]} question="q" />} />
          <Route path="/ask" element={<p>ask page</p>} />
        </Routes>
      </MemoryRouter>,
    )
    fireEvent.click(screen.getByRole('button', { name: /send to ask/i }))
    expect(screen.getByText('ask page')).toBeInTheDocument()
    vi.restoreAllMocks()
  })
})
