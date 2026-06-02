import { apiKey } from './auth'

export interface IngestResponse {
  operation_id: string
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
  const res = await fetch('/admin/collections', {
    headers: { Authorization: `Bearer ${apiKey}` },
  })
  if (!res.ok) return []
  const data = await res.json()
  // Server returns { "collections": [...] } — extract the array.
  return Array.isArray(data.collections) ? data.collections : []
}
