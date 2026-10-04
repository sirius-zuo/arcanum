import { describe, expect, it, vi } from 'vitest'
import { render } from '@testing-library/react'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import type { ConnectOptions } from '../api/ws'
import { IngestEventsProvider } from './ingestEvents'

const captured: { opts: ConnectOptions | null; disposed: number } = { opts: null, disposed: 0 }

vi.mock('../api/ws', async (orig) => ({
  ...(await orig<typeof import('../api/ws')>()),
  connectEvents: (opts: ConnectOptions) => {
    captured.opts = opts
    return () => {
      captured.disposed += 1
    }
  },
}))
vi.mock('./bootstrap', () => ({ useBootstrap: () => ({ data: { api_key: 'k' } }) }))

describe('IngestEventsProvider', () => {
  it('events_only_nudge_the_matching_operation', () => {
    const qc = new QueryClient()
    qc.setQueryData(['operation', 'mine'], { status: 'Running' })
    qc.setQueryData(['operation', 'done'], { status: 'Succeeded' })
    const spy = vi.spyOn(qc, 'invalidateQueries')
    const view = render(
      <QueryClientProvider client={qc}>
        <IngestEventsProvider>
          <p>x</p>
        </IngestEventsProvider>
      </QueryClientProvider>,
    )
    const send = captured.opts!.onEvent
    send({ operation_id: 'someone-else', status: 'completed' })
    send({ operation_id: 'done', status: 'completed' })
    send('garbage')
    expect(spy).not.toHaveBeenCalled()
    send({ operation_id: 'mine', status: 'completed' })
    expect(spy).toHaveBeenCalledWith({ queryKey: ['operation', 'mine'] })
    view.unmount()
    expect(captured.disposed).toBe(1)
  })
})
