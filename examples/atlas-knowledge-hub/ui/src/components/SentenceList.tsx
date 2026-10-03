import clsx from 'clsx'
import { Fragment, useMemo } from 'react'
import type { KeyboardEvent } from 'react'
import type { VerifiedSentence } from '../api/types'
import { splitAnswer } from '../lib/citations'
import { byteRangeToIndexRange } from '../lib/offsets'
import { verdictMeta } from '../lib/verdicts'
import { VerdictIcon } from './VerdictChip'

/**
 * Underline per tone. The style differs too (solid, dashed, wavy, double, dotted)
 * so a verdict never rests on color alone. `no_claim` is a faint dotted line.
 */
const UNDERLINE: Record<string, string> = {
  supported: 'decoration-v-supported decoration-solid decoration-2',
  partial: 'decoration-v-partial decoration-dashed decoration-2',
  unsupported: 'decoration-v-unsupported decoration-wavy decoration-2',
  miscited: 'decoration-v-miscited decoration-double decoration-2',
  uncited: 'decoration-v-uncited decoration-dotted decoration-2',
  noclaim: 'decoration-v-noclaim/40 decoration-dotted decoration-1',
}
const TINT: Record<string, string> = {
  supported: 'bg-v-supported/10',
  partial: 'bg-v-partial/10',
  unsupported: 'bg-v-unsupported/10',
  miscited: 'bg-v-miscited/10',
  uncited: 'bg-v-uncited/10',
  noclaim: 'bg-v-noclaim/10',
}
const ICON_TEXT: Record<string, string> = {
  supported: 'text-vink-supported',
  partial: 'text-vink-partial',
  unsupported: 'text-vink-unsupported',
  miscited: 'text-vink-miscited',
  uncited: 'text-vink-uncited',
  noclaim: 'text-vink-noclaim',
}

interface Piece {
  /** Index into `sentences`, or -1 for text no sentence covers. */
  index: number
  text: string
}

/** Cut the answer along the sentence spans (UTF-8 byte ranges). Overlapping or out of order spans are skipped. */
export function cutAnswer(answer: string, sentences: VerifiedSentence[]): Piece[] {
  const order = sentences
    .map((s, index) => ({ index, range: byteRangeToIndexRange(answer, s.span[0], s.span[1]) }))
    .filter((x) => x.range[1] > x.range[0])
    .sort((p, q) => p.range[0] - q.range[0])
  const pieces: Piece[] = []
  let cursor = 0
  for (const { index, range } of order) {
    if (range[0] < cursor) continue
    if (range[0] > cursor) pieces.push({ index: -1, text: answer.slice(cursor, range[0]) })
    pieces.push({ index, text: answer.slice(range[0], range[1]) })
    cursor = range[1]
  }
  if (cursor < answer.length) pieces.push({ index: -1, text: answer.slice(cursor) })
  return pieces
}

function WithCitations({ text }: { text: string }) {
  const parts = splitAnswer(text)
  return (
    <>
      {parts.map((p, i) =>
        p.kind === 'text' ? <Fragment key={i}>{p.text}</Fragment> : (
          <span key={i} className="font-mono text-[0.85em] font-medium text-accent">
            {p.raw}
          </span>
        ),
      )}
    </>
  )
}

interface SentenceListProps {
  answer: string
  sentences: VerifiedSentence[]
  selected: number | null
  onSelect: (index: number) => void
}

/** The answer text with every verified sentence underlined in its verdict tone. Click or Enter selects one. */
export function SentenceList({ answer, sentences, selected, onSelect }: SentenceListProps) {
  const pieces = useMemo(() => cutAnswer(answer, sentences), [answer, sentences])
  const key = (e: KeyboardEvent, i: number) => {
    if (e.key === 'Enter' || e.key === ' ') {
      e.preventDefault()
      onSelect(i)
    }
  }
  return (
    <div data-testid="sentence-list" className="whitespace-pre-wrap text-[15px] leading-8">
      {pieces.map((p, n) => {
        if (p.index < 0) return <Fragment key={n}>{p.text}</Fragment>
        const s = sentences[p.index]
        const meta = verdictMeta(s.verdict)
        const active = selected === p.index
        return (
          <span
            key={n}
            role="button"
            tabIndex={0}
            aria-pressed={active}
            data-verdict={s.verdict}
            onClick={() => onSelect(p.index)}
            onKeyDown={(e) => key(e, p.index)}
            className={clsx(
              'cursor-pointer rounded-sm underline underline-offset-[6px] transition',
              UNDERLINE[meta.tone],
              active ? TINT[meta.tone] : 'hover:bg-surface-2',
              active && 'ring-2 ring-accent/50',
            )}
          >
            <WithCitations text={p.text} />
            {s.verdict !== 'no_claim' && <VerdictIcon verdict={s.verdict} className={clsx('mx-1 inline h-3.5 w-3.5 align-[-2px]', ICON_TEXT[meta.tone])} />}
            <span className="sr-only"> ({meta.label})</span>
          </span>
        )
      })}
    </div>
  )
}
