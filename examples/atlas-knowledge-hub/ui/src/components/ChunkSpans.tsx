import { Fragment } from 'react'
import type { CSSProperties } from 'react'
import type { AnnotatedChunk } from '../api/types'
import { byteToIndex, utf8Length } from '../lib/offsets'
import { Chip } from './Chip'

export interface Located {
  /** UTF-16 indices into the text. */
  start: number
  end: number
}

export interface Span {
  start: number
  end: number
  text: string
  /** Indices of every chunk that covers this span; empty for text no chunk covers. */
  chunks: number[]
  /** Chunks that begin exactly at `start`. */
  starts: number[]
}

/**
 * The inspect route returns chunk text but no offsets, so find each chunk in the source. The server
 * reports overlap as UTF-8 bytes (previous end minus this start), which gives the exact start; when
 * that does not match (gaps, rewritten text) fall back to a forward search and finally to null.
 */
export function locateChunks(text: string, chunks: AnnotatedChunk[]): (Located | null)[] {
  const out: (Located | null)[] = []
  let prev: Located | null = null
  for (const c of chunks) {
    let start = -1
    if (prev) {
      const prevEndByte = utf8Length(text.slice(0, prev.start)) + utf8Length(text.slice(prev.start, prev.end))
      const hint = byteToIndex(text, prevEndByte - c.overlap_chars)
      if (text.startsWith(c.text, hint)) start = hint
      else start = text.indexOf(c.text, prev.start + 1)
    } else {
      start = text.indexOf(c.text)
    }
    if (start < 0 || c.text.length === 0) {
      out.push(null)
      continue
    }
    prev = { start, end: start + c.text.length }
    out.push(prev)
  }
  return out
}

/** Split the whole text at every chunk boundary so the spans concatenate back to exactly `text`. */
export function buildSpans(text: string, located: (Located | null)[]): Span[] {
  const cuts = new Set<number>([0, text.length])
  for (const l of located) {
    if (l) {
      cuts.add(l.start)
      cuts.add(l.end)
    }
  }
  const points = [...cuts].sort((a, b) => a - b)
  const spans: Span[] = []
  for (let i = 0; i + 1 < points.length; i++) {
    const start = points[i]
    const end = points[i + 1]
    const chunks: number[] = []
    const starts: number[] = []
    located.forEach((l, idx) => {
      if (!l) return
      if (l.start <= start && l.end >= end) chunks.push(idx)
      if (l.start === start) starts.push(idx)
    })
    spans.push({ start, end, text: text.slice(start, end), chunks, starts })
  }
  return spans
}

/** Chunk colors come from the graph palette tokens (4.5:1 in both themes); the fill is a tint so text keeps its own contrast. */
const tone = (i: number) => `var(--n${(i % 6) + 1})`

function spanStyle(chunks: number[]): CSSProperties {
  if (chunks.length === 0) return {}
  if (chunks.length === 1) {
    const c = tone(chunks[0])
    return { backgroundColor: `rgb(${c} / 0.16)`, boxShadow: `inset 0 -2px 0 rgb(${c})` }
  }
  const a = tone(chunks[0])
  const b = tone(chunks[1])
  return {
    backgroundImage: `repeating-linear-gradient(135deg, rgb(${a} / 0.3) 0 4px, rgb(${b} / 0.3) 4px 8px)`,
    boxShadow: `inset 0 -2px 0 rgb(${a}), inset 0 -4px 0 rgb(${b})`,
  }
}

interface ChunkSpansProps {
  text: string
  chunks: AnnotatedChunk[]
}

export function ChunkSpans({ text, chunks }: ChunkSpansProps) {
  const located = locateChunks(text, chunks)
  const spans = buildSpans(text, located)
  const missing = located.filter((l) => l === null).length
  return (
    <div>
      <p
        data-testid="chunk-text"
        className="max-h-80 overflow-auto whitespace-pre-wrap break-words rounded-lg border border-border bg-surface-2 p-3 font-mono text-xs leading-6"
      >
        {spans.map((s) => (
          <span
            key={s.start}
            style={spanStyle(s.chunks)}
            data-chunks={s.chunks.join(' ') || undefined}
            data-label={s.starts.length > 0 ? s.starts.map((i) => `#${i}`).join('/') : undefined}
            className={
              s.starts.length > 0
                ? 'border-l-2 border-text/60 pl-px before:mr-0.5 before:align-super before:text-[9px] before:font-semibold before:text-muted before:content-[attr(data-label)]'
                : undefined
            }
          >
            {s.text}
          </span>
        ))}
      </p>
      <ol className="mt-2 flex flex-wrap gap-1.5" aria-label="Chunks">
        {chunks.map((c, i) => (
          <li key={i}>
            <Chip mono className="border border-border bg-surface">
              <span aria-hidden="true" className="h-2 w-2 rounded-sm" style={{ backgroundColor: `rgb(${tone(i)})` }} />
              <Fragment>
                #{i} {c.char_count} chars, ~{c.token_estimate} tok
                {c.overlap_chars > 0 ? `, overlap ${c.overlap_chars} B` : ''}
                {located[i] === null ? ', not in source' : ''}
              </Fragment>
            </Chip>
          </li>
        ))}
      </ol>
      {missing > 0 && (
        <p className="mt-1 text-xs text-muted">
          {missing} chunk{missing === 1 ? ' is' : 's are'} generated text that does not appear verbatim in the source, so it is listed but not highlighted.
        </p>
      )}
      {chunks.some((c) => c.overlap_chars > 0) && (
        <p className="mt-1 text-xs text-muted">Striped text with two underlines belongs to two overlapping chunks.</p>
      )}
    </div>
  )
}
