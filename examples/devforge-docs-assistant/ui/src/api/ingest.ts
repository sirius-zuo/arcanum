import { apiKey } from './auth'

export interface IngestResponse {
  operation_id: string
}

export interface RemoteCollection {
  id: string
  name: string
}

export async function listVectorCollections(): Promise<RemoteCollection[]> {
  const res = await fetch('/api/v1/vector/collections', {
    headers: { Authorization: `Bearer ${apiKey}` },
  })
  if (!res.ok) return []
  const data = await res.json()
  return Array.isArray(data.collections)
    ? data.collections.map((id: string) => ({ id, name: id }))
    : []
}

export async function getVectorCollectionStats(name: string): Promise<number> {
  const res = await fetch(`/api/v1/vector/collections/${encodeURIComponent(name)}/stats`, {
    headers: { Authorization: `Bearer ${apiKey}` },
  })
  if (!res.ok) return 0
  const data = await res.json()
  return data.count ?? 0
}

export async function createVectorCollection(name: string): Promise<{ ok: boolean; conflict: boolean }> {
  const res = await fetch(`/api/v1/vector/collections/${encodeURIComponent(name)}`, {
    method: 'POST',
    headers: { Authorization: `Bearer ${apiKey}` },
  })
  return { ok: res.status === 201, conflict: res.status === 409 }
}

export async function deleteVectorCollection(name: string): Promise<void> {
  await fetch(`/api/v1/vector/collections/${encodeURIComponent(name)}`, {
    method: 'DELETE',
    headers: { Authorization: `Bearer ${apiKey}` },
  })
}

/// Upload raw file bytes to POST /api/v1/upload.
export async function uploadFile(
  file: File,
  collectionId: string,
  pipeline?: string,
  force?: boolean,
): Promise<IngestResponse> {
  const qs = new URLSearchParams({ collection_id: collectionId, filename: file.name })
  if (pipeline) qs.set('pipeline', pipeline)
  if (force) qs.set('force', 'true')
  const res = await fetch(`/api/v1/upload?${qs.toString()}`, {
    method: 'POST',
    headers: {
      Authorization: `Bearer ${apiKey}`,
      'Content-Type': 'application/octet-stream',
    },
    body: file,
  })
  if (!res.ok) throw new Error(`Upload failed: ${res.status}`)
  return res.json()
}

/// Ingest a server-side file by path (the bundled samples/ dir).
export async function ingestSample(
  serverPath: string,
  collectionId: string,
  pipeline?: string,
  force?: boolean,
): Promise<IngestResponse> {
  const res = await fetch('/api/v1/ingest', {
    method: 'POST',
    headers: {
      Authorization: `Bearer ${apiKey}`,
      'Content-Type': 'application/json',
    },
    body: JSON.stringify({
      source_uri: serverPath,
      collection_id: collectionId,
      pipeline: pipeline ?? null,
      force: force ?? false,
    }),
  })
  if (!res.ok) throw new Error(`Ingest failed: ${res.status}`)
  return res.json()
}

export async function listCollections(): Promise<{ id: string; name: string }[]> {
  return listVectorCollections()
}

export interface DocumentEntry {
  source_uri: string
  registered_at: number  // Unix seconds
}

export async function listCollectionDocuments(name: string): Promise<DocumentEntry[]> {
  const res = await fetch(`/api/v1/vector/collections/${encodeURIComponent(name)}/documents`, {
    headers: { Authorization: `Bearer ${apiKey}` },
  })
  if (!res.ok) return []
  const data = await res.json()
  return Array.isArray(data.documents) ? data.documents : []
}

export async function deleteCollectionDocument(name: string, sourceUri: string): Promise<void> {
  await fetch(
    `/api/v1/vector/collections/${encodeURIComponent(name)}/documents?source_uri=${encodeURIComponent(sourceUri)}`,
    { method: 'DELETE', headers: { Authorization: `Bearer ${apiKey}` } }
  )
}
