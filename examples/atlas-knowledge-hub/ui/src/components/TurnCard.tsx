import { AlertTriangle, Library, Square } from 'lucide-react'
import type { ReactNode } from 'react'
import { useId, useMemo, useState } from 'react'
import { Link } from 'react-router-dom'
import type { GenerateMode, VerificationOutcome } from '../api/types'
import type { AskState } from '../state/ask'
import { verifyUnavailableMessage } from '../state/ask'
import { AnswerStream } from './AnswerStream'
import { Card } from './Card'
import { Chip } from './Chip'
import { ErrorState } from './ErrorState'
import { PassagePanel } from './PassagePanel'
import type { PassageTarget } from './PassagePanel'
import { VerificationSlot } from './VerificationSlot'

export interface TurnMeta {
  question: string
  mode: GenerateMode
  /** Generator the user picked, if not the default. */
  generator?: string
  verify: boolean
}

export interface VerificationRenderArgs {
  requested: boolean
  verification: VerificationOutcome | null | undefined
  running: boolean
  answer: string
  state: AskState
}

interface TurnCardProps {
  meta: TurnMeta
  state: AskState
  /** Task 15 plugs the sentence-level verification view in here. */
  renderVerification?: (args: VerificationRenderArgs) => ReactNode
}

export function formatMs(ms: number | null): string {
  if (ms === null) return '-'
  return ms < 1000 ? `${Math.round(ms)} ms` : `${(ms / 1000).toFixed(1)} s`
}

function Stat({ label, value }: { label: string; value: string }) {
  return (
    <span className="inline-flex items-baseline gap-1.5">
      <span className="text-muted">{label}</span>
      <span className="font-mono text-text">{value}</span>
    </span>
  )
}

export function TurnCard({ meta, state, renderVerification }: TurnCardProps) {
  const idPrefix = useId()
  const [target, setTarget] = useState<PassageTarget | null>(null)
  const passages = state.context?.passages ?? []
  const streaming = state.phase === 'streaming'
  const answering = streaming && state.outcome === null
  const outcome = state.outcome

  const cited = useMemo(() => {
    const ids = new Set<string>()
    for (const c of outcome?.citations ?? []) ids.add(c.ref_id)
    if (!outcome) for (const m of state.answer.matchAll(/P\d{1,3}/g)) ids.add(m[0])
    return ids
  }, [outcome, state.answer])

  const select = (refId: string) => {
    setTarget((t) => ({ refId, tick: (t?.tick ?? 0) + 1 }))
  }

  const verifyBlocked = verifyUnavailableMessage(state, meta.verify)
  const failedBeforeStart = state.phase === 'error' && state.context === null && state.answer === ''
  const verification = (renderVerification ?? ((a) => <VerificationSlot requested={a.requested} verification={a.verification} running={a.running} />))({
    requested: meta.verify,
    verification: state.verification,
    running: streaming && outcome !== null,
    answer: state.answer,
    state,
  })

  return (
    <article className="space-y-3 animate-rise" aria-label={`Question: ${meta.question}`}>
      <div className="flex justify-end">
        <p className="max-w-[85%] whitespace-pre-wrap rounded-2xl rounded-br-md bg-accent px-4 py-2.5 text-[15px] text-accent-fg shadow-soft">{meta.question}</p>
      </div>

      {failedBeforeStart ? (
        <ErrorState
          title={verifyBlocked ? 'Verification is unavailable' : 'Could not generate an answer'}
          message={state.error ?? 'Unknown error'}
          fix={verifyBlocked ? 'Turn off Verify answer to ask without a judge.' : undefined}
        />
      ) : (
        <div className="grid gap-4 lg:grid-cols-[minmax(0,1fr)_340px]">
          <Card className="min-w-0 space-y-4 p-5">
            {state.answer === '' && streaming ? (
              <p className="text-sm text-muted" role="status">
                {state.context ? 'Waiting for the first token...' : 'Retrieving passages...'}
              </p>
            ) : (
              <AnswerStream text={state.answer} streaming={answering} passages={passages} onSelect={select} />
            )}

            {state.stopped && (
              <p className="flex items-center gap-1.5 text-xs text-muted">
                <Square className="h-3 w-3 fill-current" aria-hidden="true" />
                Stopped. The text above is incomplete.
              </p>
            )}
            {state.phase === 'error' && (
              <div role="alert" className="flex items-start gap-2 rounded-lg border border-v-unsupported/30 bg-v-unsupported/5 px-3 py-2 text-sm">
                <AlertTriangle className="mt-0.5 h-4 w-4 shrink-0 text-v-unsupported" aria-hidden="true" />
                <p>
                  <span className="font-medium">The answer is incomplete.</span> <span className="text-muted">{state.error}</span>
                </p>
              </div>
            )}

            {outcome?.status === 'no_context' && (
              <div className="flex items-start gap-2 rounded-lg border border-border bg-surface-2/60 px-3 py-2 text-sm">
                <Library className="mt-0.5 h-4 w-4 shrink-0 text-accent" aria-hidden="true" />
                <p className="text-muted">
                  Nothing in the collection matched, so the generator was not called. If you have not loaded the sample corpus yet,{' '}
                  <Link to="/library" className="font-medium text-accent underline-offset-2 hover:underline">
                    load it in the Library
                  </Link>
                  .
                </p>
              </div>
            )}

            {outcome && outcome.unknown_refs.length > 0 && (
              <p className="flex items-start gap-1.5 text-xs text-v-partial">
                <AlertTriangle className="mt-px h-3.5 w-3.5 shrink-0" aria-hidden="true" />
                <span>
                  Unknown references: <span className="font-mono">{outcome.unknown_refs.join(', ')}</span>. The model cited ids that match no passage.
                </span>
              </p>
            )}
            {outcome?.stop_reason === 'max_tokens' && (
              <p className="flex items-center gap-1.5 text-xs text-v-partial">
                <AlertTriangle className="h-3.5 w-3.5" aria-hidden="true" />
                The answer hit the token limit and was cut off.
              </p>
            )}

            {(outcome || state.ttft !== null || streaming) && (
              <div role="group" aria-label="Outcome" className="flex flex-wrap items-center gap-x-4 gap-y-1.5 border-t border-border pt-3 text-xs">
                {outcome && (
                  <>
                    <Chip tone={outcome.status === 'ok' ? 'supported' : 'neutral'}>{outcome.status}</Chip>
                    <Stat label="stop" value={outcome.stop_reason} />
                    <Stat label="tokens in" value={String(outcome.usage.input_tokens ?? '-')} />
                    <Stat label="out" value={String(outcome.usage.output_tokens ?? '-')} />
                    <Stat label="generator" value={`${outcome.generator.name} (${outcome.generator.model})`} />
                  </>
                )}
                <Stat label="first token" value={formatMs(state.ttft)} />
                <Stat label="elapsed" value={formatMs(state.elapsed)} />
              </div>
            )}

            {verification}
          </Card>

          {state.context ? (
            <PassagePanel passages={passages} cited={cited} target={target} idPrefix={idPrefix} />
          ) : (
            streaming && <div className="skeleton relative h-40 overflow-hidden rounded-lg bg-surface-2" aria-hidden="true" />
          )}
        </div>
      )}
    </article>
  )
}
