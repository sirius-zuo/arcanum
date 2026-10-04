import { useQuery } from '@tanstack/react-query'
import { useBootstrap } from '../state/bootstrap'
import { ApiError } from './client'
import type { Client } from './client'
import type { DemoMcp, DemoMetrics } from './types'

export interface AuditRecord {
  entry: { operation: string; user_id: string; collection_id: string; result: string }
  timestamp: string
}

export interface GcReport {
  versions_deleted: number
  snapshots_removed: number
  chunks_removed: number
  errors: string[]
}

export type GcResult = { kind: 'ok'; report: GcReport } | { kind: 'unavailable'; message: string }
export type MetricsResult = { kind: 'ok'; data: DemoMetrics } | { kind: 'unavailable'; message: string }

const AUDIT_MAX = 100

/** Newest first, as the server returns it; capped at 100 records. */
export async function getAudit(client: Client): Promise<AuditRecord[]> {
  const res = await client.get<{ logs: AuditRecord[] }>('/admin/audit')
  return res.logs.slice(0, AUDIT_MAX)
}

export function rotateKeys(client: Client): Promise<{ status: string }> {
  return client.post<{ status: string }>('/admin/rotate-keys')
}

/** A 503 is a designed state (no GC worker in this demo), not a failure. */
export async function runGc(client: Client): Promise<GcResult> {
  try {
    return { kind: 'ok', report: await client.post<GcReport>('/admin/gc') }
  } catch (e) {
    if (e instanceof ApiError && e.status === 503) return { kind: 'unavailable', message: e.message }
    throw e
  }
}

/** A 503 means the engine's /metrics returned nothing; the page explains it instead of failing. */
export async function getMetrics(client: Client): Promise<MetricsResult> {
  try {
    return { kind: 'ok', data: await client.get<DemoMetrics>('/demo/metrics') }
  } catch (e) {
    if (e instanceof ApiError && e.status === 503) return { kind: 'unavailable', message: e.message }
    throw e
  }
}

async function plain<T>(path: string): Promise<T> {
  const res = await fetch(path)
  if (!res.ok) throw new ApiError(res.status, `${path} answered ${res.status}`)
  return (await res.json()) as T
}

export function getReady(): Promise<{ status: string }> {
  return plain('/ready')
}

export function getHealth(): Promise<{ status: string }> {
  return plain('/health')
}

export function getMcp(client: Client): Promise<DemoMcp> {
  return client.get<DemoMcp>('/demo/mcp')
}

export function useAudit() {
  const { data, client } = useBootstrap()
  return useQuery({ queryKey: ['admin', 'audit'], queryFn: () => getAudit(client), enabled: data !== null, retry: false })
}

export function useMetrics() {
  const { data, client } = useBootstrap()
  return useQuery({
    queryKey: ['demo', 'metrics'],
    queryFn: () => getMetrics(client),
    enabled: data !== null,
    // Stop polling once the engine reports nothing; the page offers a manual retry.
    refetchInterval: (query) => (query.state.data?.kind === 'unavailable' ? false : 10_000),
    retry: false,
  })
}

export function useServerStatus() {
  return useQuery({
    queryKey: ['server', 'status'],
    queryFn: async () => {
      const [ready, alive] = await Promise.allSettled([getReady(), getHealth()])
      return { ready: ready.status === 'fulfilled' && ready.value.status === 'ready', alive: alive.status === 'fulfilled' }
    },
    refetchInterval: 10_000,
    retry: false,
  })
}

export function useMcp() {
  const { data, client } = useBootstrap()
  return useQuery({ queryKey: ['demo', 'mcp'], queryFn: () => getMcp(client), enabled: data !== null, retry: false })
}
