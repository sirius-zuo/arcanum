import { MessageSquareText, Merge } from 'lucide-react'
import { useNavigate } from 'react-router-dom'
import type { ContextPassage } from '../api/types'
import { Card } from './Card'
import { Chip } from './Chip'
import { CopyButton } from './CopyButton'
import { StrategyBadge } from './StrategyBadge'
import { ASK_PREFILL_KEY } from '../lib/askPrefill'


interface PassageListProps {
  passages: ContextPassage[]
  /** The question that produced these passages; sent to Ask. */
  question: string
}

export function PassageList({ passages, question }: PassageListProps) {
  const navigate = useNavigate()

  const sendToAsk = () => {
    try {
      sessionStorage.setItem(ASK_PREFILL_KEY, question)
    } catch {
      // storage blocked: Ask just opens empty
    }
    navigate('/ask')
  }

  return (
    <ol className="space-y-3">
      {passages.map((p) => (
        <li key={p.ref_id}>
          <Card className="p-4">
            <div className="mb-3 flex flex-wrap items-center gap-2">
              <Chip tone="accent" mono>
                {p.ref_id}
              </Chip>
              {p.strategies.map((s) => (
                <StrategyBadge key={s} strategy={s} />
              ))}
              {p.chunk_ids.length > 1 && (
                <Chip icon={<Merge className="h-3 w-3" aria-hidden="true" />}>merged from {p.chunk_ids.length} chunks</Chip>
              )}
              <span className="ml-auto text-xs text-muted" title="Reciprocal rank fusion score">
                fused score <span className="font-mono text-text">{p.score.toFixed(4)}</span>
              </span>
            </div>
            <p className="whitespace-pre-wrap text-sm leading-relaxed">{p.text}</p>
            <div className="mt-3 space-y-2 border-t border-border pt-3">
              <p className="break-all font-mono text-[11px] text-muted">
                {p.source_uri} · v{p.version_num}
                {p.page !== null && ` · page ${p.page}`}
                {p.section && ` · ${p.section}`}
                {' · bytes '}
                <span>{`${p.offset_start}-${p.offset_end}`}</span>
              </p>
              <ul className="flex flex-wrap items-center gap-1.5">
                {p.chunk_ids.map((id) => (
                  <li key={id} className="inline-flex items-center rounded-md bg-surface-2 pl-1.5 font-mono text-[11px]">
                    <span className="max-w-[16rem] truncate">{id}</span>
                    <CopyButton text={id} label={`Copy chunk id ${id}`} />
                  </li>
                ))}
                <li className="ml-auto">
                  <button
                    type="button"
                    onClick={sendToAsk}
                    className="inline-flex h-7 items-center gap-1.5 rounded-lg border border-border px-2 text-xs font-medium text-accent transition hover:shadow-soft"
                  >
                    <MessageSquareText className="h-3.5 w-3.5" aria-hidden="true" />
                    Send to Ask
                  </button>
                </li>
              </ul>
            </div>
          </Card>
        </li>
      ))}
    </ol>
  )
}
