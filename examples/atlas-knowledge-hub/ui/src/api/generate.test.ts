import { describe, expect, it } from 'vitest'
import { ApiError } from './client'
import type { Client } from './client'
import { streamAnswer } from './generate'
import type { AskEvent } from './generate'
import type { GenerateRequest } from './types'

const REQ: GenerateRequest = { collection_id: 'halcyon', query: 'q' }

function sse(...events: [string, unknown][]): string {
  return events.map(([name, data]) => `event: ${name}\ndata: ${typeof data === 'string' ? data : JSON.stringify(data)}\n\n`).join('')
}

function clientFor(res: Response, seen?: { path?: string; init?: RequestInit }): Client {
  return {
    get: async () => {
      throw new Error('unused')
    },
    post: async () => {
      throw new Error('unused')
    },
    del: async () => {},
    raw: async (path, init) => {
      if (seen) {
        seen.path = path
        seen.init = init
      }
      return res
    },
  }
}

async function collect(gen: AsyncGenerator<AskEvent>): Promise<AskEvent[]> {
  const out: AskEvent[] = []
  for await (const e of gen) out.push(e)
  return out
}

const CONTEXT = { resolved_query: 'q', passages: [{ ref_id: 'P1' }], usage: { budget: 1 } }
const DONE = { status: 'ok', citations: [], unknown_refs: [], stop_reason: 'end_turn', usage: { input_tokens: 1, output_tokens: 2 }, generator: { name: 'g', model: 'm' } }

describe('streamAnswer', () => {
  it('stream_answer_yields_typed_events_in_order', async () => {
    const seen: { path?: string; init?: RequestInit } = {}
    const body = sse(
      ['context', CONTEXT],
      ['delta', { text: 'Hello ' }],
      ['delta', { text: 'world [P1].' }],
      ['done', DONE],
      ['verification', { status: 'error', code: 'judge_timeout', message: 'slow' }],
    )
    const events = await collect(streamAnswer(clientFor(new Response(body), seen), REQ, new AbortController().signal))
    expect(events.map((e) => e.type)).toEqual(['context', 'delta', 'delta', 'done', 'verification'])
    expect(events[0]).toMatchObject({ type: 'context', context: { resolved_query: 'q' } })
    expect(events[1]).toEqual({ type: 'delta', text: 'Hello ' })
    expect(events[3]).toMatchObject({ type: 'done', outcome: { status: 'ok', stop_reason: 'end_turn' } })
    expect(events[4]).toMatchObject({ type: 'verification', verification: { status: 'error', code: 'judge_timeout' } })
    expect(seen.path).toBe('/api/v1/generate')
    expect(JSON.parse(String(seen.init?.body))).toMatchObject({ collection_id: 'halcyon', query: 'q', stream: true })
  })

  it('error_event_after_start_is_yielded_not_thrown', async () => {
    const body = sse(['context', CONTEXT], ['delta', { text: 'part' }], ['error', { error: 'generation failed' }])
    const events = await collect(streamAnswer(clientFor(new Response(body)), REQ, new AbortController().signal))
    expect(events.map((e) => e.type)).toEqual(['context', 'delta', 'error'])
    expect(events[2]).toEqual({ type: 'error', message: 'generation failed' })
  })

  it('pre_stream_json_error_throws_api_error_with_message', async () => {
    const res = new Response(JSON.stringify({ error: 'verification requires a configured judge' }), { status: 503 })
    const err = await collect(streamAnswer(clientFor(res), REQ, new AbortController().signal)).catch((e: unknown) => e)
    expect(err).toBeInstanceOf(ApiError)
    expect((err as ApiError).status).toBe(503)
    expect((err as ApiError).message).toBe('verification requires a configured judge')
  })

  it('null_verification_event_is_passed_through', async () => {
    const body = sse(['context', CONTEXT], ['delta', { text: 'No relevant information.' }], ['done', { ...DONE, status: 'no_context' }], ['verification', 'null'])
    const events = await collect(streamAnswer(clientFor(new Response(body)), REQ, new AbortController().signal))
    expect(events[events.length - 1]).toEqual({ type: 'verification', verification: null })
  })

  it('malformed_json_event_becomes_an_error_event', async () => {
    const body = sse(['context', CONTEXT], ['delta', '{"text": "oops'], ['delta', { text: 'never seen' }])
    const events = await collect(streamAnswer(clientFor(new Response(body)), REQ, new AbortController().signal))
    expect(events.map((e) => e.type)).toEqual(['context', 'error'])
  })

  it('delta_without_text_becomes_an_error_event', async () => {
    const body = sse(['delta', { nope: 1 }])
    const events = await collect(streamAnswer(clientFor(new Response(body)), REQ, new AbortController().signal))
    expect(events.map((e) => e.type)).toEqual(['error'])
  })

  it('unknown_events_are_ignored', async () => {
    const body = sse(['ping', { a: 1 }], ['delta', { text: 'a' }], ['future', 'not json'])
    const events = await collect(streamAnswer(clientFor(new Response(body)), REQ, new AbortController().signal))
    expect(events).toEqual([{ type: 'delta', text: 'a' }])
  })

  it('passes_the_abort_signal_to_the_request', async () => {
    const seen: { path?: string; init?: RequestInit } = {}
    const ctrl = new AbortController()
    await collect(streamAnswer(clientFor(new Response(''), seen), REQ, ctrl.signal))
    expect(seen.init?.signal).toBe(ctrl.signal)
  })
})
