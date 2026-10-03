import { afterEach, describe, expect, it, vi } from 'vitest'
import { renderHook, waitFor } from '@testing-library/react'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import type { ReactNode } from 'react'
import { ApiError, createClient } from './client'
import type { Client } from './client'
import { checkUploadFile, getOperation, submitUpload, useOperationStatus } from './ingest'
import type { OperationDoc } from './ingest'

vi.mock('../state/bootstrap', () => ({
  useBootstrap: () => ({ data: { api_key: 'k' }, client: mockClient.current }),
}))
const mockClient: { current: Client } = { current: createClient(() => 'k') }

function readText(b: Blob): Promise<string> {
  return new Promise((resolve, reject) => {
    const r = new FileReader()
    r.onload = () => resolve(String(r.result))
    r.onerror = () => reject(r.error)
    r.readAsText(b)
  })
}

function mockFetch(res: Response) {
  const fn = vi.fn().mockResolvedValue(res)
  vi.stubGlobal('fetch', fn)
  return fn
}

const accepted = (status: number) =>
  new Response(JSON.stringify({ operation_id: 'op-1', status: 'accepted', resource: '/api/v1/ingestion-operations/op-1' }), { status })

afterEach(() => {
  vi.unstubAllGlobals()
  vi.useRealTimers()
})

describe('submitUpload', () => {
  it('submit_upload_builds_multipart_and_detects_replay', async () => {
    const client = createClient(() => 'secret')
    const file = new File(['# Hello'], 'hello.md', { type: 'text/markdown' })

    const fn = mockFetch(accepted(202))
    await expect(submitUpload(client, file)).resolves.toEqual({ operation_id: 'op-1', replay: false })
    const [url, init] = fn.mock.calls[0] as [string, RequestInit]
    expect(url).toBe('/api/v1/ingestion-operations')
    expect(init.method).toBe('POST')
    const headers = new Headers(init.headers)
    expect(headers.get('Authorization')).toBe('Bearer secret')
    expect(headers.has('Content-Type')).toBe(false)
    const form = init.body as FormData
    expect(form).toBeInstanceOf(FormData)
    const metadata = form.get('metadata') as Blob
    expect(metadata.type).toBe('application/json')
    const meta = JSON.parse(await readText(metadata)) as Record<string, unknown>
    expect(Object.keys(meta).sort()).toEqual(['collection_id', 'idempotency_key', 'logical_source_uri', 'mime_hint', 'pipeline_configuration'])
    expect(meta.logical_source_uri).toBe('hello.md')
    expect(meta.collection_id).toBe('halcyon')
    expect(meta.mime_hint).toBe('text/markdown')
    expect(meta.pipeline_configuration).toEqual({ template: 'full' })
    expect(String(meta.idempotency_key)).toMatch(/^halcyon:[0-9a-f]{64}$/)
    const payload = form.get('payload') as File
    expect(payload.size).toBe(7)

    mockFetch(accepted(200))
    await expect(submitUpload(client, file)).resolves.toEqual({ operation_id: 'op-1', replay: true })
  })

  it('maps_409_413_503_to_readable_errors', async () => {
    const client = createClient(() => 'k')
    const file = new File(['x'], 'a.txt', { type: 'text/plain' })
    for (const [status, re] of [
      [409, /different/i],
      [413, /too large/i],
      [503, /busy|queue/i],
    ] as const) {
      mockFetch(new Response(JSON.stringify({ error: 'raw' }), { status }))
      const err = await submitUpload(client, file).catch((e: unknown) => e)
      expect(err).toBeInstanceOf(ApiError)
      expect((err as ApiError).status).toBe(status)
      expect((err as ApiError).message).toMatch(re)
    }
  })
})

describe('checkUploadFile', () => {
  it('rejects_pdf_with_a_reason', () => {
    const msg = checkUploadFile(new File(['x'], 'report.pdf'))
    expect(msg).toMatch(/\.md/)
    expect(msg).toMatch(/plain text/i)
  })

  it('accepts_md_and_txt_case_insensitively', () => {
    for (const name of ['a.md', 'A.MD', 'notes.txt', 'NOTES.TXT', 'x.Md']) {
      expect(checkUploadFile(new File(['x'], name))).toBeNull()
    }
  })
})

describe('useOperationStatus', () => {
  it('operation_status_stops_polling_on_terminal', async () => {
    const base = { operation_id: 'op-1', submission: {}, accepted_at: 'now', started_at: null }
    const docs: OperationDoc[] = [
      { ...base, status: 'Accepted', terminal_report: null },
      { ...base, status: 'Running', terminal_report: null },
      {
        ...base,
        status: 'Succeeded',
        terminal_report: { operation_id: 'op-1', status: 'Succeeded', outcome: 'Ingested', content_uri: 'u', error: null, partial_output_disposition: 'none' },
      },
    ]
    let n = 0
    const get = vi.fn(async () => docs[Math.min(n++, docs.length - 1)])
    mockClient.current = { get, post: vi.fn(), del: vi.fn(), raw: vi.fn() } as unknown as Client

    const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } })
    const wrapper = ({ children }: { children: ReactNode }) => <QueryClientProvider client={qc}>{children}</QueryClientProvider>
    const { result } = renderHook(() => useOperationStatus('op-1', { pollMs: 20 }), { wrapper })
    await waitFor(() => expect(result.current.data?.status).toBe('Succeeded'))
    const calls = get.mock.calls.length
    expect(calls).toBe(3)
    await new Promise((r) => setTimeout(r, 120))
    expect(get.mock.calls.length).toBe(calls)
    expect(getOperation).toBeTypeOf('function')
  })
})
