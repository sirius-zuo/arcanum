import { Layers, Plus, X } from 'lucide-react'
import { useRef, useState } from 'react'
import type { FormEvent } from 'react'
import { Link } from 'react-router-dom'
import { ApiError } from '../api/client'
import { useBuildContext } from '../api/context'
import type { ChatMessage, ContextRequest, ContextResponse, RenderFormat } from '../api/types'
import { BudgetBar } from '../components/BudgetBar'
import { Card } from '../components/Card'
import { Chip } from '../components/Chip'
import { EmptyState } from '../components/EmptyState'
import { ErrorState } from '../components/ErrorState'
import { HowItWorks } from '../components/HowItWorks'
import { PageHeader } from '../components/PageHeader'
import { PassageList } from '../components/PassageList'
import { RenderTabs } from '../components/RenderTabs'
import { StrategyBadge } from '../components/StrategyBadge'
import { ROUTES, routeMeta } from '../routes'
import { useBootstrap } from '../state/bootstrap'

const meta = routeMeta('/context')
const step = String(ROUTES.indexOf(meta) + 1).padStart(2, '0')

const HOW_ARCANUM = ['POST /api/v1/context']
const HOW_DEMO: string[] = []

/** Tour hook point: Task 19 wires this to the tour. */
const onContextBuilt = (): void => {}

type Mode = 'query' | 'conversation'

interface Built {
  format: RenderFormat
  data: ContextResponse
  question: string
}

function Slider(props: { id: string; label: string; min: number; max: number; step: number; value: number; onChange: (n: number) => void; hint: string }) {
  return (
    <div>
      <div className="mb-1 flex items-baseline justify-between text-xs">
        <label htmlFor={props.id} className="font-medium text-muted">
          {props.label}
        </label>
        <span className="font-mono">{props.value}</span>
      </div>
      <input
        id={props.id}
        type="range"
        min={props.min}
        max={props.max}
        step={props.step}
        value={props.value}
        onChange={(e) => props.onChange(Number(e.target.value))}
        className="w-full accent-[rgb(var(--accent))]"
      />
      <p className="mt-0.5 text-[11px] text-muted">{props.hint}</p>
    </div>
  )
}

