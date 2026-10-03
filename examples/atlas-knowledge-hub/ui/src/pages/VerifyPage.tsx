import { Check, Info, Layers, RefreshCw, ShieldCheck } from 'lucide-react'
import { useEffect, useRef, useState } from 'react'
import { ApiError } from '../api/client'
import { buildContext } from '../api/context'
import { useSamples } from '../api/library'
import type { ContextRequest, ContextResponse, FlawedAnswer, VerifyRequest, VerifyResponse } from '../api/types'
import { passagesForVerify, verifyAnswer } from '../api/verify'
import { Card } from '../components/Card'
import { Chip } from '../components/Chip'
import { EmptyState } from '../components/EmptyState'
import { ErrorState } from '../components/ErrorState'
import { EvidenceView } from '../components/EvidenceView'
import { HowItWorks } from '../components/HowItWorks'
import { OverallVerdict } from '../components/OverallVerdict'
import { PageHeader } from '../components/PageHeader'
import { SentenceList } from '../components/SentenceList'
import { VerdictChip } from '../components/VerdictChip'
import { ROUTES, routeMeta } from '../routes'
import { useBootstrap } from '../state/bootstrap'

const meta = routeMeta('/verify')
const step = String(ROUTES.indexOf(meta) + 1).padStart(2, '0')

const HOW_ARCANUM = ['POST /api/v1/context', 'POST /api/v1/verify']
const HOW_DEMO = ['GET /demo/samples']

/** Tour hooks: Task 19 wires these to the tour. */
const onVerified = (): void => {}
const onFlawedChecked = (): void => {}

/** The picker value for "write my own answer". */
const CUSTOM = ''

interface Built {
  question: string
  candidateK: number
  data: ContextResponse
}

interface Outcome {
  response: VerifyResponse
  /** The answer that was sent: the sentence spans index exactly this text. */
  answer: string
  /** Set when the answer was the unedited prepared one, so `expected` applies. */
  prepared: FlawedAnswer | null
}

interface Failure {
  title: string
  message: string
}

function describeVerifyError(e: unknown): Failure {
  if (e instanceof ApiError) {
    if (e.status === 503) return { title: 'Verification is not available', message: e.message }
    if (e.status === 502 || e.status === 504) return { title: 'The judge failed', message: e.message }
    if (e.status === 400) return { title: 'The server rejected this request', message: e.message }
    return { title: 'Verification failed', message: e.message }
  }
  return { title: 'Verification failed', message: e instanceof Error ? e.message : String(e) }
}

function ExpectedNote({ prepared, response }: { prepared: FlawedAnswer; response: VerifyResponse }) {
  const rows = response.sentences.map((s, i) => ({ got: s.verdict, want: prepared.expected[i] }))
  const mismatches = rows.filter((r) => r.want !== undefined && r.want !== r.got).length
  const lengthDiffers = prepared.expected.length !== response.sentences.length
  return (
    <section aria-label="Expected verdicts" className="rounded-card border border-border bg-surface p-4">
      <h3 className="mb-2 text-sm font-semibold">Expected verdicts</h3>
      <p className="mb-3 text-xs text-muted">
        This answer was written to show specific verdicts. A mismatch is information, not an error: a different judge model can read a sentence differently, and small models are less steady.
      </p>
      <ol className="space-y-1.5">
        {rows.map((r, i) => (
          <li key={i} className="flex flex-wrap items-center gap-2 text-sm">
            <span className="w-16 text-xs text-muted">sentence {i + 1}</span>
            <span className="font-mono text-xs">{r.want === undefined ? `no expectation, got ${r.got}` : `expected ${r.want}, got ${r.got}`}</span>
            {r.want !== undefined &&
              (r.want === r.got ? (
                <Chip tone="supported" icon={<Check className="h-3 w-3" aria-hidden="true" />}>
                  match
                </Chip>
              ) : (
                <Chip tone="accent" icon={<Info className="h-3 w-3" aria-hidden="true" />}>
                  mismatch
                </Chip>
              ))}
          </li>
        ))}
      </ol>
      {(lengthDiffers || mismatches > 0) && (
        <p className="mt-3 text-xs text-muted">
          {lengthDiffers
            ? `The judge split the answer into ${response.sentences.length} sentences; the prepared list has ${prepared.expected.length}. `
            : ''}
          {mismatches > 0 ? `${mismatches} of ${rows.length} sentences differ from the prepared verdicts.` : ''}
        </p>
      )}
    </section>
  )
}

