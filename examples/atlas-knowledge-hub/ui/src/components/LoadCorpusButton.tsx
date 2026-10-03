import { Loader2, Upload } from 'lucide-react'
import type { DemoHealth } from '../api/types'

interface LoadCorpusButtonProps {
  health: DemoHealth | undefined
  loading: boolean
  /** Number of ingestion operations already submitted from this browser. */
  trackedCount: number
  /** How many of the tracked operations have reached a terminal state. */
  doneCount?: number
  onLoad: () => void
}

/** Why loading is blocked, or null when it can proceed. */
export function loadBlockedReason(health: DemoHealth | undefined): string | null {
  if (!health) return 'Checking the server'
  if (health.ready) return null
  return health.checks.find((c) => !c.ok)?.label ?? 'Atlas is not ready'
}

export function LoadCorpusButton({ health, loading, trackedCount, doneCount, onLoad }: LoadCorpusButtonProps) {
  const reason = loadBlockedReason(health)
  const disabled = reason !== null || loading
  return (
    <div className="flex flex-col items-start gap-2">
      <button
        type="button"
        onClick={onLoad}
        disabled={disabled}
        title={reason ?? undefined}
        className="inline-flex h-10 items-center gap-2 rounded-lg bg-accent px-4 text-sm font-medium text-accent-fg shadow-soft transition hover:shadow-lift disabled:cursor-not-allowed disabled:opacity-50 disabled:shadow-none"
      >
        {loading ? <Loader2 className="h-4 w-4 animate-spin" aria-hidden="true" /> : <Upload className="h-4 w-4" aria-hidden="true" />}
        Load sample corpus
      </button>
      {reason !== null ? (
        <p className="max-w-[15rem] text-xs text-muted">Blocked until this is fixed: {reason}</p>
      ) : trackedCount > 0 ? (
        <p className="max-w-[15rem] text-xs text-muted">
          {doneCount ?? 0}/{trackedCount} ingestion {trackedCount === 1 ? 'operation' : 'operations'} finished. Follow them in the Library.
        </p>
      ) : (
        <p className="max-w-[15rem] text-xs text-muted">Ingests ten fictional company documents through the full pipeline.</p>
      )}
    </div>
  )
}
