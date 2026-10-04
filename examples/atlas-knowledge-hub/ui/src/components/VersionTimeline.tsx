import clsx from 'clsx'
import { ArrowRight } from 'lucide-react'
import type { LibraryVersion } from '../api/types'
import { Chip } from './Chip'

export function formatWhen(iso: string): string {
  const d = new Date(iso)
  if (Number.isNaN(d.getTime())) return iso
  return d.toLocaleString(undefined, { month: 'short', day: 'numeric', hour: '2-digit', minute: '2-digit' })
}

export function shortHash(hash: string): string {
  return hash.replace(/^sha256:/, '').slice(0, 8)
}

/** Version badge: Active is green and Superseded is muted and struck through; the word is always shown. */
export function VersionBadge({ version }: { version: LibraryVersion }) {
  const active = version.status === 'Active'
  return (
    <Chip tone={active ? 'supported' : 'noclaim'} mono className={clsx(version.status === 'Superseded' && 'line-through')}>
      v{version.version_num}
      <span className="sr-only"> {version.status}</span>
    </Chip>
  )
}

export function VersionTimeline({ versions }: { versions: LibraryVersion[] }) {
  const sorted = [...versions].sort((a, b) => a.version_num - b.version_num)
  return (
    <ol className="flex flex-wrap items-center gap-x-1.5 gap-y-2" aria-label="Version history">
      {sorted.map((v, i) => {
        const active = v.status === 'Active'
        return (
          <li key={v.version_num} className="flex items-center gap-1.5">
            {i > 0 && <ArrowRight className="h-3.5 w-3.5 text-muted" aria-hidden="true" />}
            <div
              className={clsx(
                'rounded-lg border px-2 py-1 text-xs',
                active ? 'border-v-supported/40 bg-v-supported/5' : 'border-border text-muted',
              )}
            >
              <div className="flex items-center gap-1.5">
                <span className={clsx('font-mono font-medium', v.status === 'Superseded' && 'line-through')}>v{v.version_num}</span>
                <span className={clsx('font-medium', active ? 'text-v-supported' : 'text-muted')}>{v.status}</span>
              </div>
              <div className="mt-0.5 flex items-center gap-1.5 text-[11px] text-muted">
                <time dateTime={v.ingested_at}>{formatWhen(v.ingested_at)}</time>
                <span className="font-mono" title={v.content_hash}>
                  {shortHash(v.content_hash)}
                </span>
              </div>
            </div>
          </li>
        )
      })}
    </ol>
  )
}
