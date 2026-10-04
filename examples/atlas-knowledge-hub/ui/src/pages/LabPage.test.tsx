import { afterEach, describe, expect, it, vi } from 'vitest'
import { fireEvent, render, screen, waitFor } from '@testing-library/react'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { MemoryRouter } from 'react-router-dom'
import { createClient } from '../api/client'
import LabPage from './LabPage'

vi.mock('../state/bootstrap', () => ({
  useBootstrap: () => ({ data: { collection: 'halcyon' }, client: createClient(() => 'k') }),
}))

afterEach(() => vi.unstubAllGlobals())

const version = { version_num: 1, status: 'Active', ingested_at: 'x', content_hash: 'h', snapshot_uri: 's' }
const json = (b: unknown) => new Response(JSON.stringify(b), { status: 200 })

describe('LabPage', () => {
  it('ignores_a_late_compare_result_for_a_previously_selected_document', async () => {
    let release: (r: Response) => void = () => {}
    vi.stubGlobal(
      'fetch',
      vi.fn(async (url: string) => {
        if (url === '/demo/library')
          return json({ collection: 'halcyon', documents: [
            { source_uri: 'a.md', document_id: 'd1', chunks: 1, versions: [version] },
            { source_uri: 'b.md', document_id: 'd2', chunks: 1, versions: [version] },
          ] })
        if (url === '/demo/samples') return json({ files: [], golden: [], flawed_answers: [], tour: [] })
        if (url.startsWith('/demo/documents/')) return json({ document_id: 'x', version_num: 1, source_uri: 'a.md', status: 'Active', mime_type: 'text/plain', text: 'late result text' })
        if (url === '/api/v1/chunk/inspect') return new Promise<Response>((r) => (release = r))
        return new Response('{}', { status: 404 })
      }),
    )
    render(
      <QueryClientProvider client={new QueryClient()}>
        <MemoryRouter>
          <LabPage />
        </MemoryRouter>
      </QueryClientProvider>,
    )
    const select = await screen.findByLabelText('Sample document')
    await waitFor(() => expect(screen.getByRole('button', { name: /compare strategies/i })).toBeEnabled())
    fireEvent.click(screen.getByRole('button', { name: /compare strategies/i }))
    fireEvent.change(select, { target: { value: 'd2' } })
    await waitFor(() => expect(fetch).toHaveBeenCalledWith('/api/v1/chunk/inspect', expect.anything()))
    release(
      json({
        results: [{ strategy: { strategy: 'fixed', params: {} }, chunks: [{ text: 'late result text', char_count: 16, token_estimate: 4, overlap_chars: 0 }], total_chunks: 1, mean_tokens: 4 }],
      }),
    )
    await new Promise((r) => setTimeout(r, 50))
    expect(screen.queryByTestId('chunk-text')).toBeNull()
  })
})
