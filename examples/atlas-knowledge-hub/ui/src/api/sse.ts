import { ApiError, readError, type Client } from './client'

export interface SseEvent {
  event: string
  data: string
}

/**
 * Incremental Server-Sent Events parser. Chunks may split anywhere, including
 * inside a CRLF pair. `id:` and `retry:` fields are ignored.
 */
export class SseParser {
  private buf = ''
  private eventName = ''
  private dataLines: string[] = []

  push(chunk: string): SseEvent[] {
    this.buf += chunk
    const out: SseEvent[] = []
    let pos = 0
    for (;;) {
      let eol = -1
      for (let i = pos; i < this.buf.length; i++) {
        const c = this.buf[i]
        if (c === '\n' || c === '\r') {
          eol = i
          break
        }
      }
      if (eol < 0) break
      // A trailing CR may be the first half of CRLF: wait for the next chunk.
      if (this.buf[eol] === '\r' && eol === this.buf.length - 1) break
      const line = this.buf.slice(pos, eol)
      pos = eol + (this.buf[eol] === '\r' && this.buf[eol + 1] === '\n' ? 2 : 1)
      const ev = this.line(line)
      if (ev) out.push(ev)
    }
    this.buf = this.buf.slice(pos)
    return out
  }

  /** Flush at end of stream. An unterminated event is emitted only if it has data. */
  end(): SseEvent[] {
    const out: SseEvent[] = []
    const rest = this.buf.endsWith('\r') ? this.buf.slice(0, -1) : this.buf
    this.buf = ''
    if (rest.length > 0) this.line(rest)
    const ev = this.dispatch()
    if (ev) out.push(ev)
    return out
  }

  private line(line: string): SseEvent | null {
    if (line === '') return this.dispatch()
    if (line.startsWith(':')) return null
    const colon = line.indexOf(':')
    const field = colon < 0 ? line : line.slice(0, colon)
    let value = colon < 0 ? '' : line.slice(colon + 1)
    if (value.startsWith(' ')) value = value.slice(1)
    if (field === 'event') this.eventName = value
    else if (field === 'data') this.dataLines.push(value)
    return null
  }

  private dispatch(): SseEvent | null {
    const had = this.dataLines.length > 0
    const ev = had ? { event: this.eventName || 'message', data: this.dataLines.join('\n') } : null
    this.eventName = ''
    this.dataLines = []
    return ev
  }
}

/** POST `body` as JSON and yield the Server-Sent Events of the response. */
export async function* streamPost(
  client: Client,
  path: string,
  body: unknown,
  signal?: AbortSignal,
): AsyncGenerator<SseEvent> {
  const res = await client.raw(path, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json', Accept: 'text/event-stream' },
    body: JSON.stringify(body),
    signal,
  })
  if (!res.ok) throw await readError(res)
  if (!res.body) throw new ApiError(res.status, 'The response has no body to stream.')

  const reader = res.body.getReader()
  const decoder = new TextDecoder()
  const parser = new SseParser()
  try {
    for (;;) {
      const { done, value } = await reader.read()
      if (done) break
      yield* parser.push(decoder.decode(value, { stream: true }))
    }
    yield* parser.push(decoder.decode())
    yield* parser.end()
  } finally {
    reader.releaseLock()
  }
}
