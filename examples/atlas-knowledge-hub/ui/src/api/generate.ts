import type { Client } from './client'
import { streamPost } from './sse'
import type { ContextResponse, GenerateDone, GenerateRequest, VerificationOutcome } from './types'

export type AskEvent =
  | { type: 'context'; context: ContextResponse }
  | { type: 'delta'; text: string }
  | { type: 'done'; outcome: GenerateDone }
  | { type: 'verification'; verification: VerificationOutcome | null }
  | { type: 'error'; message: string }

type Parsed = { ok: true; value: unknown } | { ok: false }

function parse(data: string): Parsed {
  try {
    return { ok: true, value: JSON.parse(data) as unknown }
  } catch {
    return { ok: false }
  }
}

function isRecord(v: unknown): v is Record<string, unknown> {
  return typeof v === 'object' && v !== null && !Array.isArray(v)
}

function malformed(name: string): AskEvent {
  return { type: 'error', message: `The server sent a malformed ${name} event.` }
}

/**
 * POST /api/v1/generate with `stream: true` and yield typed events. Payloads
 * follow the server (`sse_response` in arcanum-server): `context`, `delta`
 * `{text}`, `done` (the outcome without answer and context), `verification`
 * (an object or null) and `error` `{error}`. Unknown events are ignored.
 *
 * Failures before the first event are ordinary JSON errors and surface as a
 * thrown ApiError. Always pass a signal and abort it when you stop consuming:
 * leaving the generator early does not close the connection.
 */
export async function* streamAnswer(client: Client, req: GenerateRequest, signal: AbortSignal): AsyncGenerator<AskEvent> {
  for await (const ev of streamPost(client, '/api/v1/generate', { ...req, stream: true }, signal)) {
    if (ev.event !== 'context' && ev.event !== 'delta' && ev.event !== 'done' && ev.event !== 'verification' && ev.event !== 'error') continue
    const parsed = parse(ev.data)
    if (!parsed.ok) {
      yield malformed(ev.event)
      return
    }
    const v = parsed.value
    switch (ev.event) {
      case 'context':
        if (!isRecord(v)) return yield malformed('context')
        yield { type: 'context', context: v as unknown as ContextResponse }
        break
      case 'delta':
        if (!isRecord(v) || typeof v.text !== 'string') return yield malformed('delta')
        yield { type: 'delta', text: v.text }
        break
      case 'done':
        if (!isRecord(v)) return yield malformed('done')
        yield { type: 'done', outcome: v as unknown as GenerateDone }
        break
      case 'verification':
        if (v !== null && !isRecord(v)) return yield malformed('verification')
        yield { type: 'verification', verification: v as VerificationOutcome | null }
        break
      case 'error':
        yield { type: 'error', message: isRecord(v) && typeof v.error === 'string' ? v.error : 'The generation failed.' }
        return
    }
  }
}
