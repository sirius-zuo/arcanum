const API_KEY = import.meta.env.VITE_API_KEY ?? ''

export async function arcanumFetch(url: string, init?: RequestInit): Promise<Response> {
  const headers: Record<string, string> = {}
  if (API_KEY) headers['Authorization'] = `Bearer ${API_KEY}`
  // Finding #1: Don't set Content-Type for FormData — the browser sets
  // multipart/form-data with the correct boundary automatically.
  // For all other requests, default to application/json.
  if (!(init?.body instanceof FormData)) {
    headers['Content-Type'] = 'application/json'
  }
  return fetch(url, {
    ...init,
    headers: { ...headers, ...(init?.headers as Record<string, string> ?? {}) },
  })
}
