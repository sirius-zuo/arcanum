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

export async function search(query: string, collectionId: string, topK = 15): Promise<SearchResult> {
  const res = await arcanumFetch('/api/v1/search', {
    method: 'POST',
    body: JSON.stringify({ query, collection_id: collectionId, top_k: topK }),
  })
  if (!res.ok) throw new Error(`Search failed: ${res.status}`)
  return res.json()
}

export type ResultType = 'Passage' | 'Chapter Summary' | 'Book Summary' | 'Graph' | 'Match'

/// Library-friendly result-type label. RAPTOR level → L0 Passage / L1 Chapter / L2 Book.
export function resultType(chunk: RetrievedChunk): ResultType {
  if (chunk.strategy === 'Graph') return 'Graph'
  if (chunk.strategy === 'Raptor') {
    const lvl = chunk.indexed_chunk.chunk.metadata?.['level']
    if (lvl === 0 || lvl === '0') return 'Passage'
    if (lvl === 1 || lvl === '1') return 'Chapter Summary'
    if (lvl === 2 || lvl === '2') return 'Book Summary'
    // Finding #8: any RAPTOR chunk with absent/unknown level is a summary of some kind —
    // default to Chapter Summary so it surfaces in discovery views rather than as 'Match'.
    return 'Chapter Summary'
  }
  if (chunk.strategy === 'Vector' || chunk.strategy === 'Bm25') return 'Passage'
  return 'Match'
}
