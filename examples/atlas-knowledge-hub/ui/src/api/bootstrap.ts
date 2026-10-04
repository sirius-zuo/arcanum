import { ApiError } from './client'
import type { Bootstrap } from './types'

/** The one unauthenticated call: it hands the UI its API key and the collection. */
export async function fetchBootstrap(): Promise<Bootstrap> {
  const res = await fetch('/demo/bootstrap')
  if (!res.ok) {
    let message = `Bootstrap failed (${res.status})`
    try {
      const body = (await res.json()) as { error?: string }
      if (body.error) message = body.error
    } catch {
      // keep the status based message
    }
    throw new ApiError(res.status, message)
  }
  return (await res.json()) as Bootstrap
}
