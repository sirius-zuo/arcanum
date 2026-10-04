import { afterEach, describe, expect, it, vi } from 'vitest'
import { fireEvent, render, screen, waitFor } from '@testing-library/react'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { MemoryRouter } from 'react-router-dom'
import { createClient } from '../api/client'
import EvidencePage from './EvidencePage'

vi.mock('../state/bootstrap', () => ({
  useBootstrap: () => ({ data: { collection: 'halcyon', features: { evidence: true } }, client: createClient(() => 'k') }),
}))

afterEach(() => vi.unstubAllGlobals())

const A = '11111111-1111-4111-8111-111111111111'
const chain = { root: { id: A, kind: 'Chunk', label: 'Leave policy chunk', metadata: {}, children: [] }, raw_sources: [] }

function renderAt(url: string) {
  return render(
    <QueryClientProvider client={new QueryClient()}>
      <MemoryRouter initialEntries={[url]}>
        <EvidencePage />
      </MemoryRouter>
    </QueryClientProvider>,
  )
}

describe('EvidencePage', () => {
  it('loads_the_chunk_from_the_url_on_mount', async () => {
    const fn = vi.fn(async (_url: string) => new Response(JSON.stringify(chain), { status: 200 }))
    vi.stubGlobal('fetch', fn)
    renderAt(`/evidence?chunk=${A}`)
    expect(await screen.findByText('Leave policy chunk')).toBeInTheDocument()
    expect(fn.mock.calls[0][0]).toBe(`/evidence/chunk/${A}`)
    expect(screen.getByLabelText(/chunk id/i)).toHaveValue(A)
  })

  it('loads_an_entity_from_the_url', async () => {
    const fn = vi.fn(async (_url: string) => new Response(JSON.stringify(chain), { status: 200 }))
    vi.stubGlobal('fetch', fn)
    renderAt(`/evidence?entity=${A}`)
    await screen.findByText('Leave policy chunk')
    expect(fn.mock.calls[0][0]).toBe(`/evidence/entity/${A}`)
  })

  it('rejects_a_bad_uuid_without_a_request', () => {
    const fn = vi.fn()
    vi.stubGlobal('fetch', fn)
    renderAt('/evidence')
    fireEvent.change(screen.getByLabelText(/chunk id/i), { target: { value: 'nope' } })
    fireEvent.click(screen.getByRole('button', { name: /look up/i }))
    expect(screen.getByRole('alert')).toHaveTextContent(/not a valid uuid/i)
    expect(fn).not.toHaveBeenCalled()
  })

  it('explains_404_and_503', async () => {
    vi.stubGlobal('fetch', vi.fn(async () => new Response(JSON.stringify({ error: 'not found' }), { status: 404 })))
    renderAt(`/evidence?chunk=${A}`)
    await waitFor(() => expect(screen.getByRole('alert')).toHaveTextContent(/no evidence found/i))
    vi.stubGlobal('fetch', vi.fn(async () => new Response(JSON.stringify({ error: 'evidence resolver not configured' }), { status: 503 })))
    fireEvent.click(screen.getByRole('button', { name: /look up/i }))
    await waitFor(() => expect(screen.getByRole('alert')).toHaveTextContent(/evidence resolver not configured/i))
  })

  it('relation_needs_three_fields_and_encodes_the_type', async () => {
    const fn = vi.fn(async (_url: string) => new Response(JSON.stringify(chain), { status: 200 }))
    vi.stubGlobal('fetch', fn)
    renderAt('/evidence')
    fireEvent.click(screen.getByRole('tab', { name: /relation/i }))
    fireEvent.change(screen.getByLabelText(/source entity id/i), { target: { value: A } })
    fireEvent.change(screen.getByLabelText(/relation type/i), { target: { value: 'works on/with' } })
    fireEvent.change(screen.getByLabelText(/target entity id/i), { target: { value: A } })
    fireEvent.click(screen.getByRole('button', { name: /look up/i }))
    await screen.findByText('Leave policy chunk')
    expect(fn.mock.calls[0][0]).toBe(`/evidence/relation/${A}/works%20on%2Fwith/${A}`)
  })
})
