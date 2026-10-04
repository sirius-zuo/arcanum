import { afterEach, describe, expect, it, vi } from 'vitest'
import { fireEvent, render, screen, waitFor } from '@testing-library/react'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { MemoryRouter } from 'react-router-dom'
import { createClient } from '../api/client'
import ContextPage from '../pages/ContextPage'
import { CommandActionsProvider } from './commandActions'
import { TourProvider, useTour } from './tour'

vi.mock('./bootstrap', () => ({
  useBootstrap: () => ({ data: { collection: 'halcyon' }, client: createClient(() => 'k') }),
}))
vi.mock('../state/bootstrap', () => ({
  useBootstrap: () => ({ data: { collection: 'halcyon' }, client: createClient(() => 'k') }),
}))

const tour = [
  { id: 'load', title: 'Load', why: 'w', action: 'a', route: '/library', completes_when: 'corpus_loaded', payoff: 'p' },
  { id: 'update', title: 'Update', why: 'w', action: 'a', route: '/library', completes_when: 'update_applied', payoff: 'p' },
  { id: 'context', title: 'Context', why: 'w', action: 'a', route: '/context', completes_when: 'context_built', payoff: 'p' },
]
const docs = Array.from({ length: 10 }, (_, i) => ({
  source_uri: i === 0 ? 'security-policy.md' : `d${i}.md`,
  document_id: `d${i}`,
  chunks: 1,
  versions: i === 0 ? [{ version_num: 1 }, { version_num: 2 }] : [{ version_num: 1 }],
}))

function Probe() {
  const { state } = useTour()
  return <p data-testid="done">{Object.keys(state.completed).sort().join(',')}</p>
}

function mount(children: React.ReactNode) {
  return render(
    <QueryClientProvider client={new QueryClient()}>
      <MemoryRouter>
        <CommandActionsProvider>
          <TourProvider>
            <Probe />
            {children}
          </TourProvider>
        </CommandActionsProvider>
      </MemoryRouter>
    </QueryClientProvider>,
  )
}

afterEach(() => vi.unstubAllGlobals())

describe('TourProvider', () => {
  it('derives_corpus_loaded_and_update_applied_from_library_data', async () => {
    localStorage.clear()
    vi.stubGlobal(
      'fetch',
      vi.fn(async (url: string) =>
        new Response(JSON.stringify(url === '/demo/samples' ? { files: [], golden: [], flawed_answers: [], tour } : { collection: 'halcyon', documents: docs }), { status: 200 }),
      ),
    )
    mount(null)
    await waitFor(() => expect(screen.getByTestId('done')).toHaveTextContent('load,update'))
  })

  it('context_page_signals_context_built_after_a_build', async () => {
    localStorage.clear()
    const body = {
      resolved_query: 'q', resolved_query_source: 'original', passages: [], background: [],
      usage: { budget: 4000, used: 0, passages: 0, background: 0, dropped_passages: 0, counter: 'c' },
      retrieval: { queries: ['q'], strategies_ok: ['vector'], strategies_failed: [] }, rendered: '<documents/>',
    }
    vi.stubGlobal(
      'fetch',
      vi.fn(async (url: string) =>
        new Response(JSON.stringify(url === '/demo/samples' ? { files: [], golden: [], flawed_answers: [], tour } : url === '/demo/library' ? { collection: 'halcyon', documents: [] } : body), { status: 200 }),
      ),
    )
    mount(<ContextPage />)
    await waitFor(() => expect(vi.mocked(fetch).mock.calls.some((c) => c[0] === '/demo/samples')).toBe(true))
    await new Promise((r) => setTimeout(r, 20))
    fireEvent.change(screen.getByLabelText('Query'), { target: { value: 'hello' } })
    fireEvent.click(screen.getByRole('button', { name: /build context/i }))
    await waitFor(() => expect(screen.getByTestId('done')).toHaveTextContent('context'))
  })
})
