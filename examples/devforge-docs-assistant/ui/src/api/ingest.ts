export interface IngestResponse {
  operation_id: string
}

export async function uploadFile(
  _file: File,
  _collectionId: string,
  _pipeline?: string,
): Promise<IngestResponse> {
  return { operation_id: '' }
}

export async function ingestSample(
  _serverPath: string,
  _collectionId: string,
  _pipeline?: string,
): Promise<IngestResponse> {
  return { operation_id: '' }
}

export async function listCollections(): Promise<{ id: string; name: string }[]> {
  return []
}
