export interface RetrievedChunk {
  indexed_chunk: {
    chunk: { id: string; text: string; collection_id: string; metadata: Record<string, unknown> }
    store_id: string
  }
  score: number
  strategy: 'Vector' | 'Bm25' | 'Graph' | 'Raptor' | 'ColBert'
}

export interface SearchResult {
  chunks: RetrievedChunk[]
  citations?: unknown[]
  strategy_scores: Record<string, number>
  confidence: number
}

export async function search(
  _query: string,
  _collectionId: string,
  _topK = 8,
): Promise<SearchResult> {
  return { chunks: [], strategy_scores: {}, confidence: 0 }
}
