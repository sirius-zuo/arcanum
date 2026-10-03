function toHex(buf: ArrayBuffer): string {
  return Array.from(new Uint8Array(buf), (b) => b.toString(16).padStart(2, '0')).join('')
}

/**
 * Deterministic idempotency key: `<collection>:<sha256 hex>` over the source uri and the bytes.
 * Submitting the same file twice therefore replays the first operation instead of ingesting again.
 */
export async function idempotencyKey(collection: string, sourceUri: string, bytes: ArrayBuffer): Promise<string> {
  const head = new TextEncoder().encode(`${sourceUri}\0`)
  const body = new Uint8Array(bytes)
  const all = new Uint8Array(head.length + body.length)
  all.set(head, 0)
  all.set(body, head.length)
  const digest = await crypto.subtle.digest('SHA-256', all)
  return `${collection}:${toHex(digest)}`
}
