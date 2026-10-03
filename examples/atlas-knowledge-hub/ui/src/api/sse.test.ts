import { describe, expect, it } from 'vitest'
import type { Client } from './client'
import { ApiError } from './client'
import { SseParser, streamPost, type SseEvent } from './sse'

const FIXTURE =
  ': keep-alive comment\n' +
  'event: token\n' +
  'data: {"t":"Café"}\n' +
  '\n' +
  'data: line one\n' +
  'data:line two\n' +
  'data:  spaced\n' +
  '\n' +
  'id: 7\n' +
  'retry: 100\n' +
  'event: done\n' +
  'data: 日本語 😀\n' +
  '\n'

const EXPECTED: SseEvent[] = [
  { event: 'token', data: '{"t":"Café"}' },
  { event: 'message', data: 'line one\nline two\n spaced' },
  { event: 'done', data: '日本語 😀' },
]

function parseAll(chunks: string[]): SseEvent[] {
  const p = new SseParser()
  const out: SseEvent[] = []
  for (const c of chunks) out.push(...p.push(c))
  out.push(...p.end())
  return out
}

describe('SseParser', () => {
  it('parses a whole fixture', () => {
    expect(parseAll([FIXTURE])).toEqual(EXPECTED)
  })
  it('parses identically when split at every position', () => {
    for (let i = 0; i <= FIXTURE.length; i++) {
      expect(parseAll([FIXTURE.slice(0, i), FIXTURE.slice(i)])).toEqual(EXPECTED)
    }
  })
  it('parses identically one character at a time', () => {
    expect(parseAll(FIXTURE.split(''))).toEqual(EXPECTED)
  })
  it('handles CRLF, split at every position', () => {
    const crlf = FIXTURE.replace(/\n/g, '\r\n')
    expect(parseAll([crlf])).toEqual(EXPECTED)
    for (let i = 0; i <= crlf.length; i++) {
      expect(parseAll([crlf.slice(0, i), crlf.slice(i)])).toEqual(EXPECTED)
    }
  })
  it('handles lone CR line endings', () => {
    const cr = FIXTURE.replace(/\n/g, '\r')
    expect(parseAll([cr])).toEqual(EXPECTED)
    // a CR at the end of a chunk followed by LF at the start of the next must not make a blank line
    expect(parseAll(['data: a\r', '\ndata: b\r\n\r\n'])).toEqual([{ event: 'message', data: 'a\nb' }])
  })
  it('ignores comment lines and unknown fields', () => {
    expect(parseAll([': hi\n: there\n\n'])).toEqual([])
    expect(parseAll(['foo: bar\ndata: x\n\n'])).toEqual([{ event: 'message', data: 'x' }])
  })
  it('flushes an unterminated final event with data on end()', () => {
    const p = new SseParser()
    expect(p.push('event: done\ndata: bye')).toEqual([])
    expect(p.end()).toEqual([{ event: 'done', data: 'bye' }])
  })
  it('does not flush an unterminated event without data', () => {
    const p = new SseParser()
    p.push('event: done\n')
    expect(p.end()).toEqual([])
  })
  it('does not double flush', () => {
    const p = new SseParser()
    p.push('data: x')
    expect(p.end()).toHaveLength(1)
    expect(p.end()).toEqual([])
  })
  it('resets the event name between events', () => {
    expect(parseAll(['event: a\ndata: 1\n\ndata: 2\n\n'])).toEqual([
      { event: 'a', data: '1' },
      { event: 'message', data: '2' },
    ])
  })
})

function fakeClient(res: Response, seen?: { path?: string; init?: RequestInit }): Client {
  return {
    get: async () => {
      throw new Error('unused')
    },
    post: async () => {
      throw new Error('unused')
    },
    del: async () => {
      throw new Error('unused')
    },
    raw: async (path, init) => {
      if (seen) {
        seen.path = path
        seen.init = init
      }
      return res
    },
  }
}

function streamOf(bytes: Uint8Array, size: number): ReadableStream<Uint8Array> {
  let pos = 0
  return new ReadableStream<Uint8Array>({
    pull(controller) {
      if (pos >= bytes.length) {
        controller.close()
        return
      }
      controller.enqueue(bytes.slice(pos, pos + size))
      pos += size
    },
  })
}

async function collect(gen: AsyncGenerator<SseEvent>): Promise<SseEvent[]> {
  const out: SseEvent[] = []
  for await (const e of gen) out.push(e)
  return out
}

describe('streamPost', () => {
  it('yields events in order, even when chunks split multibyte characters', async () => {
    const bytes = new TextEncoder().encode(FIXTURE)
    for (const size of [1, 2, 3, 5, 1000]) {
      const seen: { path?: string; init?: RequestInit } = {}
      const res = new Response(streamOf(bytes, size), { status: 200 })
      const events = await collect(streamPost(fakeClient(res, seen), '/api/x', { q: 1 }))
      expect(events).toEqual(EXPECTED)
      expect(seen.path).toBe('/api/x')
      expect(seen.init?.method).toBe('POST')
      expect(seen.init?.body).toBe('{"q":1}')
      expect(new Headers(seen.init?.headers).get('Content-Type')).toBe('application/json')
      expect(new Headers(seen.init?.headers).get('Accept')).toBe('text/event-stream')
    }
  })
  it('flushes a trailing event without a final blank line', async () => {
    const res = new Response(new TextEncoder().encode('event: done\ndata: ok'), { status: 200 })
    expect(await collect(streamPost(fakeClient(res), '/x', {}))).toEqual([{ event: 'done', data: 'ok' }])
  })
  it('throws an ApiError carrying the server error message on non-2xx', async () => {
    const res = new Response(JSON.stringify({ error: 'no generator configured' }), {
      status: 503,
      headers: { 'Content-Type': 'application/json' },
    })
    const err = await collect(streamPost(fakeClient(res), '/x', {})).catch((e: unknown) => e)
    expect(err).toBeInstanceOf(ApiError)
    expect((err as ApiError).status).toBe(503)
    expect((err as ApiError).message).toBe('no generator configured')
  })
  it('throws when the response has no body', async () => {
    const res = new Response(null, { status: 200 })
    await expect(collect(streamPost(fakeClient(res), '/x', {}))).rejects.toBeInstanceOf(ApiError)
  })
})
