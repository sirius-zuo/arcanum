import { ArrowUp, Square } from 'lucide-react'
import { useEffect, useState } from 'react'
import type { FormEvent, KeyboardEvent } from 'react'
import { Link } from 'react-router-dom'
import type { Bootstrap, GenerateMode } from '../api/types'
import { ASK_PREFILL_KEY } from '../lib/askPrefill'
import { Card } from './Card'

export interface AskSubmit {
  question: string
  mode: GenerateMode
  /** Only set for a non-default generator. */
  generator: string | undefined
  verify: boolean
}

export const EXAMPLE_QUESTIONS = [
  'Who is the on-call lead for the team that owns the navigation stack?',
  'How much annual leave do I get each year?',
  'Why did 212 robots stop moving at the Northgate warehouse in Utrecht?',
  'How long does the HX-2 battery last and how fast does it charge?',
]

interface AskFormProps {
  boot: Bootstrap
  /** A run is in flight: the submit button becomes Stop. */
  busy: boolean
  onSubmit: (v: AskSubmit) => void
  onStop: () => void
  /** Set when the server refused verification; disables the toggle and explains why. */
  verifyDisabledReason?: string | null
  /** A turn failed: put its question back (when the box is empty) so the user can retry. */
  restore?: { key: number; text: string } | null
  /** Hide the example chips once a conversation has started, to keep the sticky form short. */
  showExamples?: boolean
}

/** Read and remove the question another page left for Ask. Safe when storage is blocked. */
function consumePrefill(): string | null {
  try {
    const v = sessionStorage.getItem(ASK_PREFILL_KEY)
    if (v !== null) sessionStorage.removeItem(ASK_PREFILL_KEY)
    return v
  } catch {
    return null
  }
}

export function AskForm({ boot, busy, onSubmit, onStop, verifyDisabledReason, restore, showExamples = true }: AskFormProps) {
  const defaultGen = boot.generators.find((g) => g.is_default)?.name ?? boot.generators[0]?.name ?? ''
  const [question, setQuestion] = useState('')
  const [mode, setMode] = useState<GenerateMode>('answer')
  const [generator, setGenerator] = useState(defaultGen)
  const [verify, setVerify] = useState(false)

  // An effect, not an initializer: it survives StrictMode's double render and consumes once.
  useEffect(() => {
    const v = consumePrefill()
    if (v) setQuestion(v)
  }, [])

  const restoreKey = restore?.key
  const restoreText = restore?.text
  useEffect(() => {
    if (restoreKey !== undefined && restoreText) setQuestion((q) => (q.trim() === '' ? restoreText : q))
  }, [restoreKey, restoreText])

  const noJudge = !boot.features.verify || !boot.judge
  const reason = verifyDisabledReason
    ? verifyDisabledReason
    : noJudge
      ? 'Verification is unavailable because no judge is configured on this server.'
      : null
  const filled = question.trim() !== ''

  const submit = (e?: FormEvent) => {
    e?.preventDefault()
    if (!filled || busy) return
    onSubmit({
      question: question.trim(),
      mode,
      generator: generator && generator !== defaultGen ? generator : undefined,
      verify: verify && !reason,
    })
    setQuestion('')
  }

  const onKey = (e: KeyboardEvent<HTMLTextAreaElement>) => {
    if (e.key === 'Enter' && (e.metaKey || e.ctrlKey)) submit()
  }

  return (
    <Card className="p-3 shadow-lift">
      <form onSubmit={submit}>
        <label htmlFor="ask-question" className="sr-only">
          Question
        </label>
        <textarea
          id="ask-question"
          value={question}
          rows={2}
          onChange={(e) => setQuestion(e.target.value)}
          onKeyDown={onKey}
          placeholder={mode === 'answer' ? 'Ask the Halcyon corpus anything...' : 'Name a topic to summarize...'}
          className="w-full resize-none rounded-lg bg-transparent px-2 py-1.5 text-[15px] outline-none placeholder:text-muted"
        />
        <div className="mt-1 flex flex-wrap items-center gap-x-4 gap-y-2 border-t border-border pt-2">
          <div role="radiogroup" aria-label="Mode" className="inline-flex rounded-lg border border-border bg-surface-2 p-0.5">
            {(['answer', 'summarize'] as const).map((m) => (
              <button
                key={m}
                type="button"
                role="radio"
                aria-checked={mode === m}
                onClick={() => setMode(m)}
                className={`h-7 rounded-md px-3 text-xs capitalize transition ${mode === m ? 'bg-surface font-medium shadow-soft' : 'text-muted hover:text-text'}`}
              >
                {m}
              </button>
            ))}
          </div>

          <label className="flex items-center gap-1.5 text-xs text-muted">
            Generator
            <select
              aria-label="Generator"
              value={generator}
              disabled={boot.generators.length < 2}
              onChange={(e) => setGenerator(e.target.value)}
              className="h-7 max-w-[14rem] rounded-lg border border-border bg-surface px-2 text-xs text-text"
            >
              {boot.generators.map((g) => (
                <option key={g.name} value={g.name}>
                  {g.name} ({g.model}){g.is_default ? ' default' : ''}
                </option>
              ))}
            </select>
          </label>

          <label className={`flex items-center gap-1.5 text-xs ${reason ? 'text-muted' : ''}`}>
            <input
              type="checkbox"
              checked={verify && !reason}
              disabled={Boolean(reason)}
              onChange={(e) => setVerify(e.target.checked)}
              className="h-3.5 w-3.5 accent-[rgb(var(--accent))]"
            />
            Verify answer
          </label>

          <div className="ml-auto flex items-center gap-2">
            <span className="hidden text-[11px] text-muted sm:inline">Ctrl or Cmd + Enter</span>
            {busy ? (
              <button
                type="button"
                onClick={onStop}
                className="inline-flex h-9 items-center gap-2 rounded-lg border border-border bg-surface px-4 text-sm font-medium transition hover:shadow-soft"
              >
                <Square className="h-3.5 w-3.5 fill-current" aria-hidden="true" />
                Stop
              </button>
            ) : (
              <button
                type="submit"
                disabled={!filled}
                className="inline-flex h-9 items-center gap-2 rounded-lg bg-accent px-4 text-sm font-medium text-accent-fg transition hover:opacity-90 disabled:opacity-50"
              >
                <ArrowUp className="h-4 w-4" aria-hidden="true" />
                {mode === 'answer' ? 'Ask' : 'Summarize'}
              </button>
            )}
          </div>
        </div>

        <p className="mt-2 text-[11px] leading-relaxed text-muted">
          {reason ?? (
            <>
              {boot.judge && <span className="font-mono">{`Judge: ${boot.judge}`}</span>}
              {' (read only). '}
            </>
          )}
          {!reason && (
            <>
              Pick the judge and strict citations in the{' '}
              <Link to="/verify" className="text-accent underline-offset-2 hover:underline">
                Verify lab
              </Link>
              .
            </>
          )}
        </p>
      </form>

      {showExamples && (
      <ul className="mt-2 flex flex-wrap gap-1.5" aria-label="Example questions">
        {EXAMPLE_QUESTIONS.map((q) => (
          <li key={q}>
            <button
              type="button"
              onClick={() => setQuestion(q)}
              className="rounded-full border border-border bg-surface-2/60 px-2.5 py-1 text-xs text-muted transition hover:border-accent/50 hover:text-text"
            >
              {q}
            </button>
          </li>
        ))}
      </ul>
      )}
    </Card>
  )
}
