const API_KEY = import.meta.env.VITE_API_KEY ?? ''

export async function arcanumFetch(url: string, init?: RequestInit): Promise<Response> {
  const headers: Record<string, string> = { 'Content-Type': 'application/json' }
  if (API_KEY) headers['Authorization'] = `Bearer ${API_KEY}`
  return fetch(url, { ...init, headers: { ...headers, ...(init?.headers as Record<string, string> ?? {}) } })
}
