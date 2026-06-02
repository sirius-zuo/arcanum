import { useState } from 'react'
import { search, clauseLevel, ALL_STRATEGIES, SearchResult } from '../api/search'
import { Search } from 'lucide-react'

const COLLECTION = 'vantage_contracts'

function StrategyBars({ scores, dominant }: { scores: Record<string, number>; dominant: string }) {
  const max = Math.max(...Object.values(scores), 0.0001)
  return (
    <div className="w-36 flex-shrink-0 space-y-1.5">
      {ALL_STRATEGIES.map(s => {
        const v = scores[s] ?? 0
        const pct = (v / max) * 100
        const isDom = s === dominant
        return (
          <div key={s} className="text-xs">
            <div className="flex justify-between mb-0.5">
              <span className={isDom ? 'font-semibold text-navy' : 'text-slate-500'}>{s}</span>
              <span className="text-slate-400">{v.toFixed(2)}</span>
            </div>
            <div className="h-1.5 bg-slate-100 rounded-full overflow-hidden">
              <div
                className={`h-full ${isDom ? 'bg-navy' : 'bg-slate-300'}`}
                style={{ width: `${pct}%`, backgroundColor: isDom ? '#1e3a5f' : undefined }}
              />
            </div>
          </div>
        )
      })}
    </div>
  )
}

export default function SearchClausesPage() {
  const [query, setQuery]       = useState('')
  const [result, setResult]     = useState<SearchResult | null>(null)
  const [loading, setLoading]   = useState(false)
  const [error, setError]       = useState<string | null>(null)
  // Finding #10: toggles between 400-char excerpt and full clause text.
  const [showFull, setShowFull] = useState(false)

  function dominant(scores: Record<string, number>): string {
    const e = Object.entries(scores).sort((a, b) => b[1] - a[1])
    return e[0]?.[0] ?? ''
  }

  async function go(e: React.FormEvent) {
    e.preventDefault()
    if (!query.trim()) return
    setLoading(true); setError(null); setShowFull(false)
    try { setResult(await search(query, COLLECTION)) }
    catch (err) { setError(String(err)) }
    finally { setLoading(false) }
  }

  return (
    <div className="max-w-4xl">
      <h1 className="text-2xl text-slate-900 mb-1">Search Clauses</h1>
      <p className="text-slate-500 text-sm mb-6">
        Examples: "indemnification cap", "data residency obligations", "surviving obligations post-termination".
      </p>

      <form onSubmit={go} className="mb-6">
        <div className="flex gap-3">
          <div className="flex-1 flex items-center gap-2 bg-white border border-slate-300 rounded-lg px-4 py-3 focus-within:border-navy focus-within:ring-2 focus-within:ring-slate-100 transition">
            <Search size={16} className="text-slate-400 flex-shrink-0" />
            <input
              value={query}
              onChange={e => setQuery(e.target.value)}
              placeholder="Search clauses, obligations, parties…"
              className="flex-1 bg-transparent text-slate-800 placeholder-slate-400 outline-none text-sm"
            />
          </div>
          <button
            type="submit"
            disabled={loading}
            className="px-5 py-3 bg-slate-800 hover:bg-slate-900 disabled:opacity-50 text-white rounded-lg text-sm font-medium transition-colors"
          >
            {loading ? 'Searching…' : 'Search'}
          </button>
        </div>
      </form>

      {error && (
        <div className="mb-4 p-3 bg-red-50 border border-red-200 rounded-lg text-red-700 text-sm">{error}</div>
      )}

      {/* Finding #10: toggle appears when there are results */}
      {result && result.chunks.length > 0 && (
        <div className="flex justify-end mb-3">
          <button
            onClick={() => setShowFull(f => !f)}
            className="text-xs text-slate-500 hover:text-slate-800 underline"
          >
            {showFull ? 'Show excerpts' : 'Show full text'}
          </button>
        </div>
      )}

      {result && (
        <div className="space-y-3">
          {result.chunks.length === 0 && (
            <p className="text-slate-400 text-sm">No clauses found. Upload sample contracts on the Library page first.</p>
          )}
          {result.chunks.map((c, i) => {
            const lvl  = clauseLevel(c)
            const text = c.indexed_chunk.chunk.text
            return (
              <div key={i} className="bg-white border border-slate-200 rounded-xl p-5 shadow-sm flex gap-5">
                <div className="flex-1 min-w-0">
                  <div className="flex items-center gap-2 mb-2">
                    {lvl && (
                      <span className="text-xs px-2 py-0.5 rounded bg-emerald-50 text-emerald-700 border border-emerald-200">
                        {lvl}
                      </span>
                    )}
                    <span className="text-xs text-slate-400">{c.indexed_chunk.chunk.collection_id}</span>
                  </div>
                  {/* Finding #10: showFull shows complete clause text; default is 400-char excerpt */}
                  <p className="text-sm text-slate-700 leading-relaxed whitespace-pre-wrap">
                    {showFull ? text : text.slice(0, 400) + (text.length > 400 ? '…' : '')}
                  </p>
                </div>
                <StrategyBars scores={result.strategy_scores} dominant={dominant(result.strategy_scores)} />
              </div>
            )
          })}
        </div>
      )}
    </div>
  )
}
