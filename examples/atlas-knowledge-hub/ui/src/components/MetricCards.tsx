import type { ReactNode } from 'react'
import type { DemoMetrics, MetricCounter, MetricHistogram } from '../api/types'
import { Card } from './Card'
import { Stat } from './Stat'

/** Mean of a histogram (`sum / count`); null when nothing was recorded. */
export function histogramMean(h: Pick<MetricHistogram, 'count' | 'sum'>): number | null {
  return h.count > 0 && Number.isFinite(h.sum) ? h.sum / h.count : null
}

function formatSeconds(s: number | null): string {
  if (s === null) return '-'
  return s < 1 ? `${Math.round(s * 1000)} ms` : `${s.toFixed(2)} s`
}

/** Counter values can arrive as null when the engine reports a non-finite number. */
const val = (c: MetricCounter) => (Number.isFinite(c.value) ? c.value : 0)

function sumBy(counters: MetricCounter[], name: string, label: string): Map<string, number> {
  const out = new Map<string, number>()
  for (const c of counters) {
    if (c.name !== name) continue
    const key = c.labels[label] ?? 'unknown'
    out.set(key, (out.get(key) ?? 0) + val(c))
  }
  return out
}

function pooledMean(histograms: MetricHistogram[], name: string, match?: (h: MetricHistogram) => boolean): number | null {
  let count = 0
  let sum = 0
  for (const h of histograms) {
    if (h.name !== name || (match && !match(h))) continue
    count += h.count
    sum += Number.isFinite(h.sum) ? h.sum : 0
  }
  return histogramMean({ count, sum })
}

function Section({ title, children }: { title: string; children: ReactNode }) {
  return (
    <Card role="region" aria-label={title} className="p-4">
      <h3 className="mb-3 text-sm font-semibold">{title}</h3>
      {children}
    </Card>
  )
}

function CountList({ rows, empty }: { rows: [string, number][]; empty: string }) {
  if (rows.length === 0) return <p className="text-xs text-muted">{empty}</p>
  return (
    <dl className="space-y-1 text-xs">
      {rows.map(([k, v]) => (
        <div key={k} className="flex justify-between gap-3">
          <dt className="font-mono">{k}</dt>
          <dd className="font-mono tabular-nums">{v}</dd>
        </div>
      ))}
    </dl>
  )
}

export function MetricCards({ snapshot }: { snapshot: DemoMetrics }) {
  const { counters, histograms } = snapshot

  const reqOk = new Map<string, number>()
  const reqErr = new Map<string, number>()
  for (const c of counters) {
    if (c.name !== 'arcanum_requests_total') continue
    const endpoint = c.labels.endpoint ?? 'unknown'
    const target = c.labels.status === 'ok' ? reqOk : reqErr
    target.set(endpoint, (target.get(endpoint) ?? 0) + val(c))
  }
  const endpoints = [...new Set([...reqOk.keys(), ...reqErr.keys()])].sort()

  const generation = [...sumBy(counters, 'arcanum_generation_total', 'status')]
  const verify = [...sumBy(counters, 'arcanum_verify_requests_total', 'outcome')]
  const ingested = [...sumBy(counters, 'arcanum_ingest_docs_total', 'status')].find(([k]) => k === 'ok')?.[1] ?? 0
  const retrievers = counters.find((c) => c.name === 'arcanum_active_retrievers')

  return (
    <div className="grid gap-4 md:grid-cols-2">
      <Section title="Requests">
        {endpoints.length === 0 ? (
          <p className="text-xs text-muted">No requests recorded yet.</p>
        ) : (
          <table className="w-full text-left text-xs">
            <thead className="text-muted">
              <tr>
                <th scope="col" className="py-1 font-medium">Endpoint</th>
                <th scope="col" className="py-1 text-right font-medium">OK</th>
                <th scope="col" className="py-1 text-right font-medium">Error</th>
                <th scope="col" className="py-1 text-right font-medium">Mean</th>
              </tr>
            </thead>
            <tbody className="font-mono tabular-nums">
              {endpoints.map((e) => (
                <tr key={e}>
                  <td className="py-0.5">{e}</td>
                  <td className="py-0.5 text-right">{reqOk.get(e) ?? 0}</td>
                  <td className="py-0.5 text-right">{reqErr.get(e) ?? 0}</td>
                  <td className="py-0.5 text-right">
                    {formatSeconds(pooledMean(histograms, 'arcanum_request_duration_seconds', (h) => h.labels.endpoint === e))}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
      </Section>
      <Section title="Generation">
        <CountList rows={generation} empty="No generations recorded yet." />
        <p className="mt-3 text-xs text-muted">
          Mean duration <span className="font-mono">{formatSeconds(pooledMean(histograms, 'arcanum_generation_duration_seconds'))}</span>
        </p>
      </Section>
      <Section title="Verify">
        <CountList rows={verify} empty="No verifications recorded yet." />
        <p className="mt-3 text-xs text-muted">
          Mean duration <span className="font-mono">{formatSeconds(pooledMean(histograms, 'arcanum_verify_duration_seconds'))}</span>
        </p>
      </Section>
      <Section title="Ingest and retrieval">
        <div className="flex gap-8">
          <Stat label="Docs ingested" value={ingested} />
          <Stat label="Active retrievers" value={retrievers ? val(retrievers) : '-'} />
        </div>
      </Section>
    </div>
  )
}
