import { useState } from 'react'
import { search, dominantStrategy, raptorLevel, SearchResult } from '../api/search'
import { Sparkles, Network, TreePine, Search } from 'lucide-react'

const COLLECTION = 'helix_research'

// Finding #6: derives a query-specific explanation rather than a static string.
// Uses lightweight client-side heuristics on the query text.
function classifierReason(query: string, strategy: string): string {
  const q = query.toLowerCase()
  if (strategy === 'Graph') {
    return /\bcompound\s+\d|\begfr\b|\bjak[12]?\b|\bstat[0-9]?\b|\bcrispr\b/i.test(query)
      ? 'Entity names detected → Graph traversal'
      : 'Entity mentions detected → Graph traversal'
  }
  if (strategy === 'Raptor') {
    return /\b(summarise|summarize|across all|overview|throughout|all (phase|trial))\b/.test(q)
      ? 'Synthesis signal detected → RAPTOR'
      : 'Cross-document summary signal → RAPTOR'
  }
  return 'No strong entity/synthesis signals → Semantic search'
}

// Finding #6: accepts query so classifierReason can inspect it.
function RoutingPanel({ strategy, query }: { strategy: string; query: string }) {
  const map: Record<string, { icon: typeof Network; label: string; cls: string }> = {
    Graph:  { icon: Network,  label: 'Graph traversal',            cls: 'text-purple-300 bg-purple-500/15'  },
    Raptor: { icon: TreePine, label: 'Document synthesis (RAPTOR)', cls: 'text-emerald-300 bg-emerald-500/15' },
    Vector: { icon: Search,   label: 'Semantic search',             cls: 'text-cyan-300 bg-cyan-500/15'      },
    Bm25:   { icon: Search,   label: 'Keyword + semantic',          cls: 'text-cyan-300 bg-cyan-500/15'      },
  }
  const info = map[strategy] ?? map.Vector
  const Icon = info.icon
  return (
    <div className={`flex items-center gap-3 rounded-lg px-4 py-3 mb-6 ${info.cls}`}>
      <Icon size={18} />
      <div>
        <div className="text-sm font-medium">{info.label}</div>
        <div className="text-xs opacity-70">{classifierReason(query, strategy)}</div>
      </div>
    </div>
  )
}

// Finding #7: returns the entity path string from chunk metadata if the server includes it.
// Checks common key names the arcanum-server graph retriever may use.
function graphPath(metadata: Record<string, unknown>): string | null {
  const path = metadata['entity_path'] ?? metadata['graph_path'] ?? metadata['path']
  return typeof path === 'string' && path.length > 0 ? path : null
}

// Finding #10: one horizontal bar representing a single strategy's relative score.
function ScoreBar({ strategy, score, maxScore }: { strategy: string; score: number; maxScore: number }) {
  const pct = maxScore > 0 ? Math.round((score / maxScore) * 100) : 0
  const color =
    strategy === 'Graph'  ? '#a78bfa' :
    strategy === 'Raptor' ? '#34d399' :
    strategy === 'Bm25'   ? '#f97316' :
                            '#22d3ee'
  return (
    <div className="flex items-center gap-1.5">
      <span className="text-[10px] text-slate-500 font-mono w-12 text-right shrink-0">{strategy}</span>
      <div className="w-12 h-1 bg-slate-800 rounded-full overflow-hidden">
        <div className="h-full rounded-full" style={{ width: `${pct}%`, backgroundColor: color }} />
      </div>
    </div>
  )
}

