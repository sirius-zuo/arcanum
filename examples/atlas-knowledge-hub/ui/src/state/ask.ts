import { useCallback, useEffect, useRef, useState } from 'react'
import { ApiError } from '../api/client'
import { streamAnswer } from '../api/generate'
import type { AskEvent } from '../api/generate'
import type { Client } from '../api/client'
import type { ContextResponse, GenerateDone, GenerateRequest, VerificationOutcome } from '../api/types'
import { useBootstrap } from './bootstrap'

export type AskPhase = 'idle' | 'streaming' | 'done' | 'error'

export interface AskState {
  phase: AskPhase
  answer: string
  context: ContextResponse | null
  outcome: GenerateDone | null
  /** undefined: not arrived (or not requested); null: arrived with nothing to verify. */
  verification: VerificationOutcome | null | undefined
  /** Milliseconds from start to the first delta. */
  ttft: number | null
  /** Milliseconds from start to the end of the answer (live while it streams). */
  elapsed: number | null
  error: string | null
  /** HTTP status when the failure came before the stream started. */
  errorStatus: number | null
  /** True when the user stopped the run: phase is idle and the partial answer is kept. */
  stopped: boolean
}

export const INITIAL_ASK: AskState = {
  phase: 'idle',
  answer: '',
  context: null,
  outcome: null,
  verification: undefined,
  ttft: null,
  elapsed: null,
  error: null,
  errorStatus: null,
  stopped: false,
}

type Stream = (client: Client, req: GenerateRequest, signal: AbortSignal) => AsyncGenerator<AskEvent>

export interface UseAsk {
  state: AskState
  start: (req: GenerateRequest) => void
  stop: () => void
  reset: () => void
}

/** One streaming run at a time. Stop, a new start and unmount all abort the request. */
export function useAsk(opts: { stream?: Stream } = {}): UseAsk {
  const { client } = useBootstrap()
  const stream = opts.stream ?? streamAnswer
  const [state, setState] = useState<AskState>(INITIAL_ASK)
  const ctrl = useRef<AbortController | null>(null)
  const t0 = useRef(0)

  const abort = useCallback(() => {
    ctrl.current?.abort()
    ctrl.current = null
  }, [])

  useEffect(() => abort, [abort])

  const answering = state.phase === 'streaming' && state.outcome === null
  useEffect(() => {
    if (!answering) return
    const id = window.setInterval(() => setState((s) => (s.phase === 'streaming' && s.outcome === null ? { ...s, elapsed: performance.now() - t0.current } : s)), 100)
    return () => window.clearInterval(id)
  }, [answering])

  const start = useCallback(
    (req: GenerateRequest) => {
      abort()
      const mine = new AbortController()
      ctrl.current = mine
      t0.current = performance.now()
      setState({ ...INITIAL_ASK, phase: 'streaming', elapsed: 0 })
      const live = () => ctrl.current === mine && !mine.signal.aborted
      const apply = (fn: (s: AskState) => AskState) => {
        if (live()) setState(fn)
      }

      void (async () => {
        let finished = false
        try {
          for await (const ev of stream(client, req, mine.signal)) {
            if (!live()) return
            switch (ev.type) {
              case 'context':
                apply((s) => ({ ...s, context: ev.context }))
                break
              case 'delta': {
                const now = performance.now() - t0.current
                apply((s) => ({ ...s, answer: s.answer + ev.text, ttft: s.ttft ?? now, elapsed: now }))
                break
              }
              case 'done': {
                const now = performance.now() - t0.current
                apply((s) => ({ ...s, outcome: ev.outcome, elapsed: now }))
                finished = true
                break
              }
              case 'verification':
                apply((s) => ({ ...s, verification: ev.verification }))
                break
              case 'error':
                apply((s) => ({ ...s, phase: 'error', error: ev.message }))
                finished = true
                return
            }
          }
          apply((s) =>
            finished
              ? { ...s, phase: 'done' }
              : { ...s, phase: 'error', error: 'The stream ended before the answer was complete.' },
          )
        } catch (e) {
          if (!live()) return
          const status = e instanceof ApiError ? e.status : null
          apply((s) => ({ ...s, phase: 'error', error: e instanceof Error ? e.message : String(e), errorStatus: status === 0 ? null : status }))
        } finally {
          // Close the connection even when the loop left early (an error event).
          if (ctrl.current === mine) ctrl.current = null
          mine.abort()
        }
      })()
    },
    [abort, client, stream],
  )

  const stop = useCallback(() => {
    if (!ctrl.current) return
    abort()
    setState((s) => (s.phase === 'streaming' ? { ...s, phase: 'idle', stopped: true } : s))
  }, [abort])

  const reset = useCallback(() => {
    abort()
    setState(INITIAL_ASK)
  }, [abort])

  return { state, start, stop, reset }
}

/** The server's message when this turn asked for verification and no judge is configured (HTTP 503 before generation). */
export function verifyUnavailableMessage(state: AskState, verifyRequested: boolean): string | null {
  if (!verifyRequested || state.phase !== 'error' || state.errorStatus !== 503 || state.error === null) return null
  return /verif|judge/i.test(state.error) ? state.error : null
}
