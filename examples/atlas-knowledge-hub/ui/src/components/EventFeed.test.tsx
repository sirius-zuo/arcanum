import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { act, render, screen } from '@testing-library/react'
import { EventFeed } from './EventFeed'

class FakeSocket {
  static all: FakeSocket[] = []
  onopen: (() => void) | null = null
  onclose: (() => void) | null = null
  onmessage: ((e: { data: unknown }) => void) | null = null
  constructor(
    public url: string,
    public protocols: string[],
  ) {
    FakeSocket.all.push(this)
  }
  close() {}
  open() {
    this.onopen?.()
  }
  drop() {
    this.onclose?.()
  }
  event(payload: unknown) {
    this.onmessage?.({ data: JSON.stringify({ type: 'event', payload }) })
  }
}
const factory = (url: string, protocols: string[]) => new FakeSocket(url, protocols) as unknown as WebSocket

beforeEach(() => {
  FakeSocket.all = []
})
afterEach(() => vi.useRealTimers())

describe('EventFeed', () => {
  it('keeps_last_fifty_and_shows_status', () => {
    render(<EventFeed apiKey="k" wsFactory={factory} />)
    expect(screen.getByText('Connecting')).toBeInTheDocument()
    const ws = FakeSocket.all[0]
    expect(ws.protocols).toEqual(['arcanum-v1', 'k'])
    act(() => ws.open())
    expect(screen.getByText('Live')).toBeInTheDocument()
    act(() => {
      for (let i = 0; i < 60; i++) ws.event({ operation_id: `op-${i}`, status: 'running', reason: i === 59 ? 'embed_failed' : null })
    })
    expect(screen.getAllByRole('listitem')).toHaveLength(50)
    expect(screen.queryByText('op-9')).not.toBeInTheDocument()
    expect(screen.getByText('op-10')).toBeInTheDocument()
    // newest first
    expect(screen.getAllByRole('listitem')[0]).toHaveTextContent('op-59')
    expect(screen.getAllByRole('listitem')[0]).toHaveTextContent('embed_failed')
    act(() => ws.drop())
    expect(screen.getByText('Reconnecting')).toBeInTheDocument()
  })
})
