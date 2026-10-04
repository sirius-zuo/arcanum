import { useState } from 'react'
import { useMutation, useQueryClient } from '@tanstack/react-query'
import { BookOpen, FileText, Loader2, Sparkles, Trash2 } from 'lucide-react'
import { deleteSource, useTerminalCount } from '../api/ingest'
import { useApplyUpdate, useHealth, useLibrary, useLoadSamples } from '../api/library'
import type { LibraryDocument } from '../api/types'
import { Card } from '../components/Card'
import { DocumentDrawer } from '../components/DocumentDrawer'
import { EmptyState } from '../components/EmptyState'
import { ErrorState } from '../components/ErrorState'
import { HowItWorks } from '../components/HowItWorks'
import { useInspector } from '../components/Inspector'
import { LoadCorpusButton } from '../components/LoadCorpusButton'
import { OperationRail } from '../components/OperationRail'
import { PageHeader } from '../components/PageHeader'
import { Skeleton } from '../components/Skeleton'
import { UploadZone } from '../components/UploadZone'
import { VersionBadge, VersionTimeline } from '../components/VersionTimeline'
import { ROUTES, routeMeta } from '../routes'
import { useBootstrap } from '../state/bootstrap'
import { useOperations } from '../state/operations'

const meta = routeMeta('/library')
const step = String(ROUTES.indexOf(meta) + 1).padStart(2, '0')

const HOW_ARCANUM = [
  'POST /api/v1/ingestion-operations (multipart: metadata + payload)',
  'GET /api/v1/ingestion-operations/:id (polled until a terminal report)',
  'GET /ws/events (live nudges; polling stays the source of truth)',
  'DELETE /api/v1/collections/halcyon/sources?source_uri=...',
]
const HOW_DEMO = [
  'GET /demo/library',
  'GET /demo/documents/:id/versions/:n/text',
  'POST /demo/samples/load',
  'POST /demo/samples/apply-update',
]

const POLICY = 'security-policy.md'

function DocRow({ doc }: { doc: LibraryDocument }) {
  const { client } = useBootstrap()
  const queryClient = useQueryClient()
  const inspector = useInspector()
  const [confirming, setConfirming] = useState(false)
  const remove = useMutation({
    mutationFn: () => deleteSource(client, doc.source_uri),
    onSuccess: () => queryClient.invalidateQueries({ queryKey: ['demo', 'library'] }),
    onSettled: () => setConfirming(false),
  })
  const active = doc.versions.find((v) => v.status === 'Active')

  return (
    <tr className="border-t border-border align-top">
      <td className="px-4 py-3">
        <div className="flex items-center gap-2 font-mono text-xs font-medium">
          <FileText className="h-3.5 w-3.5 shrink-0 text-muted" aria-hidden="true" />
          {doc.source_uri}
        </div>
      </td>
      <td className="px-4 py-3">{active ? <VersionBadge version={active} /> : <span className="text-xs text-muted">None</span>}</td>
      <td className="px-4 py-3 font-mono text-xs tabular-nums">{doc.chunks}</td>
      <td className="px-4 py-3">
        <VersionTimeline versions={doc.versions} />
      </td>
      <td className="px-4 py-3">
        <div className="flex flex-wrap items-center justify-end gap-2">
          <button
            type="button"
            onClick={() => inspector.open(<DocumentDrawer doc={doc} />, doc.source_uri)}
            className="h-8 rounded-lg border border-border bg-surface px-2.5 text-xs font-medium transition hover:shadow-soft"
          >
            View text
          </button>
          {confirming ? (
            <span className="inline-flex items-center gap-1.5">
              <button
                type="button"
                onClick={() => remove.mutate()}
                disabled={remove.isPending}
                className="inline-flex h-8 items-center gap-1 rounded-lg bg-v-unsupported px-2.5 text-xs font-medium text-bg transition disabled:opacity-60"
              >
                {remove.isPending && <Loader2 className="h-3 w-3 animate-spin" aria-hidden="true" />}
                Confirm delete
              </button>
              <button type="button" onClick={() => setConfirming(false)} className="h-8 rounded-lg px-2 text-xs text-muted hover:text-text">
                Cancel
              </button>
            </span>
          ) : (
            <button
              type="button"
              onClick={() => setConfirming(true)}
              aria-label={`Delete source ${doc.source_uri}`}
              className="inline-flex h-8 items-center gap-1 rounded-lg border border-border bg-surface px-2.5 text-xs font-medium text-v-unsupported transition hover:shadow-soft"
            >
              <Trash2 className="h-3.5 w-3.5" aria-hidden="true" />
              Delete source
            </button>
          )}
        </div>
        {remove.isError && (
          <p role="alert" className="mt-1.5 text-right text-xs text-v-unsupported">
            {remove.error.message}
          </p>
        )}
      </td>
    </tr>
  )
}

