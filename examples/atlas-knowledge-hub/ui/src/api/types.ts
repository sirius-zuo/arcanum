// Field names are snake_case exactly as the server serializes them.

// ---------- /demo ----------

export interface GeneratorInfo {
  name: string
  protocol: string
  model: string
  is_default: boolean
}

export interface Features {
  context: boolean
  generate: boolean
  verify: boolean
  evidence: boolean
  experiments: boolean
  gc: boolean
}

export interface Bootstrap {
  api_key: string
  collection: string
  orchestration_mode: string
  generators: GeneratorInfo[]
  judge: string | null
  anthropic_enabled: boolean
  ollama_url: string
  mcp_port: number
  features: Features
}

export interface HealthCheck {
  id: string
  label: string
  ok: boolean
  detail: string
  fix: string | null
}

export interface DemoHealth {
  ready: boolean
  checks: HealthCheck[]
}

export type VersionStatus = 'Active' | 'Superseded' | 'Deleted'

export interface LibraryVersion {
  version_num: number
  status: VersionStatus
  ingested_at: string
  content_hash: string
  snapshot_uri: string
}

export interface LibraryDocument {
  source_uri: string
  document_id: string
  chunks: number
  versions: LibraryVersion[]
}

export interface Library {
  collection: string
  documents: LibraryDocument[]
}

export interface DocumentText {
  document_id: string
  version_num: number
  source_uri: string
  status: VersionStatus
  mime_type: string
  text: string
}

export interface SampleFile {
  path: string
  source_uri: string
  title: string
  description: string
  is_update: boolean
}

export interface GoldenQuery {
  query: string
  relevant_source_uri: string
}

export interface FlawedAnswer {
  id: string
  title: string
  question: string
  answer: string
  expected: Verdict[]
}

export interface TourStep {
  id: string
  title: string
  why: string
  action: string
  route: string
  completes_when: string
  payoff: string
}

export interface Samples {
  files: SampleFile[]
  golden: GoldenQuery[]
  flawed_answers: FlawedAnswer[]
  tour: TourStep[]
}

export interface OperationRef {
  source_uri: string
  operation_id: string
}

export interface SamplesOperations {
  operations: OperationRef[]
}

export interface EvalReport {
  hit_rate_at_k: number
  mrr: number
  ndcg_at_k: number
  k: number
  num_queries: number
  precision_at_k: number
  recall_at_k: number
}

export interface EvalQuery {
  query: string
  relevant_source_uri: string
  first_relevant_rank: number | null
}

export interface EvalResponse {
  report: EvalReport
  queries: EvalQuery[]
}

export interface MetricCounter {
  name: string
  labels: Record<string, string>
  value: number
}

export interface MetricHistogram {
  name: string
  labels: Record<string, string>
  count: number
  sum: number
}

export interface DemoMetrics {
  counters: MetricCounter[]
  histograms: MetricHistogram[]
}

export interface McpTool {
  name: string
  description: string
  input_schema: unknown
}

export interface DemoMcp {
  endpoint: string
  tools: McpTool[]
}

export interface ErrorBody {
  error: string
}

// ---------- Context API ----------

export type RenderFormat = 'numbered' | 'xml' | 'markdown'

export interface ChatMessage {
  role: 'user' | 'assistant'
  content: string
}

export interface ContextRequest {
  collection_id: string
  query?: string
  messages?: ChatMessage[]
  token_budget?: number
  background_share?: number
  candidate_k?: number
  render?: RenderFormat
}

export interface ContextPassage {
  ref_id: string
  document_id: string
  version_num: number
  source_uri: string
  snapshot_uri: string
  canonical_uri: string | null
  section: string | null
  page: number | null
  offset_start: number
  offset_end: number
  text: string
  chunk_ids: string[]
  strategies: string[]
  score: number
}

export interface ContextBackground {
  ref_id: string
  text: string
  level: number
  document_id: string
  covers: string[]
  score: number
}

export interface ContextUsage {
  budget: number
  used: number
  passages: number
  background: number
  dropped_passages: number
  counter: string
}

