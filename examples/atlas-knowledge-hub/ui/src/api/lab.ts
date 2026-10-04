import { ApiError, readError } from './client'
import type { Client } from './client'
import { searchChunks } from './search'
import type {
  BenchmarkMetrics,
  ChunkStrategyConfig,
  EvalResponse,
  Experiment,
  ExperimentEvalResult,
  ExperimentSample,
  GoldenQuery,
  InspectResult,
  PerBackendChunkConfig,
} from './types'

export function inspectChunking(client: Client, text: string, strategies: ChunkStrategyConfig[]): Promise<InspectResult[]> {
  return client.post<{ results: InspectResult[] }>('/api/v1/chunk/inspect', { text, strategies }).then((r) => r.results)
}

export interface CorpusDoc {
  source_uri: string
  text: string
}

/** The benchmark takes `RawDocument`s: content is a JSON array of UTF-8 bytes and the id a UUID. */
export function runBenchmark(
  client: Client,
  corpus: CorpusDoc[],
  queries: GoldenQuery[],
  strategies: ChunkStrategyConfig[],
): Promise<BenchmarkMetrics[]> {
  const encoder = new TextEncoder()
  const ids = new Map<string, string>()
  const docs = corpus.map((d) => {
    const id = crypto.randomUUID()
    ids.set(d.source_uri, id)
    return { id, content: Array.from(encoder.encode(d.text)), mime_type: 'text/plain', source_uri: d.source_uri, metadata: {} }
  })
  // A query with no expected document would score a perfect 1.0 on the server, so leave it out.
  const labeled = queries
    .filter((q) => ids.has(q.relevant_source_uri))
    .map((q) => ({ text: q.query, expected_doc_ids: [ids.get(q.relevant_source_uri) as string] }))
  return client
    .post<{ metrics: BenchmarkMetrics[] }>('/api/v1/chunk/benchmark', { corpus: docs, queries: labeled, strategies })
    .then((r) => r.metrics)
}

const base = (collection: string) => `/api/v1/collections/${encodeURIComponent(collection)}/experiments`

/** The server answers 201 with a JSON body here, but `client.post` treats every 201 as empty, so read it directly. */
export async function startExperiment(client: Client, collection: string, config: PerBackendChunkConfig): Promise<Experiment> {
  const res = await client.raw(base(collection), {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(config),
  })
  if (res.ok) return (await res.json()) as Experiment
  const err = await readError(res)
  if (err.status === 409) {
    throw new ApiError(409, 'An experiment is already active on this collection. Abandon it before starting another.', err.body)
  }
  throw err
}

export function getExperiment(client: Client, collection: string, id: string): Promise<Experiment> {
  return client.get<Experiment>(`${base(collection)}/${encodeURIComponent(id)}`)
}

export function evalExperiment(client: Client, collection: string, id: string, samples: ExperimentSample[]): Promise<ExperimentEvalResult> {
  return client.post<ExperimentEvalResult>(`${base(collection)}/${encodeURIComponent(id)}/eval`, samples)
}

export function promoteExperiment(client: Client, collection: string, id: string): Promise<{ status: string; message: string }> {
  return client.post(`${base(collection)}/${encodeURIComponent(id)}/promote`)
}

export function abandonExperiment(client: Client, collection: string, id: string): Promise<void> {
  return client.del(`${base(collection)}/${encodeURIComponent(id)}`)
}

/**
 * Turn the golden set into labeled chunk samples: search each query and keep the returned chunks whose
 * source is the expected document. A query whose expected document is not retrieved has no usable label.
 */
export async function buildExperimentSamples(
  client: Client,
  collection: string,
  golden: GoldenQuery[],
): Promise<{ samples: ExperimentSample[]; skipped: number }> {
  const samples: ExperimentSample[] = []
  let skipped = 0
  for (const g of golden) {
    const res = await searchChunks(client, { query: g.query, collection_id: collection, top_k: 10 })
    const ids = res.chunks
      .map((c) => c.indexed_chunk.chunk)
      .filter((c) => c.provenance.source_uri === g.relevant_source_uri)
      .map((c) => c.id)
    if (ids.length === 0) skipped += 1
    else samples.push({ query: g.query, relevant_chunk_ids: ids })
  }
  return { samples, skipped }
}

export function runEval(client: Client): Promise<EvalResponse> {
  return client.post<EvalResponse>('/demo/eval')
}
