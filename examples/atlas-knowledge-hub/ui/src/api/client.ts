export class ApiError extends Error {
  status: number
  body: unknown

  constructor(status: number, message: string, body: unknown = null) {
    super(message)
    this.name = 'ApiError'
    this.status = status
    this.body = body
  }
}

export interface Client {
  get<T>(path: string): Promise<T>
  post<T>(path: string, body?: unknown): Promise<T>
  del(path: string): Promise<void>
  raw(path: string, init?: RequestInit): Promise<Response>
}

async function readError(res: Response): Promise<ApiError> {
  const text = await res.text().catch(() => '')
  let body: unknown = text || null
  let message = `${res.status} ${res.statusText}`.trim()
  if (text) {
    try {
      body = JSON.parse(text)
      const err = (body as { error?: unknown } | null)?.error
      if (typeof err === 'string' && err) message = err
    } catch {
      message = text.slice(0, 200)
    }
  }
  return new ApiError(res.status, message, body)
}

/** 204, 205 and 201 carry no JSON the server promises; never parse them. */
function hasNoBody(res: Response): boolean {
  return res.status === 204 || res.status === 205 || res.status === 201
}

export function createClient(getKey: () => string | null): Client {
  async function raw(path: string, init: RequestInit = {}): Promise<Response> {
    const key = getKey()
    if (!key) {
      throw new ApiError(0, 'The API key is not available yet (bootstrap has not completed).')
    }
    const headers = new Headers(init.headers)
    headers.set('Authorization', `Bearer ${key}`)
    return fetch(path, { ...init, headers })
  }

  async function json<T>(path: string, init: RequestInit): Promise<T> {
    const res = await raw(path, init)
    if (!res.ok) throw await readError(res)
    if (hasNoBody(res)) return undefined as T
    return (await res.json()) as T
  }

  return {
    get: <T,>(path: string) => json<T>(path, { method: 'GET' }),
    post: <T,>(path: string, body?: unknown) =>
      json<T>(path, {
        method: 'POST',
        headers: body === undefined ? undefined : { 'Content-Type': 'application/json' },
        body: body === undefined ? undefined : JSON.stringify(body),
      }),
    del: async (path: string) => {
      const res = await raw(path, { method: 'DELETE' })
      if (!res.ok) throw await readError(res)
    },
    raw,
  }
}
