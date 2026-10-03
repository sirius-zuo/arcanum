import clsx from 'clsx'
import { AlertTriangle, FileText } from 'lucide-react'
import { useId } from 'react'
import type { ContextPassage } from '../api/types'

interface CitationChipProps {
  refId: string
  /** The passage this id resolves to in the context; absent for unknown refs. */
  passage?: ContextPassage
  onSelect: (refId: string) => void
}

/** An inline `[P1]` marker: click jumps to the passage, hover or focus shows where it points. */
export function CitationChip({ refId, passage, onSelect }: CitationChipProps) {
  const tipId = useId()
  const unknown = !passage
  const background = refId.startsWith('S')
  const label = passage ? `${refId}, ${passage.source_uri}, jump to passage` : `${refId}, unknown reference`

  return (
    <span className="group relative mx-0.5 inline-block align-baseline">
      <button
        type="button"
        data-state={unknown ? 'unknown' : 'known'}
        aria-label={label}
        aria-describedby={tipId}
        onClick={() => {
          if (passage) onSelect(refId)
        }}
        className={clsx(
          'inline-flex items-center gap-0.5 rounded-md px-1.5 font-mono text-[11px] font-medium leading-5 transition',
          unknown
            ? 'cursor-help border border-dashed border-v-partial/60 bg-v-partial/10 text-v-partial'
            : 'cursor-pointer bg-accent/10 text-accent hover:bg-accent/20',
        )}
      >
        {unknown && <AlertTriangle className="h-3 w-3" aria-hidden="true" />}
        {refId}
      </button>
      <span
        id={tipId}
        role="tooltip"
        className="pointer-events-none invisible absolute bottom-full left-0 z-30 mb-1.5 w-64 rounded-lg border border-border bg-surface p-2.5 text-left font-sans text-xs font-normal normal-case leading-snug text-text opacity-0 shadow-lift transition group-focus-within:visible group-focus-within:opacity-100 group-hover:visible group-hover:opacity-100"
      >
        {passage ? (
          <>
            <span className="flex items-center gap-1.5 font-medium">
              <FileText className="h-3.5 w-3.5 shrink-0 text-accent" aria-hidden="true" />
              <span className="break-all">{passage.source_uri}</span>
            </span>
            <span className="mt-1 block font-mono text-[11px] text-muted">
              v{passage.version_num} · bytes {passage.offset_start}-{passage.offset_end}
            </span>
          </>
        ) : (
          <span className="text-v-partial">
            {background
              ? `${refId} is a background summary. Generate cannot resolve background ids to a source, so it is listed as an unknown reference.`
              : `No passage ${refId} exists in this answer's context. The model cited something it was not given.`}
          </span>
        )}
      </span>
    </span>
  )
}
