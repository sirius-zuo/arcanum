export type WsStatus = 'open' | 'closed' | 'retrying'

const BASE_MS = 250
const CAP_MS = 10_000

/** 250 ms doubling per attempt, capped at 10 s. Deterministic on purpose. */
export function backoffDelay(attempt: number): number {
  return Math.min(CAP_MS, BASE_MS * 2 ** Math.max(0, Math.min(attempt, 30)))
}

export function eventsUrl(loc: { protocol: string; host: string }): string {
  const scheme = loc.protocol === 'https:' ? 'wss:' : 'ws:'
  return `${scheme}//${loc.host}/ws/events`
}

export interface ConnectOptions {
  url: string
  key: string
  onEvent(e: unknown): void
  onStatus(s: WsStatus): void
  wsFactory?: (url: string, protocols: string[]) => WebSocket
}

/** Connect to the events socket, reconnecting with backoff. Returns a disposer. */
export function connectEvents(opts: ConnectOptions): () => void {
  const make = opts.wsFactory ?? ((url: string, protocols: string[]) => new WebSocket(url, protocols))
  let socket: WebSocket | null = null
  let timer: ReturnType<typeof setTimeout> | null = null
  let attempt = 0
  let disposed = false

  function connect() {
    timer = null
    const ws = make(opts.url, ['arcanum-v1', opts.key])
    socket = ws
    ws.onopen = () => {
      if (disposed) return
      attempt = 0
      opts.onStatus('open')
    }
    ws.onmessage = (msg: MessageEvent) => {
      if (disposed || typeof msg.data !== 'string') return
      let frame: unknown
      try {
        frame = JSON.parse(msg.data)
      } catch {
        return
      }
      if (typeof frame === 'object' && frame !== null && !Array.isArray(frame)) {
        const f = frame as { type?: unknown; payload?: unknown }
        if (f.type === 'event') opts.onEvent(f.payload)
      }
    }
    ws.onclose = () => {
      if (disposed || socket !== ws) return
      socket = null
      opts.onStatus('closed')
      opts.onStatus('retrying')
      timer = setTimeout(connect, backoffDelay(attempt))
      attempt += 1
    }
  }

  connect()

  return () => {
    disposed = true
    if (timer !== null) clearTimeout(timer)
    timer = null
    if (socket) {
      socket.onopen = socket.onmessage = socket.onclose = null
      socket.close()
      socket = null
    }
  }
}