export interface StrategyFailure {
  strategy: string
  reason: string
}

export interface ContextRetrieval {
  queries: string[]
  strategies_ok: string[]
  strategies_failed: StrategyFailure[]
}

export interface ContextResponse {
  resolved_query: string
  resolved_query_source: 'original' | 'rewritten' | 'fallback'
  passages: ContextPassage[]
  background: ContextBackground[]
  usage: ContextUsage
  retrieval: ContextRetrieval
  rendered?: string
}

// ---------- Generate API ----------

export type GenerateMode = 'answer' | 'summarize'

export interface GenerateRequest {
  collection_id: string
  mode?: GenerateMode
  query?: string
  messages?: ChatMessage[]
  generator?: string
  max_tokens?: number
  temperature?: number
  instructions?: string
  context?: { token_budget?: number; background_share?: number; candidate_k?: number }
  stream?: boolean
  verify?: boolean
}

export interface Citation {
  ref_id: string
  chunk_ids: string[]
  document_id: string
  version_num: number
  source_uri: string
  offset_start: number
  offset_end: number
  answer_spans: [number, number][]
}

export type StopReason = 'end_turn' | 'max_tokens' | 'other'

export interface GenerateUsage {
  input_tokens: number | null
  output_tokens: number | null
}

export interface GeneratorRef {
  name: string
  model: string
}

/** The `done` event: the JSON response minus `answer` and `context`. */
export interface GenerateDone {
  status: 'ok' | 'no_context'
  citations: Citation[]
  unknown_refs: string[]
  stop_reason: StopReason
  usage: GenerateUsage
  generator: GeneratorRef
}

export interface GenerateResponse extends GenerateDone {
  answer: string
  context: ContextResponse
  verification?: VerificationOutcome | null
}

// ---------- Verify API ----------

export type Verdict = 'supported' | 'miscited' | 'uncited_supported' | 'partial' | 'unsupported' | 'no_claim'

export interface VerifyPassageRef {
  ref_id: string
  chunk_ids: string[]
}

export interface VerifyRequest {
  collection_id: string
  answer: string
  passages: VerifyPassageRef[]
  judge?: string
  strict_citations?: boolean
}

export type EvidenceVersionStatus = 'active' | 'superseded' | 'deleted' | 'unknown'

export interface ClaimEvidence {
  ref_id: string
  chunk_id: string
  document_id: string
  version_num: number
  version_status: EvidenceVersionStatus
  source_uri: string
  /** UTF-8 byte offsets into the canonical text. */
  offset_start: number
  offset_end: number
  quote: string
  quote_matched: boolean
}

export interface Claim {
  text: string
  supported: boolean
  evidence: ClaimEvidence[]
}

export interface VerifiedSentence {
  /** UTF-8 byte range into `answer`. */
  span: [number, number]
  text: string
  verdict: Verdict
  cited: string[]
  invalid_refs: string[]
  claims: Claim[]
}

export type VerdictCounts = Record<Verdict, number>

export interface JudgeUsage {
  input_tokens: number | null
  output_tokens: number | null
  judge_calls: number
}

export interface VerifyResponse {
  verdict: 'pass' | 'fail'
  strict_citations: boolean
  counts: VerdictCounts
  sentences: VerifiedSentence[]
  passages_unavailable: string[]
  judge: GeneratorRef
  usage: JudgeUsage
}

export type VerificationErrorCode =
  | 'judge_unavailable'
  | 'judge_upstream'
  | 'judge_timeout'
  | 'judge_invalid_output'
  | 'invalid'
  | 'forbidden'
  | 'internal'

export interface VerificationOk extends VerifyResponse {
  status: 'ok'
}

export interface VerificationError {
  status: 'error'
  code: VerificationErrorCode
  message: string
}

export type VerificationOutcome = VerificationOk | VerificationError

// ---------- Search ----------

export interface SearchRequest {
  query: string
  collection_id?: string
  top_k?: number
}

