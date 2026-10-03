import { afterEach, describe, expect, it, vi } from 'vitest'
import { ApiError, createClient } from './client'

function mockFetch(res: Response) {
  const fn = vi.fn().mockResolvedValue(res)
  vi.stubGlobal('fetch', fn)
  return fn
}

describe('api client', () => {
  afterEach(() => {
    vi.unstubAllGlobals()
  })

  it('apierror_carries_status_and_message', async () => {
    mockFetch(new Response(JSON.stringify({ error: 'no such thing' }), { status: 404 }))
    const c = createClient(() => 'k')
    const err = await c.get('/x').catch((e: unknown) => e)
    expect(err).toBeInstanceOf(ApiError)
    const e = err as ApiError
    expect(e.status).toBe(404)
    expect(e.message).toBe('no such thing')
    expect(e.body).toEqual({ error: 'no such thing' })
  })

  it('post_sends_bearer_header_and_json_body', async () => {
    const fn = mockFetch(new Response(JSON.stringify({ ok: true }), { status: 200 }))
    const c = createClient(() => 'secret')
    await expect(c.post('/p', { a: 1 })).resolves.toEqual({ ok: true })
    const [url, init] = fn.mock.calls[0] as [string, RequestInit]
    expect(url).toBe('/p')
    expect(init.method).toBe('POST')
    expect(new Headers(init.headers).get('Authorization')).toBe('Bearer secret')
    expect(new Headers(init.headers).get('Content-Type')).toBe('application/json')
    expect(init.body).toBe('{"a":1}')
  })

  it('204_and_201_resolve_without_parsing', async () => {
    const c = createClient(() => 'k')
    mockFetch(new Response(null, { status: 204 }))
    await expect(c.del('/d')).resolves.toBeUndefined()
    mockFetch(new Response('', { status: 201 }))
    await expect(c.post('/created')).resolves.toBeUndefined()
  })

  it('401_rejects_with_apierror', async () => {
    mockFetch(new Response('nope', { status: 401 }))
    const c = createClient(() => 'bad')
    const err = (await c.get('/x').catch((e: unknown) => e)) as ApiError
    expect(err).toBeInstanceOf(ApiError)
    expect(err.status).toBe(401)
  })

  it('does_not_send_before_a_key_exists', async () => {
    const fn = mockFetch(new Response('{}', { status: 200 }))
    const c = createClient(() => null)
    await expect(c.get('/x')).rejects.toBeInstanceOf(ApiError)
    expect(fn).not.toHaveBeenCalled()
  })
})
