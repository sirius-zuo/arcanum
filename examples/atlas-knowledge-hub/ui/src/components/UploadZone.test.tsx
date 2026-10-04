import { afterEach, describe, expect, it, vi } from 'vitest'
import { render, screen, waitFor, fireEvent } from '@testing-library/react'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { createClient } from '../api/client'
import { OperationsProvider, useOperations } from '../state/operations'
import { OperationRail } from './OperationRail'
import { UploadZone } from './UploadZone'

vi.mock('../state/bootstrap', () => ({
  useBootstrap: () => ({ data: { api_key: 'k' }, client: createClient(() => 'k') }),
}))

function Page() {
  const { ops } = useOperations()
  return (
    <>
      <UploadZone />
      <OperationRail ops={ops} />
    </>
  )
}

describe('upload flow', () => {
  afterEach(() => {
    vi.unstubAllGlobals()
    localStorage.clear()
  })

  it('a_200_replay_of_a_tracked_operation_shows_the_replay_badge_in_the_rail', async () => {
    const op = { operation_id: 'op-1', submission: {}, status: 'Succeeded', accepted_at: 'now', started_at: null, terminal_report: { operation_id: 'op-1', status: 'Succeeded', outcome: 'Unchanged', content_uri: 'u', error: null, partial_output_disposition: 'none' } }
    let posts = 0
    vi.stubGlobal(
      'fetch',
      vi.fn(async (_url: string, init?: RequestInit) => {
        if (init?.method === 'POST') {
          posts += 1
          return new Response(JSON.stringify({ operation_id: 'op-1', status: 'accepted', resource: 'r' }), { status: posts === 1 ? 202 : 200 })
        }
        return new Response(JSON.stringify(op), { status: 200 })
      }),
    )
    render(
      <QueryClientProvider client={new QueryClient()}>
        <OperationsProvider>
          <Page />
        </OperationsProvider>
      </QueryClientProvider>,
    )
    const input = screen.getByLabelText('Choose files to upload')
    const file = new File(['# a'], 'a.md', { type: 'text/markdown' })
    fireEvent.change(input, { target: { files: [file] } })
    await waitFor(() => expect(screen.getAllByText('a.md').length).toBeGreaterThan(0))
    expect(screen.queryByText('Replay', { selector: 'span' })).not.toBeInTheDocument()
    fireEvent.change(input, { target: { files: [file] } })
    await waitFor(() => expect(screen.getAllByText('Replay').length).toBeGreaterThan(0))
    expect(document.querySelectorAll('ul[class*="divide-y"] > li')).toHaveLength(1)
  })
})
