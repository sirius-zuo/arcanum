const apiKey =
  import.meta.env.VITE_API_KEY ??
  localStorage.getItem('arcanum_key') ??
  ''

export interface IngestResponse {
  operation_id: string
}

/// Upload raw file bytes to POST /api/v1/upload (works for text AND binary files).
/// Uses fetch directly (not arcanumFetch) so the body is the file bytes, not JSON.
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

/// Ingest a file the server can already read by path (the bundled samples/ dir).
/// Sends a source_uri to POST /api/v1/ingest; the engine's FileLoader reads it.
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
      // Field name MUST be `pipeline` — arcanum-server's HttpIngestRequest uses `pipeline`.
      pipeline: pipeline ?? null,
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
  return res.json()
}
