import { useMutation, useQueryClient } from '@tanstack/react-query'
import { Archive, Gauge, KeyRound, RefreshCw } from 'lucide-react'
import { useState } from 'react'
import { rotateKeys, runGc, useAudit, useMetrics, useServerStatus } from '../api/admin'
import type { GcResult } from '../api/admin'
import { AuditTable } from '../components/AuditTable'
import { Card } from '../components/Card'
import { Chip } from '../components/Chip'
import { ConfirmDialog } from '../components/ConfirmDialog'
import { EmptyState } from '../components/EmptyState'
import { ErrorState } from '../components/ErrorState'
import { EventFeed } from '../components/EventFeed'
import { MetricCards } from '../components/MetricCards'
import { PageHeader } from '../components/PageHeader'
import { ReadinessPanel } from '../components/ReadinessPanel'
import { Skeleton } from '../components/Skeleton'
import { ROUTES, routeMeta } from '../routes'
import { useBootstrap } from '../state/bootstrap'

const meta = routeMeta('/admin')
const step = String(ROUTES.indexOf(meta) + 1).padStart(2, '0')

const button =
  'inline-flex h-9 items-center gap-2 rounded-lg border border-border bg-surface px-3 text-sm font-medium transition hover:bg-surface-2 disabled:opacity-60'

function Section({ title, hint, children }: { title: string; hint?: string; children: React.ReactNode }) {
  return (
    <Card className="p-5">
      <h2 className="text-base font-semibold">{title}</h2>
      {hint && <p className="mb-4 mt-1 text-sm text-muted">{hint}</p>}
      {!hint && <div className="mb-3" />}
      {children}
    </Card>
  )
}

function GcCard() {
  const { client } = useBootstrap()
  const gc = useMutation<GcResult>({ mutationFn: () => runGc(client) })
  const result = gc.data
  return (
    <Section title="Retention GC" hint="Removes superseded and deleted versions past retention, with their snapshots and chunks.">
      <button type="button" className={button} disabled={gc.isPending} onClick={() => gc.mutate()}>
        <Archive className="h-4 w-4" aria-hidden="true" />
        {gc.isPending ? 'Running' : 'Run GC now'}
      </button>
      <div className="mt-4" aria-live="polite">
        {gc.isError && <ErrorState title="GC failed" message={gc.error.message} />}
        {result?.kind === 'unavailable' && (
          <EmptyState
            icon={Archive}
            title="Retention GC is not available"
            description="Retention GC needs Postgres bookkeeping; it is disabled in this demo. See the Evidence section of README.md for how versions and retention work."
          />
        )}
        {result?.kind === 'ok' && (
          <dl className="grid grid-cols-3 gap-3 text-sm">
            <div>
              <dt className="text-xs text-muted">Versions deleted</dt>
              <dd className="font-mono">{result.report.versions_deleted}</dd>
            </div>
            <div>
              <dt className="text-xs text-muted">Snapshots removed</dt>
              <dd className="font-mono">{result.report.snapshots_removed}</dd>
            </div>
            <div>
              <dt className="text-xs text-muted">Chunks removed</dt>
              <dd className="font-mono">{result.report.chunks_removed}</dd>
              {result.report.errors.length > 0 && <Chip tone="unsupported">{result.report.errors.length} errors</Chip>}
            </div>
          </dl>
        )}
      </div>
    </Section>
  )
}

function RotateCard() {
  const { client } = useBootstrap()
  const queryClient = useQueryClient()
  const [confirming, setConfirming] = useState(false)
  const rotate = useMutation({
    mutationFn: () => rotateKeys(client),
    onSettled: () => {
      setConfirming(false)
      void queryClient.invalidateQueries({ queryKey: ['admin', 'audit'] })
    },
  })
  return (
    <Section title="Rotate keys" hint="Asks the engine to rotate its signing keys and records the action in the audit log.">
      <button type="button" className={button} onClick={() => setConfirming(true)}>
        <KeyRound className="h-4 w-4" aria-hidden="true" />
        Rotate keys
      </button>
      <div className="mt-4" aria-live="polite">
        {rotate.isSuccess && <Chip tone="supported">Keys {rotate.data.status}</Chip>}
        {rotate.isError && <ErrorState title="Rotation failed" message={rotate.error.message} />}
      </div>
      {confirming && (
        <ConfirmDialog
          title="Rotate signing keys?"
          confirmLabel="Rotate keys"
          busy={rotate.isPending}
          onConfirm={() => rotate.mutate()}
          onCancel={() => setConfirming(false)}
        >
          This is an admin operation. It is written to the audit log and reloads the secret store if one is configured.
        </ConfirmDialog>
      )}
    </Section>
  )
}

export default function AdminPage() {
  const { data } = useBootstrap()
  const audit = useAudit()
  const metrics = useMetrics()
  const status = useServerStatus()
  const queryClient = useQueryClient()
  const snapshot = metrics.data?.kind === 'ok' ? metrics.data.data : null

  return (
    <>
      <PageHeader
        eyebrow={`${step} / ${meta.label}`}
        title={meta.label}
        description={meta.blurb}
        actions={
          <button type="button" className={button} onClick={() => void queryClient.invalidateQueries({ queryKey: ['admin'] })}>
            <RefreshCw className="h-4 w-4" aria-hidden="true" />
            Refresh audit
          </button>
        }
      />
      <div className="space-y-6">
        <div className="grid gap-6 lg:grid-cols-2">
          <Section title="Readiness">
            <ReadinessPanel status={status.data} metrics={snapshot} />
          </Section>
          <Section title="Live events">{data ? <EventFeed apiKey={data.api_key} /> : <Skeleton className="h-24" />}</Section>
        </div>

        <Section title="Metrics">
          {metrics.isPending && <Skeleton className="h-32" />}
          {metrics.isError && <ErrorState title="Metrics request failed" message={metrics.error.message} />}
          {metrics.data?.kind === 'unavailable' && (
            <EmptyState
              icon={Gauge}
              title="Metrics are not reporting right now"
              description={`Metrics are not reporting right now (${metrics.data.message}). Request counts and latency appear here once the engine serves them.`}
            />
          )}
          {snapshot && <MetricCards snapshot={snapshot} />}
        </Section>

        <Section title="Audit log" hint="The 100 most recent operations, newest first.">
          {audit.isPending && <Skeleton className="h-32" />}
          {audit.isError && <ErrorState title="Audit log unavailable" message={audit.error.message} />}
          {audit.data && <AuditTable records={audit.data} />}
        </Section>

        <div className="grid gap-6 lg:grid-cols-2">
          <RotateCard />
          <GcCard />
        </div>
      </div>
    </>
  )
}
