import { useState } from 'react'
import clsx from 'clsx'
import { useDocumentText } from '../api/library'
import type { LibraryDocument } from '../api/types'
import { ErrorState } from './ErrorState'
import { Skeleton } from './Skeleton'
import { VersionBadge } from './VersionTimeline'

/** Canonical text of one version, with a switcher between versions. Rendered inside the Inspector. */
export function DocumentDrawer({ doc }: { doc: LibraryDocument }) {
  const versions = [...doc.versions].sort((a, b) => b.version_num - a.version_num)
  const initial = (versions.find((v) => v.status === 'Active') ?? versions[0]).version_num
  const [n, setN] = useState(initial)
  const text = useDocumentText(doc.document_id, n)
  const current = versions.find((v) => v.version_num === n)

  return (
    <div className="space-y-4">
      <div>
        <p className="font-mono text-xs font-medium">{doc.source_uri}</p>
        <p className="mt-1 text-xs text-muted">The canonical text the pipeline chunked. Evidence offsets point into exactly this text.</p>
      </div>
      <div role="tablist" aria-label="Versions" className="flex flex-wrap gap-1.5">
        {versions.map((v) => (
          <button
            key={v.version_num}
            type="button"
            role="tab"
            aria-selected={v.version_num === n}
            onClick={() => setN(v.version_num)}
            className={clsx(
              'inline-flex items-center gap-1.5 rounded-lg border px-2 py-1 text-xs transition',
              v.version_num === n ? 'border-accent bg-accent/10' : 'border-border hover:bg-surface-2',
            )}
          >
            <VersionBadge version={v} />
            <span className="text-muted">{v.status}</span>
          </button>
        ))}
      </div>
      {current && current.status !== 'Active' && (
        <p className="text-xs text-muted">This version is {current.status.toLowerCase()}; new answers use the active version.</p>
      )}
      {text.isPending ? (
        <div className="space-y-2" aria-busy="true">
          {[0, 1, 2, 3, 4, 5].map((i) => (
            <Skeleton key={i} className="h-4 w-full" />
          ))}
        </div>
      ) : text.isError ? (
        <ErrorState title="The text could not be loaded" message={text.error.message} />
      ) : (
        <pre
          tabIndex={0}
          aria-label={`Text of ${doc.source_uri} version ${n}`}
          className="max-h-[calc(100vh-17rem)] overflow-auto whitespace-pre-wrap break-words rounded-lg border border-border bg-surface-2 p-3 font-mono text-xs leading-5"
        >
          {text.data.text}
        </pre>
      )}
    </div>
  )
}
