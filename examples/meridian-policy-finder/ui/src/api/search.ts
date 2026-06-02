import { arcanumFetch } from './client'

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
  query: string,
  collectionId: string,
  topK = 6,
): Promise<SearchResult> {
  const res = await arcanumFetch('/api/v1/search', {
    method: 'POST',
    body: JSON.stringify({ query, collection_id: collectionId, top_k: topK }),
  })
  if (!res.ok) throw new Error(`Search failed: ${res.status}`)
  return res.json()
}

/// Returns the dominant strategy label for the routing indicator.
/// Picks the strategy with the highest aggregate score.
export function dominantStrategy(scores: Record<string, number>): string | null {
  const entries = Object.entries(scores)
  if (entries.length === 0) return null
  entries.sort((a, b) => b[1] - a[1])
  return entries[0][0]
}