export default function ContextPage() {
  const { data: boot } = useBootstrap()
  const build = useBuildContext()
  const [mode, setMode] = useState<Mode>('query')
  const [query, setQuery] = useState('')
  const [messages, setMessages] = useState<ChatMessage[]>([{ role: 'user', content: '' }])
  const [tokenBudget, setTokenBudget] = useState(4000)
  const [share, setShare] = useState(0.2)
  const [candidateK, setCandidateK] = useState(50)
  const [format, setFormat] = useState<RenderFormat>('xml')
  const [built, setBuilt] = useState<Built | null>(null)
  const lastReq = useRef<{ req: ContextRequest; question: string } | null>(null)

  const lastUser = [...messages].reverse().find((m) => m.role === 'user')
  const lastIsUser = messages.length > 0 && messages[messages.length - 1].role === 'user'
  const filled = mode === 'query' ? query.trim() !== '' : lastIsUser && messages.every((m) => m.content.trim() !== '')

  const send = (req: ContextRequest, question: string) => {
    lastReq.current = { req, question }
    build.mutate(req, {
      onSuccess: (data) => {
        setBuilt({ format: req.render ?? 'xml', data, question })
        onContextBuilt()
      },
    })
  }

  const submit = (e: FormEvent) => {
    e.preventDefault()
    if (!boot || !filled) return
    const base: ContextRequest = {
      collection_id: boot.collection,
      token_budget: tokenBudget,
      background_share: share,
      candidate_k: candidateK,
      render: format,
    }
    if (mode === 'query') send({ ...base, query: query.trim() }, query.trim())
    else send({ ...base, messages }, lastUser?.content.trim() ?? '')
  }

  const switchFormat = (f: RenderFormat) => {
    setFormat(f)
    if (built && built.data.rendered !== undefined && built.format === f) return
    const last = lastReq.current
    if (last) send({ ...last.req, render: f }, last.question)
  }

  const setMessage = (i: number, patch: Partial<ChatMessage>) =>
    setMessages((ms) => ms.map((m, j) => (j === i ? { ...m, ...patch } : m)))

  const err = build.error
  const noRegistry = err instanceof ApiError && err.status === 503 && /chunk registry/i.test(err.message)
  const data = built?.data
  const shownRendered = built && built.format === format ? built.data.rendered : undefined

  return (
    <>
      <PageHeader
        eyebrow={`${step} / ${meta.label}`}
        title={meta.label}
        description={meta.blurb}
        actions={<HowItWorks arcanum={HOW_ARCANUM} demo={HOW_DEMO} />}
      />

      <form onSubmit={submit} className="mb-8 grid gap-6 lg:grid-cols-[minmax(0,1fr)_320px]">
        <Card className="p-4">
          <div role="tablist" aria-label="Input mode" className="mb-3 inline-flex rounded-lg border border-border bg-surface-2 p-0.5">
            {(['query', 'conversation'] as const).map((m) => (
              <button
                key={m}
                type="button"
                role="tab"
                aria-selected={mode === m}
                onClick={() => setMode(m)}
                className={`h-7 rounded-md px-3 text-xs transition ${mode === m ? 'bg-surface font-medium shadow-soft' : 'text-muted hover:text-text'}`}
              >
                {m === 'query' ? 'Single query' : 'Conversation'}
              </button>
            ))}
          </div>

          {mode === 'query' ? (
            <>
              <label htmlFor="ctx-query" className="mb-1 block text-xs font-medium text-muted">
                Query
              </label>
              <input
                id="ctx-query"
                value={query}
                onChange={(e) => setQuery(e.target.value)}
                placeholder="What do you want context for?"
                className="h-10 w-full rounded-lg border border-border bg-surface px-3 text-sm outline-none focus:border-accent"
              />
            </>
          ) : (
            <div>
              <ul className="space-y-2">
                {messages.map((m, i) => (
                  <li key={i} className="flex items-start gap-2">
                    <select
                      aria-label={`Role of message ${i + 1}`}
                      value={m.role}
                      onChange={(e) => setMessage(i, { role: e.target.value === 'assistant' ? 'assistant' : 'user' })}
                      className="h-9 rounded-lg border border-border bg-surface px-2 text-xs"
                    >
                      <option value="user">user</option>
                      <option value="assistant">assistant</option>
                    </select>
                    <textarea
                      aria-label={`Content of message ${i + 1}`}
                      value={m.content}
                      rows={2}
                      onChange={(e) => setMessage(i, { content: e.target.value })}
                      className="min-w-0 flex-1 rounded-lg border border-border bg-surface px-3 py-2 text-sm outline-none focus:border-accent"
                    />
                    <button
                      type="button"
                      aria-label={`Remove message ${i + 1}`}
                      disabled={messages.length === 1}
                      onClick={() => setMessages((ms) => ms.filter((_, j) => j !== i))}
                      className="grid h-9 w-9 place-items-center rounded-lg text-muted hover:bg-surface-2 disabled:opacity-40"
                    >
                      <X className="h-4 w-4" aria-hidden="true" />
                    </button>
                  </li>
                ))}
              </ul>
              <button
                type="button"
                onClick={() => setMessages((ms) => [...ms, { role: ms[ms.length - 1]?.role === 'user' ? 'assistant' : 'user', content: '' }])}
                className="mt-2 inline-flex h-7 items-center gap-1 rounded-lg border border-border px-2 text-xs text-muted hover:text-text"
              >
                <Plus className="h-3.5 w-3.5" aria-hidden="true" />
                Add message
              </button>
              {!lastIsUser && <p className="mt-2 text-xs text-v-partial">The last message must be from the user.</p>}
            </div>
          )}

          <button
            type="submit"
            disabled={!filled || !boot || build.isPending}
            className="mt-4 inline-flex h-10 items-center gap-2 rounded-lg bg-accent px-4 text-sm font-medium text-accent-fg transition hover:opacity-90 disabled:opacity-50"
          >
            <Layers className="h-4 w-4" aria-hidden="true" />
            {build.isPending ? 'Building...' : 'Build context'}
          </button>
        </Card>

        <Card className="space-y-4 p-4">
          <Slider id="ctx-budget" label="token_budget" min={200} max={16000} step={100} value={tokenBudget} onChange={setTokenBudget} hint="Upper bound on the assembled context." />
          <Slider id="ctx-share" label="background_share" min={0} max={1} step={0.05} value={share} onChange={setShare} hint="Fraction reserved for background summaries." />
          <Slider id="ctx-k" label="candidate_k" min={1} max={200} step={1} value={candidateK} onChange={setCandidateK} hint="Candidates fetched per strategy." />
        </Card>
      </form>

      <div aria-live="polite">
        {err && !build.isPending && (
          <ErrorState
            title={noRegistry ? 'Context is not available' : err instanceof ApiError && err.status === 503 ? 'Retrieval unavailable' : 'Could not build context'}
            message={err.message}
            action={
              noRegistry ? (
                <Link to="/library" className="text-sm font-medium text-accent underline-offset-2 hover:underline">
                  Check the Library
                </Link>
              ) : undefined
            }
          />
        )}
        {!data && !err && (
          <EmptyState
            icon={Layers}
            title="Build a context"
            description="Context packs the best passages into a token budget, each with a ref id you can cite and a byte range you can trace."
          />
        )}
        {data && built && (
          <div className="space-y-6">
            <Card className="space-y-4 p-4">
              <BudgetBar used={data.usage.used} budget={data.usage.budget} dropped={data.usage.dropped_passages} />
              <dl className="grid gap-x-6 gap-y-2 text-xs sm:grid-cols-2">
                <div>
                  <dt className="text-muted">Resolved query ({data.resolved_query_source})</dt>
                  <dd className="mt-0.5 font-medium">{data.resolved_query}</dd>
                </div>
                {data.resolved_query_source !== 'original' && (
                  <div>
                    <dt className="text-muted">{data.resolved_query_source === 'rewritten' ? 'Conversation rewritten from the last user message' : 'Rewrite failed, used the last user message'}</dt>
                    <dd className="mt-0.5">{built.question}</dd>
                  </div>
                )}
                <div>
                  <dt className="text-muted">Strategies ok</dt>
                  <dd className="mt-1 flex flex-wrap gap-1.5">
                    {data.retrieval.strategies_ok.length === 0 ? <span className="text-muted">none</span> : data.retrieval.strategies_ok.map((s) => <StrategyBadge key={s} strategy={s} />)}
                  </dd>
                </div>
                <div>
                  <dt className="text-muted">Strategies failed</dt>
                  <dd className="mt-1 flex flex-wrap gap-1.5">
                    {data.retrieval.strategies_failed.length === 0 ? (
                      <span className="text-muted">none</span>
                    ) : (
                      data.retrieval.strategies_failed.map((f) => (
                        <span key={f.strategy} title={f.reason}>
                          <Chip tone="unsupported" icon={<X className="h-3 w-3" aria-hidden="true" />}>
                            {f.strategy}: {f.reason}
                          </Chip>
                        </span>
                      ))
                    )}
                  </dd>
                </div>
                <div>
                  <dt className="text-muted">Counts</dt>
                  <dd className="mt-0.5 font-mono">
                    {data.usage.passages} passage tokens, {data.usage.background} background tokens, {data.usage.dropped_passages} dropped ({data.usage.counter})
                  </dd>
                </div>
                {data.retrieval.queries.length > 0 && (
                  <div>
                    <dt className="text-muted">Queries sent to retrieval</dt>
                    <dd className="mt-0.5 font-mono">{data.retrieval.queries.join(' | ')}</dd>
                  </div>
                )}
              </dl>
            </Card>

            <section aria-labelledby="ctx-rendered">
              <h2 id="ctx-rendered" className="mb-2 text-sm font-semibold">
                Rendered context
              </h2>
              <RenderTabs format={format} onFormat={switchFormat} rendered={shownRendered} pending={build.isPending} />
            </section>

            <section aria-labelledby="ctx-passages">
              <h2 id="ctx-passages" className="mb-2 text-sm font-semibold">
                Passages ({data.passages.length})
              </h2>
              {data.passages.length === 0 ? (
                <p className="text-sm text-muted">No passages fit the budget.</p>
              ) : (
                <PassageList passages={data.passages} question={built.question} />
              )}
            </section>

            <section aria-labelledby="ctx-bg">
              <h2 id="ctx-bg" className="mb-2 text-sm font-semibold">
                Background summaries ({data.background.length})
              </h2>
              {data.background.length === 0 ? (
                <p className="text-sm text-muted">No background summaries were selected.</p>
              ) : (
                <ul className="space-y-3">
                  {data.background.map((b) => (
                    <li key={b.ref_id}>
                      <Card className="p-4">
                        <div className="mb-2 flex flex-wrap items-center gap-2">
                          <Chip tone="accent" mono>
                            {b.ref_id}
                          </Chip>
                          <Chip>
                            level {b.level}, covers {b.covers.length} {b.covers.length === 1 ? 'chunk' : 'chunks'}
                          </Chip>
                          <span className="ml-auto text-xs text-muted">
                            fused score <span className="font-mono text-text">{b.score.toFixed(4)}</span>
                          </span>
                        </div>
                        <p className="whitespace-pre-wrap text-sm leading-relaxed">{b.text}</p>
                      </Card>
                    </li>
                  ))}
                </ul>
              )}
            </section>
          </div>
        )}
      </div>
    </>
  )
}
