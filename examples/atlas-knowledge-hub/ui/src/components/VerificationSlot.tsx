import { AlertTriangle, Info, Loader2 } from 'lucide-react'
import { useState } from 'react'
import type { VerificationOutcome } from '../api/types'
import { EvidenceView } from './EvidenceView'
import { OverallVerdict } from './OverallVerdict'
import { SentenceList } from './SentenceList'

export interface VerificationSlotProps {
  /** The user asked for verification on this turn. */
  requested: boolean
  /** undefined: not arrived yet; null: arrived with nothing to verify. */
  verification: VerificationOutcome | null | undefined
  /** The run is still going, so a missing verification may still arrive. */
  running: boolean
  /** The full streamed answer: the verified spans are byte ranges into exactly this text. */
  answer: string
  /** The answer finished (the `done` event arrived). */
  finished: boolean
  stopped?: boolean
}

/** What Ask shows under an answer when verification was requested. */
export function VerificationSlot({ requested, verification, running, answer, finished, stopped }: VerificationSlotProps) {
  const [selected, setSelected] = useState<number | null>(null)
  if (!requested) return null
  const box = 'rounded-lg border border-border bg-surface-2/60 px-3 py-2 text-sm'

  if (verification === undefined) {
    if (running) {
      return (
        <div className={`${box} flex items-center gap-2 text-muted`} role="status">
          <Loader2 className="h-4 w-4 animate-spin" aria-hidden="true" />
          Verifying the answer against its passages...
        </div>
      )
    }
    if (!finished) return null
    return (
      <div className={`${box} flex items-start gap-2 text-muted`} role="status">
        <Info className="mt-0.5 h-4 w-4 shrink-0" aria-hidden="true" />
        {stopped ? 'Verification was requested but you stopped before its result arrived.' : 'Verification was requested but no result arrived.'}
      </div>
    )
  }
  if (verification === null) {
    return <div className={`${box} text-muted`}>Nothing to verify: no passages were found for this question.</div>
  }
  if (verification.status === 'error') {
    return (
      <div role="alert" className="flex items-start gap-2 rounded-lg border border-v-partial/40 bg-v-partial/10 px-3 py-2 text-sm">
        <AlertTriangle className="mt-0.5 h-4 w-4 shrink-0 text-vink-partial" aria-hidden="true" />
        <div>
          <p className="font-medium text-vink-partial">Verification failed, the answer above is unaffected</p>
          <p className="mt-0.5 text-muted">
            <span className="font-mono text-xs">{verification.code}</span>: {verification.message}
          </p>
        </div>
      </div>
    )
  }
  const picked = selected !== null ? verification.sentences[selected] : undefined
  return (
    <div className="space-y-3 border-t border-border pt-4">
      <OverallVerdict result={verification} />
      <div>
        <h3 className="mb-2 text-xs font-semibold uppercase tracking-wider text-muted">Verified answer</h3>
        <SentenceList answer={answer} sentences={verification.sentences} selected={selected} onSelect={setSelected} />
        <p className="mt-2 text-xs text-muted">Select a sentence to see its claims and the source text behind them.</p>
      </div>
      {picked && <EvidenceView sentence={picked} />}
    </div>
  )
}
