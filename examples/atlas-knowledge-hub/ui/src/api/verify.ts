import type { Client } from './client'
import type { ContextResponse, VerifyPassageRef, VerifyRequest, VerifyResponse } from './types'

/** The `{ref_id, chunk_ids}` pairs Verify needs, taken from a Context response. */
export function passagesForVerify(context: ContextResponse): VerifyPassageRef[] {
  return context.passages.map((p) => ({ ref_id: p.ref_id, chunk_ids: p.chunk_ids }))
}

export function verifyAnswer(client: Client, req: VerifyRequest): Promise<VerifyResponse> {
  return client.post<VerifyResponse>('/api/v1/verify', req)
}
