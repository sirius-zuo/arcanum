import { useEffect, useMemo, useRef, useState } from 'react'
import { ApiError } from '../api/client'
import { useSourceText } from '../api/evidence'
import { byteRangeToIndexRange } from '../lib/offsets'
import { segmentText } from '../lib/highlight'

interface SourceTextProps {
  text: string
  /** UTF-8 byte offsets into `text`, end exclusive. */
  start: number
  end: number
  /** Lines of context kept around the highlight; the whole text when `whole` is set. */
  contextLines?: number
  whole?: boolean
  label: string
}

function lineStartBefore(text: string, index: number, lines: number): number {
  let i = index
  for (let n = 0; n <= lines; n++) {
    if (i <= 0) return 0
    const nl = text.lastIndexOf('\n', i - 1)
    if (nl < 0) return 0
    i = nl
  }
  return i + 1
}

function lineEndAfter(text: string, index: number, lines: number): number {
  let i = index
  for (let n = 0; n <= lines; n++) {
    const nl = text.indexOf('\n', i)
    if (nl < 0) return text.length
    i = nl + 1
  }
  return i - 1
}

/** The reading block: canonical text with the byte range highlighted, scrolled into view. */
export function SourceText({ text, start, end, contextLines = 3, whole = false, label }: SourceTextProps) {
  const box = useRef<HTMLDivElement>(null)
  const mark = useRef<HTMLElement>(null)
  const [a, b] = useMemo(() => byteRangeToIndexRange(text, start, end), [text, start, end])
  const [from, to] = whole ? [0, text.length] : [lineStartBefore(text, a, contextLines), lineEndAfter(text, b, contextLines)]
  const segments = segmentText(text.slice(from, to), [{ start: a - from, end: b - from, kind: 'evidence' }])

  useEffect(() => {
    const el = box.current
    const m = mark.current
    if (!el || !m) return
    // Scroll the reading block itself; scrollIntoView would also move the page.
    el.scrollTop = Math.max(0, m.offsetTop - el.clientHeight / 2 + m.offsetHeight / 2)
  }, [a, b, from, to])

  return (
    <section aria-label={label} className="rounded-lg border border-border bg-surface-2/60">
      <div ref={box} className="relative max-h-64 overflow-auto p-3 font-mono text-[12.5px] leading-6">
        {from > 0 && <span className="block select-none text-muted">...</span>}
        <p className="whitespace-pre-wrap break-words">
          {segments.map((s, i) =>
            s.kinds.length > 0 ? (
              <mark key={i} ref={mark} data-testid="source-highlight" className="rounded-sm bg-accent/25 px-0.5 text-text underline decoration-accent decoration-2 underline-offset-4">
                {s.text}
              </mark>
            ) : (
              <span key={i}>{s.text}</span>
            ),
          )}
        </p>
        {to < text.length && <span className="block select-none text-muted">...</span>}
      </div>
      <p className="border-t border-border px-3 py-1.5 font-mono text-[11px] text-muted">
        bytes {start}-{end}
        {end <= start && ' (empty range)'}
      </p>
    </section>
  )
}

interface SourceViewerProps {
  documentId: string
  version: number
  start: number
  end: number
  contextLines?: number
}

/** Loads the canonical text of a document version and highlights an evidence byte range in it. */
export function SourceViewer({ documentId, version, start, end, contextLines }: SourceViewerProps) {
  const q = useSourceText(documentId, version)
  const [whole, setWhole] = useState(false)

  if (q.isPending) {
    return <div role="status" aria-label="Loading source text" className="skeleton relative h-20 overflow-hidden rounded-lg bg-surface-2" />
  }
  if (q.isError) {
    const gone = q.error instanceof ApiError && q.error.status === 404
    return (
      <p role="alert" className="rounded-lg border border-v-partial/40 bg-v-partial/10 px-3 py-2 text-xs text-vink-partial">
        {gone ? 'The source text for this version is no longer available.' : `Could not load the source text: ${q.error.message}`}
      </p>
    )
  }
  return (
    <div>
      <SourceText text={q.data.text} start={start} end={end} contextLines={contextLines} whole={whole} label={`Source text, ${q.data.source_uri} v${version}`} />
      <button type="button" onClick={() => setWhole((w) => !w)} className="mt-1 text-[11px] text-muted underline-offset-2 hover:text-text hover:underline">
        {whole ? 'Show less context' : 'Show whole document'}
      </button>
    </div>
  )
}
