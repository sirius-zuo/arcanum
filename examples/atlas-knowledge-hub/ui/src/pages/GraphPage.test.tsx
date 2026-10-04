import { afterEach, describe, expect, it, vi } from 'vitest'
import { fireEvent, render, screen, within } from '@testing-library/react'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { MemoryRouter, Route, Routes } from 'react-router-dom'
import { createClient } from '../api/client'
import { ASK_PREFILL_KEY } from '../components/PassageList'
import GraphPage, { MULTI_HOP_QUESTION } from './GraphPage'

vi.mock('../state/bootstrap', () => ({
  useBootstrap: () => ({ data: { collection: 'halcyon' }, client: createClient(() => 'k') }),
}))

afterEach(() => {
  vi.unstubAllGlobals()
  sessionStorage.clear()
})

const A = '11111111-1111-4111-8111-111111111111'
const B = '22222222-2222-4222-8222-222222222222'
const graph = {
  nodes: [
    { id: A, name: 'Maya Chen', entity_type: 'Person' },
    { id: B, name: 'Navigation Team', entity_type: 'Team' },
  ],
  edges: [
    { source: A, target: B, label: 'leads' },
    { source: A, target: B, label: 'leads' },
  ],
}

function renderPage(body: unknown) {
  vi.stubGlobal('fetch', vi.fn(async () => new Response(JSON.stringify(body), { status: 200 })))
  return render(
    <QueryClientProvider client={new QueryClient()}>
      <MemoryRouter initialEntries={['/graph']}>
        <Routes>
          <Route path="/graph" element={<GraphPage />} />
          <Route path="/ask" element={<p>ask page</p>} />
        </Routes>
      </MemoryRouter>
    </QueryClientProvider>,
  )
}

describe('GraphPage', () => {
  it('explains_an_empty_graph', async () => {
    renderPage({ nodes: [], edges: [] })
    expect(await screen.findByText(/full pipeline/i)).toBeInTheDocument()
  })

  it('selects_a_node_by_keyboard_and_links_to_evidence', async () => {
    renderPage(graph)
    const node = await screen.findByRole('button', { name: 'Maya Chen, Person' })
    fireEvent.keyDown(node, { key: 'Enter' })
    expect(screen.getByRole('link', { name: /open evidence/i })).toHaveAttribute('href', `/evidence?entity=${A}`)
    expect(screen.getByText('Connections (1)')).toBeInTheDocument()
    expect(screen.getByRole('button', { name: 'Navigation Team, Team' })).toHaveAttribute('aria-pressed', 'false')
  })

  it('finds_an_entity_by_name', async () => {
    renderPage(graph)
    fireEvent.change(await screen.findByLabelText('Find entity'), { target: { value: 'navig' } })
    fireEvent.click(within(screen.getByRole('list', { name: 'Matching entities' })).getByRole('button'))
    expect(screen.getByRole('button', { name: 'Navigation Team, Team' })).toHaveAttribute('aria-pressed', 'true')
  })

  it('prefills_ask_with_the_multi_hop_question', async () => {
    renderPage(graph)
    fireEvent.click(await screen.findByRole('button', { name: /multi-hop/i }))
    expect(sessionStorage.getItem(ASK_PREFILL_KEY)).toBe(MULTI_HOP_QUESTION)
    expect(screen.getByText('ask page')).toBeInTheDocument()
  })
})
