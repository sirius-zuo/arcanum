import { useState } from 'react'
import { search, dominantStrategy, raptorLevel, SearchResult } from '../api/search'
import { Sparkles, Network, TreePine, Search } from 'lucide-react'

const COLLECTION = 'helix_research'

function RoutingPanel({ strategy }: { strategy: string }) {
  const map: Record<string, { icon: typeof Network; label: string; reason: string; cls: string }> = {
    Graph:  { icon: Network, label: 'Graph traversal', reason: 'Entity mentions detected', cls: 'text-purple-300 bg-purple-500/15' },
    Raptor: { icon: TreePine, label: 'Document synthesis (RAPTOR)', reason: 'Cross-document summary signal', cls: 'text-emerald-300 bg-emerald-500/15' },
    Vector: { icon: Search, label: 'Semantic search', reason: 'No strong entity/summary signal', cls: 'text-cyan-300 bg-cyan-500/15' },
    Bm25:   { icon: Search, label: 'Keyword + semantic', reason: 'Lexical match', cls: 'text-cyan-300 bg-cyan-500/15' },
  }
  const info = map[strategy] ?? map.Vector
  const Icon = info.icon
  return (
    <div className={`flex items-center gap-3 rounded-lg px-4 py-3 mb-6 ${info.cls}`}>
      <Icon size={18} />
      <div>
        <div className="text-sm font-medium">{info.label}</div>
        <div className="text-xs opacity-70">{info.reason}</div>
      </div>
    </div>
  )
}

export default function CopilotPage() {
  const [query, setQuery] = useState('')
  const [result, setResult] = useState<SearchResult | null>(null)
  const [routedTo, setRoutedTo] = useState<string | null>(null)
  const [loading, setLoading] = useState(false)
  const [error, setError] = useState<string | null>(null)

  async function ask(e: React.FormEvent) {
    e.preventDefault()
    if (!query.trim()) return
    setLoading(true); setError(null)
    try {
      const r = await search(query, COLLECTION)
      setResult(r)
      setRoutedTo(dominantStrategy(r.strategy_scores))
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
      <p className="text-slate-500 text-sm mb-6">Ask about compounds, trials, mechanisms. The classifier routes each query to the best strategy.</p>

      <form onSubmit={ask} className="mb-6">
        <div className="flex gap-3">
          <input
            value={query}
            onChange={e => setQuery(e.target.value)}
            placeholder="e.g. Does Compound 17g inhibit EGFR?  /  Summarise adverse events across Phase 2 trials"
            className="flex-1 bg-[#0f0f16] border border-slate-700 rounded-lg px-4 py-3 text-sm text-slate-200 placeholder-slate-600 outline-none focus:border-teal-500 transition"
          />
          <button type="submit" disabled={loading} className="px-5 py-3 bg-teal-600 hover:bg-teal-500 disabled:opacity-50 text-white rounded-lg text-sm font-medium transition-colors">
            {loading ? 'Thinking…' : 'Ask'}
          </button>
        </div>
      </form>

      {error && <div className="mb-4 p-3 bg-red-900/30 border border-red-700/50 rounded text-red-300 text-sm">{error}</div>}

      {routedTo && <RoutingPanel strategy={routedTo} />}

      {result && (
        <div className="space-y-3">
          {result.chunks.length === 0 && <p className="text-slate-500 text-sm">No results. Ingest the sample papers on the Corpus page first.</p>}
          {result.chunks.map((c, i) => {
            const lvl = raptorLevel(c)
            return (
              <div key={i} className="bg-[#0f0f16] border border-slate-800 rounded-lg p-4">
                <div className="flex items-center justify-between mb-2">
                  <span className="text-xs text-slate-500 font-mono">{c.indexed_chunk.chunk.collection_id}</span>
                  <div className="flex items-center gap-2">
                    {lvl && <span className="text-xs px-2 py-0.5 rounded bg-emerald-500/15 text-emerald-300 font-mono">{lvl}</span>}
                    <span className={`text-xs px-2 py-0.5 rounded font-mono ${
                      c.strategy === 'Graph' ? 'bg-purple-500/15 text-purple-300' :
                      c.strategy === 'Raptor' ? 'bg-emerald-500/15 text-emerald-300' :
                      c.strategy === 'Bm25' ? 'bg-orange-500/15 text-orange-300' :
                      'bg-cyan-500/15 text-cyan-300'
                    }`}>{c.strategy}</span>
                  </div>
                </div>
                <p className="text-sm text-slate-300 leading-relaxed whitespace-pre-wrap">
                  {c.indexed_chunk.chunk.text.slice(0, 450)}{c.indexed_chunk.chunk.text.length > 450 ? '…' : ''}
                </p>
                <div className="mt-2 text-xs text-slate-600 font-mono">score {c.score.toFixed(3)}</div>
              </div>
            )
          })}
        </div>
      )}
    </div>
  )
}
