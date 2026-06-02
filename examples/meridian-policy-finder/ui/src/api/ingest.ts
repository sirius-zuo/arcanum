import { apiKey } from './client'

export interface IngestResponse {
  operation_id: string
}

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