export default function VerifyPage() {
  const { data: boot, client } = useBootstrap()
  const samples = useSamples()
  const flawed = samples.data?.flawed_answers ?? []

  const [sampleId, setSampleId] = useState<string | null>(null)
  const [question, setQuestion] = useState('')
  const [answer, setAnswer] = useState('')
  const [candidateK, setCandidateK] = useState(50)
  const [strict, setStrict] = useState(false)
  const [judge, setJudge] = useState<string | null>(null)
  const [built, setBuilt] = useState<Built | null>(null)
  const [outcome, setOutcome] = useState<Outcome | null>(null)
  const [selected, setSelected] = useState<number | null>(null)
  const [raw, setRaw] = useState(false)
  const [failure, setFailure] = useState<Failure | null>(null)
  const [busy, setBusy] = useState<'passages' | 'verify' | null>(null)
  const seq = useRef(0)

  const choose = (id: string, list: FlawedAnswer[]) => {
    setSampleId(id)
    setOutcome(null)
    setFailure(null)
    setSelected(null)
    const s = list.find((x) => x.id === id)
    if (s) {
      setQuestion(s.question)
      setAnswer(s.answer)
    }
  }

  // Start on the first prepared answer once the samples arrive.
  useEffect(() => {
    if (sampleId === null && flawed.length > 0) choose(flawed[0].id, flawed)
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [flawed.length])

  if (!boot) return null

  const sample = flawed.find((s) => s.id === sampleId) ?? null
  const defaultJudge = boot.judge ?? ''
  const chosenJudge = judge ?? defaultJudge
  const unavailable = !boot.features.verify || !boot.judge
  const ready = answer.trim() !== '' && question.trim() !== '' && !unavailable

  const contextRequest = (): ContextRequest => ({
    collection_id: boot.collection,
    query: question.trim(),
    token_budget: 4000,
    background_share: 0.2,
    candidate_k: candidateK,
    render: 'xml',
  })

  const build = async (): Promise<Built> => {
    const data = await buildContext(client, contextRequest())
    const next = { question: question.trim(), candidateK, data }
    setBuilt(next)
    return next
  }

  const rebuild = async () => {
    const mine = ++seq.current
    setBusy('passages')
    setFailure(null)
    try {
      await build()
    } catch (e) {
      if (seq.current === mine) setFailure({ title: 'Could not build passages', message: e instanceof Error ? e.message : String(e) })
    } finally {
      if (seq.current === mine) setBusy(null)
    }
  }

  const run = async () => {
    const mine = ++seq.current
    setFailure(null)
    setOutcome(null)
    setSelected(null)
    setRaw(false)
    let stage: 'passages' | 'verify' = 'passages'
    try {
      setBusy('passages')
      const reuse = built && built.question === question.trim() && built.candidateK === candidateK
      const ctx = reuse ? built : await build()
      const passages = passagesForVerify(ctx.data)
      stage = 'verify'
      if (seq.current !== mine) return
      setBusy('verify')
      const req: VerifyRequest = { collection_id: boot.collection, answer, passages }
      if (chosenJudge && chosenJudge !== defaultJudge) req.judge = chosenJudge
      if (strict) req.strict_citations = true
      const response = await verifyAnswer(client, req)
      if (seq.current !== mine) return
      const prepared = sample && sample.answer === answer ? sample : null
      setOutcome({ response, answer, prepared })
      setSelected(response.sentences.findIndex((s) => s.verdict !== 'supported' && s.verdict !== 'no_claim'))
      onVerified()
      if (prepared && prepared.expected.some((v) => v !== 'supported')) onFlawedChecked()
    } catch (e) {
      if (seq.current !== mine) return
      setFailure(stage === 'passages' ? { title: 'Could not build passages', message: e instanceof Error ? e.message : String(e) } : describeVerifyError(e))
    } finally {
      if (seq.current === mine) setBusy(null)
    }
  }

  const sel = outcome && selected !== null && selected >= 0 ? outcome.response.sentences[selected] : undefined
  const field = 'w-full rounded-lg border border-border bg-surface px-3 text-sm outline-none focus:border-accent'

  return (
    <>
      <PageHeader eyebrow={`${step} / ${meta.label}`} title={meta.label} description={meta.blurb} actions={<HowItWorks arcanum={HOW_ARCANUM} demo={HOW_DEMO} />} />

      {unavailable && (
        <div className="mb-6">
          <ErrorState
            title="Verification is not available"
            message="This server has no judge configured, so it cannot verify answers. Context and Search still work."
            fix="Set [verify] judge in config.toml to a generator name, then restart Atlas."
          />
        </div>
      )}

      <div className="mb-8 grid gap-6 lg:grid-cols-[minmax(0,1fr)_340px]">
        <Card className="space-y-4 p-4">
          <div>
            <label htmlFor="vf-sample" className="mb-1 block text-xs font-medium text-muted">
              Prepared answer
            </label>
            <select
              id="vf-sample"
              value={sampleId ?? CUSTOM}
              onChange={(e) => choose(e.target.value, flawed)}
              className={`${field} h-10`}
            >
              <option value={CUSTOM}>Write my own answer</option>
              {flawed.map((s) => (
                <option key={s.id} value={s.id}>
                  {s.title}
                </option>
              ))}
            </select>
            {sample && (
              <p className="mt-2 flex flex-wrap items-center gap-1.5 text-xs text-muted">
                Designed to show:
                {Array.from(new Set(sample.expected)).map((v) => (
                  <VerdictChip key={v} verdict={v} />
                ))}
              </p>
            )}
            {samples.isError && <p className="mt-2 text-xs text-vink-partial">The prepared answers could not be loaded, but you can still paste your own.</p>}
          </div>

          <div>
            <label htmlFor="vf-question" className="mb-1 block text-xs font-medium text-muted">
              Question (used to build the passages)
            </label>
            <input id="vf-question" value={question} onChange={(e) => setQuestion(e.target.value)} className={`${field} h-10`} />
          </div>

          <div>
            <label htmlFor="vf-answer" className="mb-1 block text-xs font-medium text-muted">
              Answer
            </label>
            <textarea id="vf-answer" value={answer} rows={6} onChange={(e) => setAnswer(e.target.value)} className={`${field} py-2 font-mono text-[13px] leading-6`} />
            <p className="mt-1 text-[11px] text-muted">Cite passages as [P1], [P2]. Edit freely: your edits are verified as written.</p>
          </div>

          <button
            type="button"
            onClick={() => void run()}
            disabled={!ready || busy !== null}
            className="inline-flex h-10 items-center gap-2 rounded-lg bg-accent px-4 text-sm font-medium text-accent-fg transition hover:opacity-90 disabled:opacity-50"
          >
            <ShieldCheck className="h-4 w-4" aria-hidden="true" />
            {busy === 'passages' ? 'Building passages...' : busy === 'verify' ? 'Judging...' : 'Run verification'}
          </button>
        </Card>

        <Card className="space-y-4 p-4">
          <div>
            <label htmlFor="vf-judge" className="mb-1 block text-xs font-medium text-muted">
              Judge
            </label>
            <select id="vf-judge" value={chosenJudge} onChange={(e) => setJudge(e.target.value)} className={`${field} h-9`}>
              {boot.generators.map((g) => (
                <option key={g.name} value={g.name}>
                  {g.name} ({g.model}){g.name === defaultJudge ? ' - default' : ''}
                </option>
              ))}
            </select>
            <p className="mt-1 text-[11px] text-muted">Only sent when it differs from the server default.</p>
          </div>
          <label className="flex items-start gap-2 text-sm">
            <input type="checkbox" checked={strict} onChange={(e) => setStrict(e.target.checked)} className="mt-1 accent-[rgb(var(--accent))]" />
            <span>
              <span className="font-mono text-xs">strict_citations</span>
              <span className="mt-0.5 block text-[11px] text-muted">Also fail the verdict on miscited and uncited sentences.</span>
            </span>
          </label>
          <div>
            <div className="mb-1 flex items-baseline justify-between text-xs">
              <label htmlFor="vf-k" className="font-medium text-muted">
                candidate_k
              </label>
              <span className="font-mono">{candidateK}</span>
            </div>
            <input
              id="vf-k"
              type="range"
              min={1}
              max={200}
              step={1}
              value={candidateK}
              onChange={(e) => setCandidateK(Number(e.target.value))}
              className="w-full accent-[rgb(var(--accent))]"
            />
            <p className="mt-0.5 text-[11px] text-muted">Candidates per strategy when building the passages. Raise it if a document you need is missing.</p>
          </div>
        </Card>
      </div>

      <section aria-labelledby="vf-passages" className="mb-8">
        <div className="mb-2 flex flex-wrap items-center gap-3">
          <h2 id="vf-passages" className="text-sm font-semibold">
            Passages used {built && <span className="font-mono font-normal text-muted">({built.data.passages.length})</span>}
          </h2>
          <button
            type="button"
            onClick={() => void rebuild()}
            disabled={question.trim() === '' || busy !== null || unavailable}
            className="inline-flex h-7 items-center gap-1.5 rounded-lg border border-border px-2.5 text-xs text-muted transition hover:text-text disabled:opacity-50"
          >
            <RefreshCw className="h-3.5 w-3.5" aria-hidden="true" />
            Rebuild passages
          </button>
        </div>
        {sample?.expected.includes('miscited') && (
          <p className="mb-2 flex items-start gap-1.5 text-xs text-muted">
            <Info className="mt-px h-3.5 w-3.5 shrink-0" aria-hidden="true" />
            The miscited sentence needs the HX-1 datasheet among the passages, and not as P1. If it is missing below, raise candidate_k and rebuild.
          </p>
        )}
        {!built ? (
          <p className="text-sm text-muted">Run verification to build them from the question with the Context defaults, or rebuild them here first.</p>
        ) : built.data.passages.length === 0 ? (
          <p className="text-sm text-vink-partial">No passages were found for this question. Without passages every sentence is unsupported. Is the corpus loaded?</p>
        ) : (
          <ul className="grid gap-2 md:grid-cols-2">
            {built.data.passages.map((p) => (
              <li key={p.ref_id} className="flex flex-wrap items-center gap-2 rounded-lg border border-border bg-surface px-3 py-2 text-xs">
                <Chip tone="accent" mono>
                  {p.ref_id}
                </Chip>
                <span className="min-w-0 break-all font-medium">{p.source_uri}</span>
                <span className="font-mono text-[11px] text-muted">
                  v{p.version_num} · bytes {p.offset_start}-{p.offset_end} · {p.chunk_ids.length} {p.chunk_ids.length === 1 ? 'chunk' : 'chunks'}
                </span>
              </li>
            ))}
          </ul>
        )}
      </section>

      <div aria-live="polite" className="space-y-6">
        {failure && !busy && <ErrorState title={failure.title} message={failure.message} />}
        {!outcome && !failure && !busy && (
          <EmptyState
            icon={Layers}
            title="Check an answer against its passages"
            description="Each sentence gets a verdict: supported, partial, unsupported, miscited, supported but uncited, or no claim. Every claim points to the exact bytes of the stored source."
          />
        )}
        {outcome && (
          <>
            <OverallVerdict result={outcome.response} />
            <Card className="space-y-3 p-5">
              <h2 className="text-xs font-semibold uppercase tracking-wider text-muted">Verified answer</h2>
              <SentenceList answer={outcome.answer} sentences={outcome.response.sentences} selected={selected} onSelect={setSelected} />
              <p className="text-xs text-muted">Select a sentence to see its claims and the source text behind them.</p>
            </Card>
            {sel && <EvidenceView sentence={sel} />}
            {outcome.prepared && <ExpectedNote prepared={outcome.prepared} response={outcome.response} />}
            <div>
              <button
                type="button"
                aria-expanded={raw}
                onClick={() => setRaw((r) => !r)}
                className="inline-flex h-8 items-center gap-1.5 rounded-lg border border-border px-3 text-xs text-muted transition hover:text-text"
              >
                {raw ? 'Hide raw JSON' : 'Show raw JSON'}
              </button>
              {raw && (
                <pre className="mt-2 max-h-96 overflow-auto rounded-lg border border-border bg-surface-2 p-3 font-mono text-[11px] leading-5">
                  {JSON.stringify(outcome.response, null, 2)}
                </pre>
              )}
            </div>
          </>
        )}
      </div>
    </>
  )
}
