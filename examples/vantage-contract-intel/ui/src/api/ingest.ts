import { arcanumFetch } from './client'

export async function uploadFile(
  file: File,
  collectionId: string,
  pipeline: string,
): Promise<void> {
  const formData = new FormData()
  formData.append('file', file)
  const res = await arcanumFetch(
    `/api/v1/upload?collection_id=${collectionId}&pipeline=${pipeline}`,
    { method: 'POST', body: formData },
  )
  if (!res.ok) throw new Error(`Upload failed: ${res.status}`)
}

export async function ingestSample(
  filename: string,
  collectionId: string,
  pipeline: string,
): Promise<void> {
  const res = await arcanumFetch(
    `/api/v1/ingest/sample?filename=${encodeURIComponent(filename)}&collection_id=${collectionId}&pipeline=${pipeline}`,
    { method: 'POST' },
  )
  if (!res.ok) throw new Error(`Ingest failed: ${res.status}`)
}
