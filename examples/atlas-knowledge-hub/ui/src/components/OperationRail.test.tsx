import { describe, expect, it, vi } from 'vitest'
import { render, screen } from '@testing-library/react'
import { OperationRow } from './OperationRail'
import type { OperationDoc } from '../api/ingest'

const base = { operation_id: 'op-1', submission: {}, accepted_at: '2026-10-03T10:00:00Z', started_at: '2026-10-03T10:00:01Z' }

const succeeded = (outcome: 'Ingested' | 'Unchanged'): OperationDoc => ({
  ...base,
  status: 'Succeeded',
  terminal_report: { operation_id: 'op-1', status: 'Succeeded', outcome, content_uri: 'u', error: null, partial_output_disposition: 'none' },
})

describe('OperationRail rows', () => {
  it('renders_each_outcome', () => {
    const { rerender } = render(<OperationRow sourceUri="a.md" doc={succeeded('Ingested')} />)
    expect(screen.getByText('Ingested')).toBeInTheDocument()

    rerender(<OperationRow sourceUri="a.md" doc={succeeded('Unchanged')} />)
    expect(screen.getByText('Unchanged')).toBeInTheDocument()
    expect(screen.queryByRole('alert')).not.toBeInTheDocument()

    rerender(
      <OperationRow
        sourceUri="a.md"
        doc={{
          ...base,
          status: 'Failed',
          terminal_report: {
            operation_id: 'op-1',
            status: 'Failed',
            outcome: null,
            content_uri: null,
            error: { code: 'embed_failed', message: 'Ollama refused the request', retryable: true },
            partial_output_disposition: 'none',
          },
        }}
      />,
    )
    expect(screen.getByText('embed_failed')).toBeInTheDocument()
    expect(screen.getByText(/Ollama refused the request/)).toBeInTheDocument()
    expect(screen.getByText(/retry/i)).toBeInTheDocument()

    vi.useFakeTimers()
    vi.setSystemTime(new Date('2026-10-03T10:00:09Z'))
    rerender(<OperationRow sourceUri="a.md" doc={{ ...base, status: 'Running', terminal_report: null }} />)
    expect(screen.getByText('Running')).toBeInTheDocument()
    expect(screen.getByText('8s')).toBeInTheDocument()
    vi.useRealTimers()
  })

  it('shows_replay_badge', () => {
    render(<OperationRow sourceUri="a.md" replay doc={succeeded('Unchanged')} />)
    expect(screen.getByText('Replay')).toBeInTheDocument()
  })
})
