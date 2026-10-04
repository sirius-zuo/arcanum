import { Check, X } from 'lucide-react'
import type { DemoMetrics } from '../api/types'
import { Chip } from './Chip'
import type { ChipTone } from './Chip'

export interface ServerStatus {
  ready: boolean
  alive: boolean
}

const BREAKER: Record<number, { label: string; tone: ChipTone }> = {
  0: { label: 'Closed', tone: 'supported' },
  1: { label: 'Open', tone: 'unsupported' },
  2: { label: 'Half open', tone: 'partial' },
}

function Row({ label, ok, okText, badText }: { label: string; ok: boolean; okText: string; badText: string }) {
  const Icon = ok ? Check : X
  return (
    <li className="flex items-center justify-between gap-3 py-2 text-sm">
      <span>{label}</span>
      <Chip tone={ok ? 'supported' : 'unsupported'} icon={<Icon className="h-3 w-3" aria-hidden="true" />}>
        {ok ? okText : badText}
      </Chip>
    </li>
  )
}

/**
 * Liveness and readiness come from `/health` and `/ready`. Breaker state is only exposed as the
 * `arcanum_circuit_breaker_state` gauge, so it is read from the metrics snapshot when there is one.
 */
export function ReadinessPanel({ status, metrics }: { status: ServerStatus | undefined; metrics: DemoMetrics | null }) {
  const breakers = (metrics?.counters ?? []).filter((c) => c.name === 'arcanum_circuit_breaker_state')
  const trips = (metrics?.counters ?? []).filter((c) => c.name === 'arcanum_circuit_breaker_trips_total')
  return (
    <div>
      {status ? (
        <ul className="divide-y divide-border">
          <Row label="Liveness (/health)" ok={status.alive} okText="Alive" badText="Not responding" />
          <Row label="Readiness (/ready)" ok={status.ready} okText="Ready" badText="Not ready" />
        </ul>
      ) : (
        <p className="text-sm text-muted">Checking the server.</p>
      )}
      <h3 className="mb-1 mt-4 text-xs font-semibold uppercase tracking-wider text-muted">Circuit breakers</h3>
      {breakers.length === 0 ? (
        <p className="text-xs text-muted">
          No breaker has reported yet. The engine publishes breaker state only after a guarded model call, and only when metrics are available.
        </p>
      ) : (
        <ul className="divide-y divide-border">
          {breakers.map((b) => {
            const state = BREAKER[Number(b.value)] ?? { label: 'Unknown', tone: 'neutral' as ChipTone }
            const tripCount = trips.find((t) => t.labels.breaker === b.labels.breaker)?.value ?? 0
            return (
              <li key={b.labels.breaker ?? 'breaker'} className="flex items-center justify-between gap-3 py-2 text-sm">
                <span className="font-mono text-xs">{b.labels.breaker ?? 'unnamed'}</span>
                <span className="flex items-center gap-2">
                  <span className="text-xs text-muted">{tripCount} trips</span>
                  <Chip tone={state.tone}>{state.label}</Chip>
                </span>
              </li>
            )
          })}
        </ul>
      )}
    </div>
  )
}
