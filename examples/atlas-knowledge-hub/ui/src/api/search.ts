import { useMutation } from '@tanstack/react-query'
import { useBootstrap } from '../state/bootstrap'
import type { Client } from './client'
import type { RetrievedChunk, SearchChunk, SearchRequest, SearchResponse } from './types'

/** What the server sends: each indexed chunk carries its full embedding vectors. */
export interface RawIndexedChunk {
  chunk: SearchChunk
  vector?: unknown
  token_vectors?: unknown
  store_id: string
}

export interface RawRetrievedChunk extends Omit<RetrievedChunk, 'indexed_chunk'> {
  indexed_chunk: RawIndexedChunk
}

export interface RawSearchResponse extends Omit<SearchResponse, 'chunks'> {
  chunks: RawRetrievedChunk[]
}

export type SearchResult = SearchResponse

/** Drop the embedding vectors and keep everything else. Never mutates its input. */
export function stripVectors(raw: RawSearchResponse): SearchResult {
  return {
    ...raw,
    chunks: raw.chunks.map(({ indexed_chunk, ...rest }) => ({
      ...rest,
      indexed_chunk: { chunk: indexed_chunk.chunk, store_id: indexed_chunk.store_id },
    })),
  }
}

export async function searchChunks(client: Client, req: SearchRequest): Promise<SearchResult> {
  const raw = await client.post<RawSearchResponse>('/api/v1/search', req)
  return stripVectors(raw)
}

export function useSearch() {
  const { client } = useBootstrap()
  return useMutation({ mutationFn: (req: SearchRequest) => searchChunks(client, req) })
}