export default function LibraryPage() {
  const health = useHealth()
  const library = useLibrary()
  const load = useLoadSamples()
  const update = useApplyUpdate()
  const { ops } = useOperations()
  const progress = useTerminalCount(ops.map((o) => o.operation_id))
  const docs = library.data?.documents ?? []
  const policy = docs.find((d) => d.source_uri === POLICY)
  const canUpdate = policy !== undefined && policy.versions.length === 1

  const loadButton = <LoadCorpusButton health={health.data} loading={load.isPending} trackedCount={ops.length} doneCount={progress.done} onLoad={() => load.mutate()} />

  return (
    <>
      <PageHeader
        eyebrow={`${step} / ${meta.label}`}
        title={meta.label}
        description={meta.blurb}
        actions={<HowItWorks arcanum={HOW_ARCANUM} demo={HOW_DEMO} />}
      />

      <div className="mb-8 grid gap-6 lg:grid-cols-2">
        <Card className="p-5">
          <UploadZone />
        </Card>
        <Card className="p-5">
          <OperationRail ops={ops} />
        </Card>
      </div>

      <div className="mb-4 flex flex-wrap items-start justify-between gap-3">
        <h2 className="text-base font-semibold">Documents</h2>
        <div className="flex flex-wrap items-start gap-3">
          {canUpdate && (
            <button
              type="button"
              onClick={() => update.mutate()}
              disabled={update.isPending}
              className="inline-flex h-10 items-center gap-2 rounded-lg border border-border bg-surface px-4 text-sm font-medium transition hover:shadow-soft disabled:opacity-50"
            >
              {update.isPending ? <Loader2 className="h-4 w-4 animate-spin" aria-hidden="true" /> : <Sparkles className="h-4 w-4" aria-hidden="true" />}
              Apply policy update
            </button>
          )}
          {docs.length > 0 && loadButton}
        </div>
      </div>
      {load.isError && <p role="alert" className="mb-3 text-sm text-v-unsupported">{load.error.message}</p>}
      {update.isError && <p role="alert" className="mb-3 text-sm text-v-unsupported">{update.error.message}</p>}
      {canUpdate && (
        <p className="mb-3 text-xs text-muted">
          {POLICY} has one version. Applying the update re-ingests it, which supersedes v1 with v2.
        </p>
      )}

      {library.isError ? (
        <ErrorState title="The library could not be read" message={library.error.message} fix="cargo run   # in examples/atlas-knowledge-hub" />
      ) : library.isPending ? (
        <Card className="space-y-4 p-5" aria-busy="true">
          {[0, 1, 2].map((i) => (
            <Skeleton key={i} className="h-8 w-full" />
          ))}
        </Card>
      ) : docs.length === 0 ? (
        <EmptyState
          icon={BookOpen}
          title="The library is empty"
          description="Load the ten sample documents, or drop your own .md or .txt files above. Each one goes through the full ingestion pipeline."
          action={loadButton}
        />
      ) : (
        <Card className="overflow-x-auto">
          <table className="w-full min-w-[760px] text-left text-sm">
            <thead>
              <tr className="text-xs text-muted">
                <th scope="col" className="px-4 py-2.5 font-medium">Document</th>
                <th scope="col" className="px-4 py-2.5 font-medium">Active</th>
                <th scope="col" className="px-4 py-2.5 font-medium">Chunks</th>
                <th scope="col" className="px-4 py-2.5 font-medium">Versions</th>
                <th scope="col" className="px-4 py-2.5 text-right font-medium">Actions</th>
              </tr>
            </thead>
            <tbody>
              {docs.map((d) => (
                <DocRow key={d.document_id} doc={d} />
              ))}
            </tbody>
          </table>
        </Card>
      )}
    </>
  )
}
