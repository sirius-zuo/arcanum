import { apiKey, arcanumFetch } from './client'

export interface IngestResponse {
  operation_id: string
}

/// Upload raw file bytes to POST /api/v1/upload.
/// Must use raw fetch (not arcanumFetch) because the body is binary, not JSON.
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

/// Ingest a server-side file by path (the bundled samples/ dir).
export async function ingestSample(
  serverPath: string,
  collectionId: string,
  pipeline?: string,
): Promise<IngestResponse> {
  const res = await arcanumFetch('/api/v1/ingest', {
    method: 'POST',
    body: JSON.stringify({
      source_uri: serverPath,
      collection_id: collectionId,
      pipeline: pipeline ?? null,
    }),
  })
  if (!res.ok) throw new Error(`Ingest failed: ${res.status}`)
  return res.json()
}
