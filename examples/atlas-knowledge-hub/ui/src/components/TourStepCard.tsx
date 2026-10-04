import { ArrowRight, Check } from 'lucide-react'
import { Link } from 'react-router-dom'
import type { TourStep } from '../api/types'

interface TourStepCardProps {
  step: TourStep
  position: number
  total: number
  done: boolean
}

/** One step: what it is, why it matters, what to do, and the payoff once it completes. */
export function TourStepCard({ step, position, total, done }: TourStepCardProps) {
  return (
    <div className="space-y-2">
      <p className="font-mono text-[11px] uppercase tracking-[0.12em] text-muted">
        Step {position} of {total}
      </p>
      <h2 className="text-[15px] font-semibold leading-snug">{step.title}</h2>
      <p className="text-sm leading-relaxed text-muted">{step.why}</p>
      <p className="text-sm leading-relaxed">
        <span className="font-medium">Do this: </span>
        {step.action}
      </p>
      <Link to={step.route} className="inline-flex items-center gap-1 text-sm font-medium text-accent underline-offset-2 hover:underline">
        Take me there
        <ArrowRight className="h-3.5 w-3.5" aria-hidden="true" />
      </Link>
      <div role="status" aria-live="polite">
        {done && (
          <p className="mt-1 flex items-start gap-2 rounded-lg border border-border bg-surface-2 px-3 py-2 text-sm">
            <Check className="mt-0.5 h-4 w-4 shrink-0 text-v-supported" aria-hidden="true" />
            <span>
              <span className="font-medium">Step complete. </span>
              {step.payoff}
            </span>
          </p>
        )}
      </div>
    </div>
  )
}