export default function CopilotPage() {
  const [query, setQuery]     = useState('')
  const [result, setResult]   = useState<SearchResult | null>(null)
  const [loading, setLoading] = useState(false)
  const [error, setError]     = useState<string | null>(null)

  // Finding #8: routedTo derived from result instead of stored as independent state.
  // Eliminates the desync risk; dominantStrategy now guards against undefined scores.
  const routedTo = result ? dominantStrategy(result.strategy_scores) : null

  async function ask(e: React.FormEvent) {
    e.preventDefault()
    if (!query.trim()) return
    setLoading(true); setError(null)
    try {
      setResult(await search(query, COLLECTION))
    } catch (err) {
      setError(String(err))
    } finally {
      setLoading(false)
    }
  }

  return (
    <div className="max-w-3xl">
      <h1 className="text-2xl font-semibold text-slate-100 mb-1 flex items-center gap-2">
        <Sparkles size={22} className="text-teal-400" /> Research Copilot
      </h1>
      <p className="text-slate-500 text-sm mb-6">
        Ask about compounds, trials, mechanisms. The classifier routes each query to the best strategy.
      </p>

      <form onSubmit={ask} className="mb-6">
        <div className="flex gap-3">
          <input
            value={query}
            onChange={e => setQuery(e.target.value)}
            placeholder="e.g. Does Compound 17g inhibit EGFR?  /  Summarise adverse events across Phase 2 trials"
            className="flex-1 bg-[#0f0f16] border border-slate-700 rounded-lg px-4 py-3 text-sm text-slate-200 placeholder-slate-600 outline-none focus:border-teal-500 transition"
          />
          <button
            type="submit"
            disabled={loading}
            className="px-5 py-3 bg-teal-600 hover:bg-teal-500 disabled:opacity-50 text-white rounded-lg text-sm font-medium transition-colors"
          >
            {loading ? 'Thinking…' : 'Ask'}
          </button>
        </div>
      </form>

      {error && (
        <div className="mb-4 p-3 bg-red-900/30 border border-red-700/50 rounded text-red-300 text-sm">{error}</div>
      )}

      {/* Finding #6: query passed for dynamic reason string */}
      {routedTo && <RoutingPanel strategy={routedTo} query={query} />}

      {result && (
        <div className="space-y-3">
          {result.chunks.length === 0 && (
            <p className="text-slate-500 text-sm">No results. Ingest the sample papers on the Corpus page first.</p>
          )}
          {result.chunks.map((c, i) => {
            const lvl  = raptorLevel(c)
            const path = graphPath(c.indexed_chunk.chunk.metadata)
            const scoreEntries = Object.entries(result.strategy_scores ?? {})
            const maxScore = Math.max(...scoreEntries.map(([, v]) => v), 0.001)
            return (
              <div key={i} className="bg-[#0f0f16] border border-slate-800 rounded-lg p-4">
                <div className="flex items-center justify-between mb-2">
                  <span className="text-xs text-slate-500 font-mono">{c.indexed_chunk.chunk.collection_id}</span>
                  <div className="flex items-center gap-2">
                    {lvl && (
                      <span className="text-xs px-2 py-0.5 rounded bg-emerald-500/15 text-emerald-300 font-mono">{lvl}</span>
                    )}
                    <span className={`text-xs px-2 py-0.5 rounded font-mono ${
                      c.strategy === 'Graph'  ? 'bg-purple-500/15 text-purple-300'  :
                      c.strategy === 'Raptor' ? 'bg-emerald-500/15 text-emerald-300' :
                      c.strategy === 'Bm25'   ? 'bg-orange-500/15 text-orange-300'  :
                                                'bg-cyan-500/15 text-cyan-300'
                    }`}>{c.strategy}</span>
                  </div>
                </div>

                <p className="text-sm text-slate-300 leading-relaxed whitespace-pre-wrap">
                  {c.indexed_chunk.chunk.text.slice(0, 450)}
                  {c.indexed_chunk.chunk.text.length > 450 ? '…' : ''}
                </p>

                {/* Finding #7: entity path for Graph results, shown when metadata includes it */}
                {c.strategy === 'Graph' && path && (
                  <div className="mt-2 text-xs text-purple-300 font-mono">{path}</div>
                )}

                <div className="mt-3 flex items-end justify-between gap-4">
                  <div className="text-xs text-slate-600 font-mono">score {c.score.toFixed(3)}</div>
                  {/* Finding #10: strategy score mini bar chart */}
                  {scoreEntries.length > 0 && (
                    <div className="space-y-0.5">
                      {scoreEntries.map(([strat, score]) => (
                        <ScoreBar key={strat} strategy={strat} score={score} maxScore={maxScore} />
                      ))}
                    </div>
                  )}
                </div>
              </div>
            )
          })}
        </div>
      )}
    </div>
  )
}
