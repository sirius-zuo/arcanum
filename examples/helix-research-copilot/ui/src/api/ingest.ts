import { arcanumFetch, apiKey } from './client'

export interface IngestResponse {
  operation_id: string
}

/// Upload raw file bytes to POST /api/v1/upload (works for text AND binary files).
export async function uploadFile(
  file: File,
  collectionId: string,
  pipeline?: string,
): Promise<IngestResponse> {
  const qs = new URLSearchParams({ collection_id: collectionId, filename: file.name })
  if (pipeline) qs.set('pipeline', pipeline)
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

/// Ingest a bundled sample the server can read by path; the engine's FileLoader reads it.
export async function ingestSample(
  serverPath: string,
  collectionId: string,
  pipeline?: string,
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
    }),
  })
  if (!res.ok) throw new Error(`Ingest failed: ${res.status}`)
  return res.json()
}

/// List collections known to the server.
export async function listCollections(): Promise<{ id: string; name: string }[]> {
  const res = await arcanumFetch('/admin/collections')
  if (!res.ok) return []
  const data = await res.json()
  return Array.isArray(data.collections) ? data.collections : []
}
