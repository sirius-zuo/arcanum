import { useEffect, useRef } from 'react'
import { useQueries, useQuery, useQueryClient } from '@tanstack/react-query'
import { ApiError } from './client'
import type { Client } from './client'
import { idempotencyKey } from '../lib/idempotency'
import { useBootstrap } from '../state/bootstrap'

export const COLLECTION = 'halcyon'
export const POLL_MS = 1_500

export type OperationStatus = 'Accepted' | 'Running' | 'Succeeded' | 'Failed'

export interface OperationError {
  code: string
  message: string
  retryable: boolean
}

export interface TerminalReport {
  operation_id: string
  status: OperationStatus
  outcome: 'Ingested' | 'Unchanged' | null
  content_uri: string | null
  error: OperationError | null
  partial_output_disposition: string
}

export interface OperationDoc {
  operation_id: string
  submission: unknown
  status: OperationStatus
  accepted_at: string
  started_at: string | null
  terminal_report: TerminalReport | null
}

export interface SubmitResult {
  operation_id: string
  replay: boolean
}

export function isTerminal(doc: OperationDoc | undefined): boolean {
  return doc?.status === 'Succeeded' || doc?.status === 'Failed'
}

const ALLOWED = ['.md', '.txt']

/** Null when the file can be uploaded, otherwise the reason it cannot. */
export function checkUploadFile(file: File): string | null {
  const name = file.name.toLowerCase()
  if (ALLOWED.some((ext) => name.endsWith(ext))) return null
  return `${file.name} was not uploaded. This demo ingests plain text only, so use .md or .txt files: the server's pass-through preprocessor cannot extract text from other formats.`
}

function readBytes(file: Blob): Promise<ArrayBuffer> {
  if (typeof file.arrayBuffer === 'function') return file.arrayBuffer()
  return new Promise((resolve, reject) => {
    const reader = new FileReader()
    reader.onload = () => resolve(reader.result as ArrayBuffer)
    reader.onerror = () => reject(reader.error ?? new Error('Could not read the file'))
    reader.readAsArrayBuffer(file)
  })
}

const SUBMIT_ERRORS: Record<number, string> = {
  409: 'This file name was already submitted with different content. Re-uploading changed content under the same key is refused; reload the page and try again.',
  413: 'The file is too large for the server to accept.',
  503: 'The ingestion queue is full. Wait for the running operations to finish and try again.',
}

export async function submitUpload(client: Client, file: File): Promise<SubmitResult> {
  const bytes = await readBytes(file)
  const mime = file.type || (file.name.toLowerCase().endsWith('.md') ? 'text/markdown' : 'text/plain')
  const metadata = {
    idempotency_key: await idempotencyKey(COLLECTION, file.name, bytes),
    logical_source_uri: file.name,
    mime_hint: mime,
    collection_id: COLLECTION,
    pipeline_configuration: { template: 'full' },
  }
  const form = new FormData()
  form.append('metadata', new Blob([JSON.stringify(metadata)], { type: 'application/json' }))
  form.append('payload', new Blob([bytes], { type: mime }), file.name)

  // No Content-Type: the browser has to add the multipart boundary itself.
  const res = await client.raw('/api/v1/ingestion-operations', { method: 'POST', body: form })
  if (!res.ok) {
    const text = await res.text().catch(() => '')
    const mapped = SUBMIT_ERRORS[res.status]
    throw new ApiError(res.status, mapped ?? (text.slice(0, 200) || `${res.status} ${res.statusText}`.trim()), text || null)
  }
  const body = (await res.json()) as { operation_id: string }
  return { operation_id: body.operation_id, replay: res.status === 200 }
}

export function getOperation(client: Client, id: string): Promise<OperationDoc> {
  return client.get<OperationDoc>(`/api/v1/ingestion-operations/${encodeURIComponent(id)}`)
}

export function deleteSource(client: Client, sourceUri: string): Promise<void> {
  return client.del(`/api/v1/collections/${COLLECTION}/sources?source_uri=${encodeURIComponent(sourceUri)}`)
}

export const operationKey = (id: string) => ['operation', id] as const

function operationOptions(client: Client, id: string, pollMs: number, enabled: boolean) {
  return {
    enabled,
    queryKey: operationKey(id),
    queryFn: () => getOperation(client, id),
    // The terminal report is the truth: poll until it exists, then stop.
    refetchInterval: (query: { state: { data: OperationDoc | undefined } }) => (isTerminal(query.state.data) ? false : pollMs),
    retry: false,
  }
}

export interface OperationStatusOptions {
  pollMs?: number
  /** Hook point for tour signals (wired in a later task). Called once when the operation moves from non-terminal to terminal during this session; never for operations already terminal at first fetch. */
  onTerminal?: (doc: OperationDoc) => void
}

/** Polls one operation until it succeeds or fails. Socket events elsewhere only nudge a refetch. */
export function useOperationStatus(id: string, opts: OperationStatusOptions = {}) {
  const { client, data: boot } = useBootstrap()
  const queryClient = useQueryClient()
  const query = useQuery(operationOptions(client, id, opts.pollMs ?? POLL_MS, boot !== null))
  const sawLive = useRef(false)
  const fired = useRef(false)
  const onTerminal = useRef(opts.onTerminal)
  onTerminal.current = opts.onTerminal

  useEffect(() => {
    if (!query.data) return
    if (!isTerminal(query.data)) {
      sawLive.current = true
      return
    }
    // Operations already terminal at first fetch (history after a page load) are not events.
    if (!sawLive.current || fired.current) return
    fired.current = true
    void queryClient.invalidateQueries({ queryKey: ['demo', 'library'] })
    onTerminal.current?.(query.data)
  }, [query.data, queryClient])

  return query
}

/** Statuses for many operations at once, sharing the per-operation cache. */
export function useTerminalCount(ids: string[]): { done: number; total: number } {
  const { client, data: boot } = useBootstrap()
  const queryClient = useQueryClient()
  const results = useQueries({ queries: ids.map((id) => operationOptions(client, id, POLL_MS, boot !== null)) })
  const done = results.filter((r) => isTerminal(r.data)).length
  const live = useRef(new Set<string>())
  const finished = results.map((r, i) => ({ id: ids[i], terminal: isTerminal(r.data), has: r.data !== undefined }))
  const signature = finished.map((f) => `${f.id}:${f.has ? (f.terminal ? 1 : 0) : '-'}`).join(',')
  useEffect(() => {
    let transitioned = false
    for (const f of finished) {
      if (!f.has) continue
      if (!f.terminal) live.current.add(f.id)
      else if (live.current.delete(f.id)) transitioned = true
    }
    if (transitioned) void queryClient.invalidateQueries({ queryKey: ['demo', 'library'] })
  }, [signature, queryClient])
  return { done, total: ids.length }
}
