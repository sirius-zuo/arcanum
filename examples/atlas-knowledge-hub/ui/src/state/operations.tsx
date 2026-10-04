import { createContext, useCallback, useContext, useMemo, useState } from 'react'
import type { ReactNode } from 'react'
import { safeGet, safeSet } from '../lib/storage'
import type { OperationRef } from '../api/types'

const KEY = 'atlas.operations'

export interface TrackedOp {
  source_uri: string
  operation_id: string
  addedAt: number
  /** True when the server answered 200: the upload replayed an earlier identical submission. */
  replay?: boolean
}

export type TrackInput = OperationRef & { replay?: boolean }

interface OperationsValue {
  ops: TrackedOp[]
  track: (ids: TrackInput[]) => void
}

const Ctx = createContext<OperationsValue | null>(null)

function isTracked(v: unknown): v is TrackedOp {
  if (typeof v !== 'object' || v === null) return false
  const o = v as Record<string, unknown>
  return typeof o.source_uri === 'string' && typeof o.operation_id === 'string' && typeof o.addedAt === 'number'
}

function load(): TrackedOp[] {
  const raw = safeGet(KEY)
  if (!raw) return []
  try {
    const parsed: unknown = JSON.parse(raw)
    return Array.isArray(parsed) ? parsed.filter(isTracked) : []
  } catch {
    return []
  }
}

export function OperationsProvider({ children }: { children: ReactNode }) {
  const [ops, setOps] = useState<TrackedOp[]>(load)

  const track = useCallback((ids: TrackInput[]) => {
    setOps((current) => {
      const now = Date.now()
      const next = [...current]
      let changed = false
      for (const id of ids) {
        const at = next.findIndex((o) => o.operation_id === id.operation_id)
        if (at === -1) {
          next.push({ source_uri: id.source_uri, operation_id: id.operation_id, addedAt: now, ...(id.replay ? { replay: true } : {}) })
          changed = true
        } else if (id.replay && !next[at].replay) {
          // A replay returns the original operation id: mark the existing entry, keep addedAt.
          next[at] = { ...next[at], replay: true }
          changed = true
        }
      }
      if (!changed) return current
      safeSet(KEY, JSON.stringify(next))
      return next
    })
  }, [])

  const value = useMemo(() => ({ ops, track }), [ops, track])
  return <Ctx.Provider value={value}>{children}</Ctx.Provider>
}

export function useOperations(): OperationsValue {
  const v = useContext(Ctx)
  if (!v) throw new Error('useOperations must be used inside OperationsProvider')
  return v
}
