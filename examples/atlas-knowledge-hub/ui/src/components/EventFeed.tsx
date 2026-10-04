import { useEffect, useState } from 'react'
import { connectEvents, eventsUrl } from '../api/ws'
import type { WsStatus } from '../api/ws'
import { Chip } from './Chip'

const MAX_EVENTS = 50

interface FeedEvent {
  n: number
  operationId: string
  status: string
  reason: string | null
}

const statusLabel: Record<WsStatus, string> = { open: 'Live', closed: 'Disconnected', retrying: 'Reconnecting' }

function toEvent(raw: unknown, n: number): FeedEvent {
  const o = typeof raw === 'object' && raw !== null ? (raw as Record<string, unknown>) : {}
  const str = (v: unknown) => (typeof v === 'string' && v ? v : null)
  return { n, operationId: str(o.operation_id) ?? 'unknown', status: str(o.status) ?? 'unknown', reason: str(o.reason) }
}

interface EventFeedProps {
  apiKey: string
  wsFactory?: (url: string, protocols: string[]) => WebSocket
}

/** Live ingestion events from the shared socket, newest first, last 50 kept. */
export function EventFeed({ apiKey, wsFactory }: EventFeedProps) {
  const [status, setStatus] = useState<WsStatus | 'connecting'>('connecting')
  const [events, setEvents] = useState<FeedEvent[]>([])

  useEffect(() => {
    let n = 0
    return connectEvents({
      url: eventsUrl(window.location),
      key: apiKey,
      wsFactory,
      onStatus: setStatus,
      onEvent: (raw) => {
        n += 1
        const ev = toEvent(raw, n)
        setEvents((prev) => [ev, ...prev].slice(0, MAX_EVENTS))
      },
    })
  }, [apiKey, wsFactory])

  const label = status === 'connecting' ? 'Connecting' : statusLabel[status]
  return (
    <div>
      <div className="mb-3 flex items-center gap-2" role="status">
        <Chip tone={status === 'open' ? 'supported' : status === 'connecting' ? 'neutral' : 'partial'}>{label}</Chip>
        <span className="text-xs text-muted">Last {MAX_EVENTS} ingestion events from all clients</span>
      </div>
      {events.length === 0 ? (
        <p className="text-sm text-muted">No events yet. Load the sample corpus on the Library page to see some.</p>
      ) : (
        <ul className="max-h-72 divide-y divide-border overflow-y-auto rounded-lg border border-border">
          {events.map((e) => (
            <li key={e.n} className="flex flex-wrap items-center gap-2 px-3 py-1.5 text-xs">
              <span className="font-mono">{e.operationId}</span>
              <Chip mono>{e.status}</Chip>
              {e.reason && <span className="text-muted">{e.reason}</span>}
            </li>
          ))}
        </ul>
      )}
    </div>
  )
}
