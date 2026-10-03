import { AlertTriangle, CheckCircle2, Loader2, XCircle } from 'lucide-react'
import type { Verdict, VerificationOutcome } from '../api/types'
import { summarize, verdictMeta } from '../lib/verdicts'
import { Chip } from './Chip'
import type { ChipTone } from './Chip'

const VERDICTS: Verdict[] = ['supported', 'uncited_supported', 'partial', 'miscited', 'unsupported', 'no_claim']
const TONE: Record<Verdict, ChipTone> = {
  supported: 'supported',
  uncited_supported: 'uncited',
  partial: 'partial',
  miscited: 'miscited',
  unsupported: 'unsupported',
  no_claim: 'noclaim',
}

export interface VerificationSlotProps {
  /** The user asked for verification on this turn. */
  requested: boolean
  /** undefined: not arrived yet; null: arrived with nothing to verify. */
  verification: VerificationOutcome | null | undefined
  /** The run is still going, so a missing verification may still arrive. */
  running: boolean
}

/**
 * Compact verification strip: overall verdict and counts. Task 15 replaces
 * this through TurnCard's `renderVerification` with the sentence-level view.
 */
export function VerificationSlot({ requested, verification, running }: VerificationSlotProps) {
  if (!requested) return null
  const box = 'rounded-lg border border-border bg-surface-2/60 px-3 py-2 text-sm'

  if (verification === undefined) {
    if (!running) return null
    return (
      <div className={`${box} flex items-center gap-2 text-muted`} role="status">
        <Loader2 className="h-4 w-4 animate-spin" aria-hidden="true" />
        Verifying the answer against its passages...
      </div>
    )
  }
  if (verification === null) {
    return <div className={`${box} text-muted`}>Nothing to verify: no passages were found for this question.</div>
  }
  if (verification.status === 'error') {
    return (
      <div role="alert" className="flex items-start gap-2 rounded-lg border border-v-partial/40 bg-v-partial/10 px-3 py-2 text-sm">
        <AlertTriangle className="mt-0.5 h-4 w-4 shrink-0 text-v-partial" aria-hidden="true" />
        <div>
          <p className="font-medium text-v-partial">Verification failed, the answer above is unaffected</p>
          <p className="mt-0.5 text-muted">
            <span className="font-mono text-xs">{verification.code}</span>: {verification.message}
          </p>
        </div>
      </div>
    )
  }
  const pass = verification.verdict === 'pass'
  const Icon = pass ? CheckCircle2 : XCircle
  return (
    <div className={`${box} space-y-2`}>
      <div className="flex flex-wrap items-center gap-2">
        <span className={`inline-flex items-center gap-1.5 font-semibold ${pass ? 'text-v-supported' : 'text-v-unsupported'}`}>
          <Icon className="h-4 w-4" aria-hidden="true" />
          {pass ? 'Verified: pass' : 'Verified: fail'}
        </span>
        <span className="text-xs text-muted">{summarize(verification.counts).label}</span>
        <span className="ml-auto font-mono text-[11px] text-muted">judge {verification.judge.name}</span>
      </div>
      <ul className="flex flex-wrap gap-1.5">
        {VERDICTS.filter((v) => verification.counts[v] > 0).map((v) => (
          <li key={v}>
            <Chip tone={TONE[v]}>
              {verdictMeta(v).label}
              <span className="font-mono">{verification.counts[v]}</span>
            </Chip>
          </li>
        ))}
      </ul>
    </div>
  )
}
