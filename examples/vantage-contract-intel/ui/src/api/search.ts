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
  citations?: unknown[]   // omitted by the current /api/v1/search response
  strategy_scores: Record<string, number>
  confidence: number
}

export async function search(query: string, collectionId: string, topK = 15): Promise<SearchResult> {
  const res = await arcanumFetch('/api/v1/search', {
    method: 'POST',
    body: JSON.stringify({ query, collection_id: collectionId, top_k: topK }),
  })
  if (!res.ok) throw new Error(`Search failed: ${res.status}`)
  return res.json()
}

/// RAPTOR clause-group level label from metadata, when the result came from RAPTOR.
export function clauseLevel(chunk: RetrievedChunk): string | null {
  if (chunk.strategy !== 'Raptor') return null
  const lvl = chunk.indexed_chunk.chunk.metadata?.['level']
  if (lvl === 0 || lvl === '0') return 'L0 Clause'
  if (lvl === 1 || lvl === '1') return 'L1 Clause Group'
  if (lvl === 2 || lvl === '2') return 'L2 Contract Summary'
  return 'RAPTOR'
}

export const ALL_STRATEGIES = ['Bm25', 'Vector', 'Graph', 'Raptor'] as const
