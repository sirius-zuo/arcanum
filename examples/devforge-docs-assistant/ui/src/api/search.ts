import { arcanumFetch } from './client'

export interface ChunkPosition {
  start: number
  end: number
  index: number
}

export interface RetrievedChunk {
  indexed_chunk: {
    chunk: {
      id: string
      text: string
      collection_id: string
      position: ChunkPosition
      metadata: Record<string, unknown>
    }
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
  query: string,
  collectionId: string,
  topK = 8,
): Promise<SearchResult> {
  const res = await arcanumFetch('/api/v1/search', {
    method: 'POST',
    body: JSON.stringify({ query, collection_id: collectionId, top_k: topK }),
  })
  if (!res.ok) throw new Error(`Search failed: ${res.status}`)
  return res.json()
}
