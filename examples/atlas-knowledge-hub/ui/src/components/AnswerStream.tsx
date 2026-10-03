import { Fragment, useMemo } from 'react'
import type { ContextPassage } from '../api/types'
import { splitAnswer } from '../lib/citations'
import { CitationChip } from './CitationChip'

interface AnswerStreamProps {
  text: string
  streaming: boolean
  passages: ContextPassage[]
  onSelect: (refId: string) => void
}

/** The answer text with citation markers turned into chips and a caret while it streams. */
export function AnswerStream({ text, streaming, passages, onSelect }: AnswerStreamProps) {
  const parts = useMemo(() => splitAnswer(text), [text])
  const byRef = useMemo(() => new Map(passages.map((p) => [p.ref_id, p])), [passages])

  return (
    <div aria-busy={streaming} className="whitespace-pre-wrap text-[15px] leading-7">
      {parts.map((part, i) =>
        part.kind === 'text' ? (
          <Fragment key={i}>{part.text}</Fragment>
        ) : (
          <span key={i} className="animate-chip-in">
            {part.ids.map((id) => (
              <CitationChip key={id} refId={id} passage={byRef.get(id)} onSelect={onSelect} />
            ))}
          </span>
        ),
      )}
      {streaming && <span aria-hidden="true" className="atlas-caret" />}
    </div>
  )
}
