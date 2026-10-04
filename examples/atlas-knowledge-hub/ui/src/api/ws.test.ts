import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { backoffDelay, connectEvents, eventsUrl } from './ws'

class FakeSocket {
  static all: FakeSocket[] = []
  url: string
  protocols: string[]
  closed = false
  onopen: (() => void) | null = null
  onclose: (() => void) | null = null
  onmessage: ((e: { data: unknown }) => void) | null = null
  onerror: (() => void) | null = null
  constructor(url: string, protocols: string[]) {
    this.url = url
    this.protocols = protocols
    FakeSocket.all.push(this)
  }
  close() {
    this.closed = true
  }
  open() {
    this.onopen?.()
  }
  drop() {
    this.onclose?.()
  }
  send(data: unknown) {
    this.onmessage?.({ data })
  }
}

const factory = (url: string, protocols: string[]) => new FakeSocket(url, protocols) as unknown as WebSocket
const last = () => FakeSocket.all[FakeSocket.all.length - 1]

beforeEach(() => {
  FakeSocket.all = []
  vi.useFakeTimers()
})
afterEach(() => vi.useRealTimers())

describe('backoffDelay', () => {
  it('doubles from 250 ms to a 10 s cap', () => {
    expect([0, 1, 2, 3, 4, 5, 6].map(backoffDelay)).toEqual([250, 500, 1000, 2000, 4000, 8000, 10000])
    expect(backoffDelay(50)).toBe(10000)
    expect(backoffDelay(1000)).toBe(10000)
  })
})

describe('eventsUrl', () => {
  it('uses ws for http and wss for https', () => {
    expect(eventsUrl({ protocol: 'http:', host: 'localhost:8080' })).toBe('ws://localhost:8080/ws/events')
    expect(eventsUrl({ protocol: 'https:', host: 'a.example' })).toBe('wss://a.example/ws/events')
  })
})

describe('connectEvents', () => {
  function setup() {
    const events: unknown[] = []
    const statuses: string[] = []
    const dispose = connectEvents({
      url: 'ws://h/ws/events',
      key: 'k1',
      onEvent: (e) => events.push(e),
      onStatus: (s) => statuses.push(s),
      wsFactory: factory,
    })
    return { events, statuses, dispose }
  }

  it('connects with the arcanum-v1 and key protocols', () => {
    setup()
    expect(last().url).toBe('ws://h/ws/events')
    expect(last().protocols).toEqual(['arcanum-v1', 'k1'])
  })

  it('delivers event payloads and ignores other frames and bad JSON', () => {
    const { events, statuses } = setup()
    last().open()
    expect(statuses).toEqual(['open'])
    last().send(JSON.stringify({ type: 'event', payload: { a: 1 } }))
    last().send(JSON.stringify({ type: 'ping' }))
    last().send(JSON.stringify([1, 2]))
    last().send('not json{')
    last().send(42)
    last().send(JSON.stringify({ type: 'event', payload: 'x' }))
    expect(events).toEqual([{ a: 1 }, 'x'])
  })

  it('reconnects after the backoff delay and reports transitions', () => {
    const { statuses } = setup()
    last().drop()
    expect(statuses).toEqual(['closed', 'retrying'])
    expect(FakeSocket.all).toHaveLength(1)
    vi.advanceTimersByTime(249)
    expect(FakeSocket.all).toHaveLength(1)
    vi.advanceTimersByTime(1)
    expect(FakeSocket.all).toHaveLength(2)
    // second failure without opening waits 500
    last().drop()
    vi.advanceTimersByTime(499)
    expect(FakeSocket.all).toHaveLength(2)
    vi.advanceTimersByTime(1)
    expect(FakeSocket.all).toHaveLength(3)
  })

  it('resets the attempt counter when a connection opens', () => {
    setup()
    last().drop()
    vi.advanceTimersByTime(250)
    last().drop()
    vi.advanceTimersByTime(500)
    expect(FakeSocket.all).toHaveLength(3)
    last().open()
    last().drop()
    vi.advanceTimersByTime(249)
    expect(FakeSocket.all).toHaveLength(3)
    vi.advanceTimersByTime(1)
    expect(FakeSocket.all).toHaveLength(4)
  })

  it('stops reconnecting and closes the socket when disposed', () => {
    const { dispose, statuses } = setup()
    const s = last()
    dispose()
    expect(s.closed).toBe(true)
    s.drop()
    vi.advanceTimersByTime(60000)
    expect(FakeSocket.all).toHaveLength(1)
    expect(statuses).toEqual([])
  })

  it('cancels a pending reconnect timer on dispose', () => {
    const { dispose } = setup()
    last().drop()
    dispose()
    vi.advanceTimersByTime(60000)
    expect(FakeSocket.all).toHaveLength(1)
  })

  it('does not deliver events after dispose', () => {
    const { dispose, events } = setup()
    const s = last()
    dispose()
    s.send(JSON.stringify({ type: 'event', payload: 1 }))
    expect(events).toEqual([])
  })
})
