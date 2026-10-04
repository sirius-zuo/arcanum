import { MessageSquareText } from 'lucide-react'
import { useEffect, useRef, useState } from 'react'
import type { GenerateRequest } from '../api/types'
import { AskForm } from '../components/AskForm'
import type { AskSubmit } from '../components/AskForm'
import { EmptyState } from '../components/EmptyState'
import { ErrorState } from '../components/ErrorState'
import { HowItWorks } from '../components/HowItWorks'
import { PageHeader } from '../components/PageHeader'
import { TurnCard } from '../components/TurnCard'
import type { TurnMeta } from '../components/TurnCard'
import { ROUTES, routeMeta } from '../routes'
import { useAsk, verifyUnavailableMessage } from '../state/ask'
import type { AskState } from '../state/ask'
import { useBootstrap } from '../state/bootstrap'
import { useTourSignal } from '../state/tour'

const meta = routeMeta('/ask')
const step = String(ROUTES.indexOf(meta) + 1).padStart(2, '0')

const HOW_ARCANUM = ['POST /api/v1/generate']
const HOW_DEMO: string[] = []

interface PastTurn {
  id: number
  meta: TurnMeta
  state: AskState
}

export default function AskPage() {
  const { data: boot } = useBootstrap()
  const ask = useAsk()
  const signal = useTourSignal()
  const [past, setPast] = useState<PastTurn[]>([])
  const [current, setCurrent] = useState<{ id: number; meta: TurnMeta } | null>(null)
  const [verifyBlocked, setVerifyBlocked] = useState<string | null>(null)
  const nextId = useRef(1)
  const latest = useRef<HTMLDivElement>(null)
  const announced = useRef(0)

  const busy = ask.state.phase === 'streaming'

  useEffect(() => {
    if (!current || !current.meta.verify) return
    const message = verifyUnavailableMessage(ask.state, true)
    if (message) setVerifyBlocked(message)
    if (ask.state.verification && announced.current !== current.id) {
      announced.current = current.id
      signal('verified')
    }
  }, [ask.state, current, signal])

  const doneId = ask.state.phase === 'done' ? current?.id : undefined
  useEffect(() => {
    if (doneId !== undefined) signal('asked')
  }, [doneId, signal])

  const currentId = current?.id
  useEffect(() => {
    if (currentId !== undefined) latest.current?.scrollIntoView?.({ behavior: 'smooth', block: 'start' })
  }, [currentId])

  if (!boot) return null

  const submit = (v: AskSubmit) => {
    if (current) setPast((p) => [...p, { ...current, state: ask.state }])
    const req: GenerateRequest = {
      collection_id: boot.collection,
      mode: v.mode,
      query: v.question,
      generator: v.generator,
      verify: v.verify || undefined,
    }
    setCurrent({ id: nextId.current++, meta: { question: v.question, mode: v.mode, generator: v.generator, verify: v.verify } })
    ask.start(req)
  }

  const generateOff = !boot.features.generate

  return (
    <>
      <PageHeader
        eyebrow={`${step} / ${meta.label}`}
        title={meta.label}
        description={meta.blurb}
        actions={<HowItWorks arcanum={HOW_ARCANUM} demo={HOW_DEMO} />}
      />

      {generateOff ? (
        <ErrorState
          title="Generation is not available"
          message="This server has no generator or no chunk registry, so it cannot answer questions. Search and Context still work."
          fix="ollama serve && ollama pull qwen2.5   # then restart Atlas"
        />
      ) : (
        <div className="flex min-h-[calc(100vh-14rem)] flex-col">
          <div className="flex-1 space-y-10 pb-6" aria-live="off">
            {!current && (
              <EmptyState
                icon={MessageSquareText}
                title="Ask the Halcyon corpus"
                description="Answers stream in with clickable citations. Click a citation to jump to the passage it came from, down to the byte range in the source document."
              />
            )}
            {past.map((t) => (
              <TurnCard key={t.id} meta={t.meta} state={t.state} />
            ))}
            {current && (
              <div ref={latest} className="scroll-mt-24">
                <TurnCard key={current.id} meta={current.meta} state={ask.state} />
              </div>
            )}
          </div>
          <div className="sticky bottom-4 z-20">
            <AskForm boot={boot} busy={busy} onSubmit={submit} onStop={ask.stop} verifyDisabledReason={verifyBlocked} />
          </div>
        </div>
      )}
    </>
  )
}
