import { FileSearch, Layers, FileText } from 'lucide-react'
import { Link } from 'react-router-dom'
import { segmentText, termRanges } from '../lib/highlight'
import type { RetrievedChunk } from '../api/types'
import { Card } from './Card'
import { Chip } from './Chip'
import { StrategyBadge } from './StrategyBadge'

interface ResultCardProps {
  chunk: RetrievedChunk
  query: string
}

export const RRF_HINT =
  'Reciprocal rank fusion: each strategy ranks its candidates and the ranks are combined as 1/(k+rank). It is a rank-based score (typically 0.016 to 0.05), not a similarity or a probability.'

/** Text with the query's terms wrapped in <mark>. */
export function Highlighted({ text, query }: { text: string; query: string }) {
  // Whole-word matches only: "is" should not light up inside "disk". Checked per code point so astral letters count.
  const isWord = (ch: string | undefined) => ch !== undefined && /[\p{L}\p{N}_]/u.test(ch)
  const before = (i: number) => {
    if (i <= 0) return undefined
    const lo = text.charCodeAt(i - 1)
    return lo >= 0xdc00 && lo <= 0xdfff && i >= 2 ? text.slice(i - 2, i) : text[i - 1]
  }
  const after = (i: number) => (i < text.length ? String.fromCodePoint(text.codePointAt(i) as number) : undefined)
  const terms = query
    .split(/\s+/)
    .map((t) => t.replace(/^[^\p{L}\p{N}]+|[^\p{L}\p{N}]+$/gu, ''))
    .join(' ')
  const ranges = termRanges(text, terms).filter((r) => !isWord(before(r.start)) && !isWord(after(r.end)))
  const segments = segmentText(text, ranges)
  return (
    <>
      {segments.map((s, i) =>
        s.kinds.length > 0 ? (
          <mark key={i} className="rounded-sm bg-accent/20 px-0.5 text-text">
            {s.text}
          </mark>
        ) : (
          <span key={i}>{s.text}</span>
        ),
      )}
    </>
  )
}

export function ResultCard({ chunk, query }: ResultCardProps) {
  const { chunk: c } = chunk.indexed_chunk
  const p = c.provenance
  const summary = typeof chunk.kind === 'object' ? chunk.kind.Summary : null
  return (
    <Card className="p-4">
      <div className="mb-3 flex flex-wrap items-center gap-2">
        <StrategyBadge strategy={chunk.strategy} />
        {summary ? (
          <Chip icon={<Layers className="h-3 w-3" aria-hidden="true" />}>
            Summary, level {summary.level}, covers {summary.covers.length} {summary.covers.length === 1 ? 'chunk' : 'chunks'}
          </Chip>
        ) : (
          <Chip icon={<FileText className="h-3 w-3" aria-hidden="true" />}>Source</Chip>
        )}
        <span className="ml-auto text-xs text-muted" title={RRF_HINT}>
          fused score <span className="font-mono text-text">{chunk.score.toFixed(4)}</span>
        </span>
      </div>
      <p className="whitespace-pre-wrap text-sm leading-relaxed">
        <Highlighted text={c.text} query={query} />
      </p>
      <div className="mt-3 flex flex-wrap items-center justify-between gap-2 border-t border-border pt-3">
        <p className="min-w-0 break-all font-mono text-[11px] text-muted">
          {p.source_uri} · v{p.document_version}
          {p.page !== null && ` · page ${p.page}`}
          {p.section && ` · ${p.section}`}
        </p>
        <Link
          to={`/evidence?chunk=${encodeURIComponent(c.id)}`}
          className="inline-flex h-7 items-center gap-1.5 rounded-lg border border-border px-2 text-xs font-medium text-accent transition hover:shadow-soft"
        >
          <FileSearch className="h-3.5 w-3.5" aria-hidden="true" />
          Open evidence
        </Link>
      </div>
    </Card>
  )
}
