import { describe, expect, it, vi } from 'vitest'
import { act, renderHook, waitFor } from '@testing-library/react'
import { ApiError } from '../api/client'
import type { AskEvent } from '../api/generate'
import type { GenerateRequest } from '../api/types'
import { useAsk } from './ask'

vi.mock('./bootstrap', () => ({
  useBootstrap: () => ({ client: {} }),
}))

const REQ: GenerateRequest = { collection_id: 'halcyon', query: 'q' }
const CONTEXT = { resolved_query: 'q', passages: [], background: [] }
const DONE = { status: 'ok', citations: [], unknown_refs: [], stop_reason: 'end_turn', usage: { input_tokens: 1, output_tokens: 2 }, generator: { name: 'g', model: 'm' } }

type Stream = (client: unknown, req: GenerateRequest, signal: AbortSignal) => AsyncGenerator<AskEvent>

/** A stream that emits its events and then waits for the signal, like an open connection. */
function openStream(events: AskEvent[], log?: AbortSignal[]): Stream {
  return async function* (_c, _r, signal) {
    log?.push(signal)
    for (const e of events) yield e
    await new Promise<void>((_, reject) => signal.addEventListener('abort', () => reject(new DOMException('aborted', 'AbortError'))))
  }
}

describe('useAsk', () => {
  it('ask_hook_aborts_cleanly', async () => {
    const log: AbortSignal[] = []
    const stream = openStream([{ type: 'context', context: CONTEXT as never }, { type: 'delta', text: 'partial' }], log)
    const { result } = renderHook(() => useAsk({ stream: stream as never }))
    act(() => result.current.start(REQ))
    await waitFor(() => expect(result.current.state.answer).toBe('partial'))
    expect(result.current.state.phase).toBe('streaming')
    act(() => result.current.stop())
    expect(log[0].aborted).toBe(true)
    await waitFor(() => expect(result.current.state.phase).toBe('idle'))
    expect(result.current.state.error).toBeNull()
    expect(result.current.state.stopped).toBe(true)
  })

  it('unmount_aborts_the_request', async () => {
    const log: AbortSignal[] = []
    const { result, unmount } = renderHook(() => useAsk({ stream: openStream([], log) as never }))
    act(() => result.current.start(REQ))
    unmount()
    expect(log[0].aborted).toBe(true)
  })

  it('new_start_aborts_the_previous_run', async () => {
    const log: AbortSignal[] = []
    const first = openStream([{ type: 'delta', text: 'old' }], log)
    const second: Stream = async function* (_c, _r, signal) {
      log.push(signal)
      yield { type: 'delta', text: 'new' }
      yield { type: 'done', outcome: DONE as never }
    }
    let n = 0
    const stream: Stream = (c, r, s) => (n++ === 0 ? first(c, r, s) : second(c, r, s))
    const { result } = renderHook(() => useAsk({ stream: stream as never }))
    act(() => result.current.start(REQ))
    await waitFor(() => expect(result.current.state.answer).toBe('old'))
    act(() => result.current.start(REQ))
    expect(log[0].aborted).toBe(true)
    await waitFor(() => expect(result.current.state.phase).toBe('done'))
    expect(result.current.state.answer).toBe('new')
    expect(result.current.state.error).toBeNull()
  })

  it('collects_events_into_done_state_with_timings_and_verification', async () => {
    const stream: Stream = async function* () {
      yield { type: 'context', context: CONTEXT as never }
      yield { type: 'delta', text: 'A' }
      yield { type: 'delta', text: 'B' }
      yield { type: 'done', outcome: DONE as never }
      yield { type: 'verification', verification: null }
    }
    const { result } = renderHook(() => useAsk({ stream: stream as never }))
    act(() => result.current.start(REQ))
    await waitFor(() => expect(result.current.state.phase).toBe('done'))
    const s = result.current.state
    expect(s.answer).toBe('AB')
    expect(s.context).not.toBeNull()
    expect(s.outcome?.status).toBe('ok')
    expect(s.verification).toBeNull()
    expect(s.ttft).not.toBeNull()
    expect(s.elapsed).not.toBeNull()
  })

  it('pre_stream_api_error_ends_in_error_with_status', async () => {
    const stream: Stream = async function* () {
      throw new ApiError(503, 'verification requires a configured judge')
      yield { type: 'delta', text: '' }
    }
    const { result } = renderHook(() => useAsk({ stream: stream as never }))
    act(() => result.current.start(REQ))
    await waitFor(() => expect(result.current.state.phase).toBe('error'))
    expect(result.current.state.error).toBe('verification requires a configured judge')
    expect(result.current.state.errorStatus).toBe(503)
  })

  it('error_event_keeps_the_partial_answer', async () => {
    const stream: Stream = async function* () {
      yield { type: 'delta', text: 'half' }
      yield { type: 'error', message: 'generation failed' }
    }
    const { result } = renderHook(() => useAsk({ stream: stream as never }))
    act(() => result.current.start(REQ))
    await waitFor(() => expect(result.current.state.phase).toBe('error'))
    expect(result.current.state.answer).toBe('half')
    expect(result.current.state.errorStatus).toBeNull()
  })

  it('a_stream_that_ends_without_done_is_an_error', async () => {
    const stream: Stream = async function* () {
      yield { type: 'delta', text: 'half' }
    }
    const { result } = renderHook(() => useAsk({ stream: stream as never }))
    act(() => result.current.start(REQ))
    await waitFor(() => expect(result.current.state.phase).toBe('error'))
    expect(result.current.state.error).toMatch(/ended/i)
  })

  it('reset_returns_to_idle_and_aborts', async () => {
    const log: AbortSignal[] = []
    const { result } = renderHook(() => useAsk({ stream: openStream([{ type: 'delta', text: 'x' }], log) as never }))
    act(() => result.current.start(REQ))
    await waitFor(() => expect(result.current.state.answer).toBe('x'))
    act(() => result.current.reset())
    expect(log[0].aborted).toBe(true)
    expect(result.current.state.phase).toBe('idle')
    expect(result.current.state.answer).toBe('')
    expect(result.current.state.stopped).toBe(false)
  })
})
