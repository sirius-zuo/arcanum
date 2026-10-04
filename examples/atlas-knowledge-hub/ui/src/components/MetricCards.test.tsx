import { describe, expect, it } from 'vitest'
import { render, screen } from '@testing-library/react'
import { MetricCards, histogramMean } from './MetricCards'
import type { DemoMetrics } from '../api/types'

const snapshot: DemoMetrics = {
  counters: [
    { name: 'arcanum_requests_total', labels: { endpoint: 'context', status: 'ok' }, value: 4 },
    { name: 'arcanum_requests_total', labels: { endpoint: 'context', status: 'error' }, value: 1 },
    { name: 'arcanum_requests_total', labels: { endpoint: 'verify', status: 'ok' }, value: 2 },
    { name: 'arcanum_generation_total', labels: { generator: 'local', mode: 'answer', status: 'ok' }, value: 3 },
    { name: 'arcanum_generation_total', labels: { generator: 'local', mode: 'answer', status: 'no_context' }, value: 1 },
    { name: 'arcanum_verify_requests_total', labels: { outcome: 'pass' }, value: 5 },
    { name: 'arcanum_verify_requests_total', labels: { outcome: 'fail' }, value: 2 },
    { name: 'arcanum_ingest_docs_total', labels: { source: 'a.md', status: 'ok' }, value: 8 },
    { name: 'arcanum_ingest_docs_total', labels: { source: 'b.md', status: 'ok' }, value: 4 },
    { name: 'arcanum_active_retrievers', labels: {}, value: 3 },
  ],
  histograms: [
    { name: 'arcanum_request_duration_seconds', labels: { endpoint: 'context' }, count: 4, sum: 2 },
    { name: 'arcanum_request_duration_seconds', labels: { endpoint: 'verify' }, count: 0, sum: 0 },
    { name: 'arcanum_generation_duration_seconds', labels: { generator: 'local' }, count: 2, sum: 5 },
  ],
}

describe('MetricCards', () => {
  it('computes_histogram_mean_and_groups_by_label', () => {
    expect(histogramMean({ count: 4, sum: 2 })).toBeCloseTo(0.5)
    expect(histogramMean({ count: 0, sum: 0 })).toBeNull()

    render(<MetricCards snapshot={snapshot} />)
    const requests = screen.getByRole('region', { name: 'Requests' })
    expect(requests).toHaveTextContent('context')
    expect(requests).toHaveTextContent('500 ms')
    // verify has a zero count histogram: mean is a dash, not NaN
    const verifyRow = screen.getByText('verify').closest('tr') as HTMLElement
    expect(verifyRow).toHaveTextContent('-')
    expect(verifyRow).not.toHaveTextContent('NaN')

    const generation = screen.getByRole('region', { name: 'Generation' })
    expect(generation).toHaveTextContent('no_context')
    expect(generation).toHaveTextContent('2.50 s')
    expect(screen.getByRole('region', { name: 'Verify' })).toHaveTextContent('pass')
    expect(screen.getByRole('region', { name: 'Ingest and retrieval' })).toHaveTextContent('12')
    expect(screen.getByRole('region', { name: 'Ingest and retrieval' })).toHaveTextContent('3')
  })
})
