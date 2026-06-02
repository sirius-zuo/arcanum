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

export async function search(query: string, collectionId: string, topK = 12): Promise<SearchResult> {
  const res = await arcanumFetch('/api/v1/search', {
    method: 'POST',
    body: JSON.stringify({ query, collection_id: collectionId, top_k: topK }),
  })
  if (!res.ok) throw new Error(`Search failed: ${res.status}`)
  return res.json()
}

// Finding #4: returns null (not the string 'RAPTOR') when level metadata is absent.
// The string 'RAPTOR' was previously returned as a fallback, producing a redundant badge.
export function raptorLevel(chunk: RetrievedChunk): string | null {
  if (chunk.strategy !== 'Raptor') return null
  const lvl = chunk.indexed_chunk.chunk.metadata?.['level']
  if (lvl === undefined || lvl === null) return null
  if (lvl === 0 || lvl === '0') return 'L0 Passage'
  if (lvl === 1 || lvl === '1') return 'L1 Chapter Summary'
  if (lvl === 2 || lvl === '2') return 'L2 Study Summary'
  return null
}

// Finding #8: guards against null/undefined scores before calling Object.entries().
export function dominantStrategy(scores: Record<string, number> | null | undefined): string | null {
  if (!scores) return null
  const entries = Object.entries(scores)
  if (entries.length === 0) return null
  entries.sort((a, b) => b[1] - a[1])
  return entries[0][0]
}
