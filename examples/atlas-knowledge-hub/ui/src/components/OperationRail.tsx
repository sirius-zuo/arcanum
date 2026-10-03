import { useEffect, useState } from 'react'
import { AlertTriangle, CheckCircle2, Equal, Loader2, Radio, RefreshCw, RotateCcw } from 'lucide-react'
import { isTerminal, useOperationStatus } from '../api/ingest'
import type { OperationDoc } from '../api/ingest'
import { useEventsStatus } from '../state/ingestEvents'
import type { TrackedOp } from '../state/operations'
import { Chip } from './Chip'

function useTick(active: boolean) {
  const [, setTick] = useState(0)
  useEffect(() => {
    if (!active) return
    const t = setInterval(() => setTick((n) => n + 1), 1000)
    return () => clearInterval(t)
  }, [active])
}

export function formatElapsed(ms: number): string {
  const s = Math.max(0, Math.floor(ms / 1000))
  if (s < 60) return `${s}s`
  return `${Math.floor(s / 60)}m ${String(s % 60).padStart(2, '0')}s`
}

interface OperationRowProps {
  sourceUri: string
  doc: OperationDoc | undefined
  replay?: boolean
}

/** One tracked operation. Pure: the caller supplies the polled document. */
export function OperationRow({ sourceUri, doc, replay }: OperationRowProps) {
  const live = doc !== undefined && !isTerminal(doc)
  useTick(live)
  const since = doc ? Date.parse(doc.started_at ?? doc.accepted_at) : NaN
  const elapsed = live && !Number.isNaN(since) ? formatElapsed(Date.now() - since) : null
  const report = doc?.terminal_report ?? null

  return (
    <li className="flex flex-wrap items-center gap-x-2 gap-y-1 py-2.5">
      <span className="min-w-0 max-w-full truncate font-mono text-xs font-medium" title={sourceUri}>
        {sourceUri}
      </span>
      <span className="flex flex-wrap items-center gap-1.5">
        {!doc && <Chip icon={<Loader2 className="h-3 w-3 animate-spin" aria-hidden="true" />}>Checking</Chip>}
        {doc?.status === 'Accepted' && <Chip>Accepted</Chip>}
        {doc?.status === 'Running' && (
          <Chip tone="accent" icon={<Loader2 className="h-3 w-3 animate-spin" aria-hidden="true" />}>
            Running
          </Chip>
        )}
        {doc?.status === 'Succeeded' && (
          <>
            {report?.outcome === 'Unchanged' ? (
              <Chip tone="neutral" icon={<Equal className="h-3 w-3" aria-hidden="true" />}>
                Unchanged
              </Chip>
            ) : (
              <Chip tone="supported" icon={<CheckCircle2 className="h-3 w-3" aria-hidden="true" />}>
                {report?.outcome ?? 'Succeeded'}
              </Chip>
            )}
          </>
        )}
        {doc?.status === 'Failed' && (
          <Chip tone="unsupported" icon={<AlertTriangle className="h-3 w-3" aria-hidden="true" />}>
            Failed
          </Chip>
        )}
        {replay && (
          <Chip tone="accent" icon={<RotateCcw className="h-3 w-3" aria-hidden="true" />}>
            Replay
          </Chip>
        )}
        {elapsed && (
          <span className="font-mono text-[11px] text-muted" aria-label={`Elapsed ${elapsed}`}>
            {elapsed}
          </span>
        )}
      </span>
      {replay && doc?.status === 'Succeeded' && (
        <p className="basis-full text-xs text-muted">The server recognised this exact upload and returned the earlier operation instead of ingesting again.</p>
      )}
      {doc?.status === 'Succeeded' && report?.outcome === 'Unchanged' && !replay && (
        <p className="basis-full text-xs text-muted">The content matches the active version, so no new version was created.</p>
      )}
      {doc?.status === 'Failed' && (
        <div role="alert" className="basis-full text-xs">
          <span className="font-mono font-medium text-v-unsupported">{report?.error?.code ?? 'unknown'}</span>
          <span className="ml-2 text-muted">{report?.error?.message ?? 'The operation failed without a report.'}</span>
          {report?.error && (
            <span className="ml-2 inline-flex items-center gap-1 text-muted">
              <RefreshCw className="h-3 w-3" aria-hidden="true" />
              {report.error.retryable ? 'Retryable: try again' : 'Not retryable'}
            </span>
          )}
        </div>
      )}
    </li>
  )
}

function TrackedRow({ op, onTerminal }: { op: TrackedOp; onTerminal?: (doc: OperationDoc) => void }) {
  const { data } = useOperationStatus(op.operation_id, { onTerminal })
  return <OperationRow sourceUri={op.source_uri} doc={data} replay={op.replay} />
}

const VISIBLE = 8

interface OperationRailProps {
  ops: TrackedOp[]
  /** Hook point for tour signals: called once per operation when it first reads terminal. */
  onTerminal?: (doc: OperationDoc) => void
}

export function OperationRail({ ops, onTerminal }: OperationRailProps) {
  const [all, setAll] = useState(false)
  const events = useEventsStatus()
  const newest = [...ops].reverse()
  const shown = all ? newest : newest.slice(0, VISIBLE)
  return (
    <div>
      <div className="mb-1 flex items-center justify-between gap-2">
        <h2 className="text-sm font-semibold">Ingestion operations</h2>
        <span className="inline-flex items-center gap-1 text-[11px] text-muted" title={events === 'open' ? 'Live events plus polling' : 'Socket unavailable; polling every 1.5 s'}>
          <Radio className="h-3 w-3" aria-hidden="true" />
          {events === 'open' ? 'Live' : 'Polling'}
        </span>
      </div>
      {ops.length === 0 ? (
        <p className="py-3 text-sm text-muted">Nothing submitted from this browser yet. Upload a file or load the sample corpus.</p>
      ) : (
        <ul className="divide-y divide-border">
          {shown.map((op) => (
            <TrackedRow key={op.operation_id} op={op} onTerminal={onTerminal} />
          ))}
        </ul>
      )}
      {newest.length > VISIBLE && (
        <button type="button" onClick={() => setAll((a) => !a)} className="mt-2 text-xs font-medium text-accent hover:underline">
          {all ? 'Show fewer' : `Show ${newest.length - VISIBLE} older`}
        </button>
      )}
    </div>
  )
}
