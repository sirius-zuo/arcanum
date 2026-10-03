import { useDocumentText } from './library'
import type { Client } from './client'
import type { ProofChain } from './types'

const UUID = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i

/** The evidence routes answer 400 for anything but a UUID; check first and skip the round trip. */
export function isUuid(s: string): boolean {
  return UUID.test(s.trim())
}

const seg = (s: string) => encodeURIComponent(s.trim())

export function getChunkProof(client: Client, id: string): Promise<ProofChain> {
  return client.get<ProofChain>(`/evidence/chunk/${seg(id)}`)
}

export function getTreeNodeProof(client: Client, id: string): Promise<ProofChain> {
  return client.get<ProofChain>(`/evidence/tree-node/${seg(id)}`)
}

export function getEntityProof(client: Client, id: string): Promise<ProofChain> {
  return client.get<ProofChain>(`/evidence/entity/${seg(id)}`)
}

/** The relation type is a free string (it can hold `/`), so it must be encoded. */
export function getRelationProof(client: Client, source: string, relationType: string, target: string): Promise<ProofChain> {
  return client.get<ProofChain>(`/evidence/relation/${seg(source)}/${encodeURIComponent(relationType)}/${seg(target)}`)
}

/** The canonical text of a document version. Evidence byte offsets index this text. */
export function useSourceText(documentId: string, version: number) {
  return useDocumentText(documentId, version)
}
