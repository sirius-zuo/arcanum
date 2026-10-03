import { AlertTriangle, CheckCircle2, XCircle } from 'lucide-react'
import type { Verdict, VerifyResponse } from '../api/types'
import { summarize } from '../lib/verdicts'
import { VerdictChip } from './VerdictChip'

const ORDER: Verdict[] = ['supported', 'uncited_supported', 'partial', 'miscited', 'unsupported', 'no_claim']

const tokens = (n: number | null) => (n === null ? 'not reported' : String(n))

/** Pass or fail banner, the six counts, judge usage and the judge's name. */
export function OverallVerdict({ result }: { result: VerifyResponse }) {
  const pass = result.verdict === 'pass'
  const Icon = pass ? CheckCircle2 : XCircle
  const tone = pass ? 'border-v-supported/40 bg-v-supported/10 text-vink-supported' : 'border-v-unsupported/40 bg-v-unsupported/10 text-vink-unsupported'
  return (
    <section aria-label="Overall verdict" className="space-y-3 rounded-card border border-border bg-surface p-4">
      <div className="flex flex-wrap items-center gap-3">
        <span className={`inline-flex items-center gap-2 rounded-lg border px-3 py-1.5 text-sm font-semibold ${tone}`}>
          <Icon className="h-4 w-4" aria-hidden="true" />
          {pass ? 'Verified: pass' : 'Verified: fail'}
        </span>
        <span className="text-sm text-muted">{summarize(result.counts).label}</span>
        {result.strict_citations && <span className="text-xs text-muted">strict citations on: miscited and uncited sentences also fail</span>}
        <span className="ml-auto font-mono text-[11px] text-muted">
          judge {result.judge.name} ({result.judge.model})
        </span>
      </div>
      <ul className="flex flex-wrap gap-1.5" aria-label="Counts by verdict">
        {ORDER.map((v) => (
          <li key={v}>
            <VerdictChip verdict={v} count={result.counts[v]} />
          </li>
        ))}
      </ul>
      <p className="flex flex-wrap gap-x-4 gap-y-1 text-xs text-muted">
        <span>
          judge calls <span className="font-mono text-text">{result.usage.judge_calls}</span>
        </span>
        <span>
          tokens in <span className="font-mono text-text">{tokens(result.usage.input_tokens)}</span>
        </span>
        <span>
          out <span className="font-mono text-text">{tokens(result.usage.output_tokens)}</span>
        </span>
      </p>
      {result.passages_unavailable.length > 0 && (
        <p className="flex items-start gap-1.5 text-xs text-vink-partial">
          <AlertTriangle className="mt-px h-3.5 w-3.5 shrink-0" aria-hidden="true" />
          <span>
            Passages unavailable (their chunks are gone, so they could not support a claim):{' '}
            <span className="font-mono">{result.passages_unavailable.join(', ')}</span>
          </span>
        </p>
      )}
    </section>
  )
}