export interface ChunkPosition {
  start: number
  end: number
  index: number
}

export interface ChunkProvenance {
  document_version: number
  source_uri: string
  snapshot_uri: string
  canonical_uri: string | null
  page: number | null
  section: string | null
  block_ids: string[]
}

export interface SearchChunk {
  id: string
  text: string
  document_id: string
  collection_id: string
  position: ChunkPosition
  metadata: Record<string, unknown>
  provenance: ChunkProvenance
}

export type RetrievalStrategy = 'Vector' | 'Bm25' | 'ColBert' | 'Raptor' | 'Graph'

export type ChunkKind = 'Source' | { Summary: { level: number; covers: string[] } }

export interface RetrievedChunk {
  /** The server also sends `vector` and `token_vectors`; `stripVectors` removes them client-side (see api/search.ts). */
  indexed_chunk: { chunk: SearchChunk; store_id: string }
  /** Fused RRF score (about 0.016 to 0.05), not a similarity. */
  score: number
  strategy: RetrievalStrategy
  kind: ChunkKind
}

export interface SearchCitation {
  document_uri: string
  document_title: string | null
  section: string | null
  chunk_index: number
  version: number | null
  snapshot_uri: string | null
}

export interface SearchResponse {
  chunks: RetrievedChunk[]
  /** Not present in the verified search response; kept optional. */
  citations?: SearchCitation[]
  strategy_scores: Record<string, number>
  /** A constant on this API: never display it. */
  confidence: number
}

// ---------- Evidence ----------

export type EvidenceKind = 'Chunk' | 'TreeNode' | 'Entity' | 'Relation'

export interface ProofNode {
  id: string
  kind: EvidenceKind
  label: string
  metadata: Record<string, unknown>
  children: ProofNode[]
}

export interface RawSourceRef {
  document_id: string
  version_num: number
  source_uri: string
  snapshot_uri: string
  canonical_uri: string | null
  page: number | null
  section: string | null
  block_ids: string[]
  offset_start: number
  offset_end: number
}

export interface ProofChain {
  root: ProofNode
  raw_sources: RawSourceRef[]
}

// ---------- Graph ----------

export interface GraphNode {
  id: string
  name: string
  entity_type: string
}

export interface GraphEdge {
  source: string
  target: string
  label: string
}

export interface GraphView {
  nodes: GraphNode[]
  edges: GraphEdge[]
}

// ---------- Lab: chunking and experiments ----------

export interface ChunkStrategyConfig {
  strategy: string
  params: Record<string, number>
}

export interface AnnotatedChunk {
  text: string
  char_count: number
  token_estimate: number
  /** Named for chars but the server computes it from UTF-8 byte offsets (previous end minus this start). */
  overlap_chars: number
}

export interface InspectResult {
  strategy: ChunkStrategyConfig
  chunks: AnnotatedChunk[]
  total_chunks: number
  mean_tokens: number
}

export interface BenchmarkMetrics {
  strategy: ChunkStrategyConfig
  recall_at_5: number
  recall_at_10: number
  mean_chunk_tokens: number
  chunk_size_p50: number
  chunk_size_p95: number
}

export interface PerBackendChunkConfig {
  vector: ChunkStrategyConfig
  lexical: ChunkStrategyConfig | null
  graph: ChunkStrategyConfig | null
  tree: ChunkStrategyConfig | null
}

export type ExperimentStatus = 'active' | 'ready_to_promote' | 'closed'

export interface ExperimentMetrics {
  champion_recall_at_5: number
  challenger_recall_at_5: number
  sample_size: number
  computed_at: string
}

export interface Experiment {
  experiment_id: string
  status: ExperimentStatus
  started_at: string
  challenger_config: PerBackendChunkConfig
  /** Absent on the start response; null until the first evaluation. */
  metrics?: ExperimentMetrics | null
}

export interface ExperimentEvalResult {
  status: ExperimentStatus
  metrics: ExperimentMetrics
}

export interface ExperimentSample {
  query: string
  relevant_chunk_ids: string[]
}
