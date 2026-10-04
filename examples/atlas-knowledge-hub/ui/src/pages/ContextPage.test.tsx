import { describe, expect, it, vi } from 'vitest'
import { fireEvent, render, screen, waitFor } from '@testing-library/react'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { MemoryRouter } from 'react-router-dom'
import { createClient } from '../api/client'
import ContextPage from './ContextPage'

vi.mock('../state/bootstrap', () => ({
  useBootstrap: () => ({ data: { collection: 'halcyon' }, client: createClient(() => 'k') }),
}))

const body = (over: object) => ({
  resolved_query: 'q',
  resolved_query_source: 'original',
  passages: [],
  background: [],
  usage: { budget: 4000, used: 0, passages: 0, background: 0, dropped_passages: 0, counter: 'c' },
  retrieval: { queries: ['q'], strategies_ok: ['vector'], strategies_failed: [{ strategy: 'colbert', reason: 'timed out' }] },
  rendered: '<documents/>',
  ...over,
})

function renderPage() {
  return render(
    <QueryClientProvider client={new QueryClient()}>
      <MemoryRouter>
        <ContextPage />
      </MemoryRouter>
    </QueryClientProvider>,
  )
}

describe('ContextPage', () => {
  it('renders_a_failed_strategy_object_without_throwing', async () => {
    vi.stubGlobal('fetch', vi.fn(async () => new Response(JSON.stringify(body({})), { status: 200 })))
    renderPage()
    fireEvent.change(screen.getByLabelText('Query'), { target: { value: 'hello' } })
    fireEvent.click(screen.getByRole('button', { name: /build context/i }))
    await waitFor(() => expect(screen.getByText(/colbert: timed out/)).toBeInTheDocument())
    vi.unstubAllGlobals()
  })

  it('distinguishes_503_causes', async () => {
    vi.stubGlobal('fetch', vi.fn(async () => new Response(JSON.stringify({ error: 'all strategies failed' }), { status: 503 })))
    renderPage()
    fireEvent.change(screen.getByLabelText('Query'), { target: { value: 'hello' } })
    fireEvent.click(screen.getByRole('button', { name: /build context/i }))
    await waitFor(() => expect(screen.getByText('Retrieval unavailable')).toBeInTheDocument())
    vi.unstubAllGlobals()
  })
})
