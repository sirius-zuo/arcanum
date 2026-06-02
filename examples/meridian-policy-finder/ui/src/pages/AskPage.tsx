import { useState } from 'react'
import { search, dominantStrategy, SearchResult } from '../api/search'
import { prependHistory } from '../store/history'
import { Search, Sparkles, Hash } from 'lucide-react'

const COLLECTION = 'meridian'

function RoutingPill({ strategy }: { strategy: string }) {
  if (strategy === 'Bm25') {
    return (
      <span className="inline-flex items-center gap-1.5 text-xs px-2.5 py-1 rounded-full bg-orange-100 text-orange-700 border border-orange-200">
        <Hash size={12} /> Keyword lookup (BM25)
      </span>
    )
  }
  if (strategy === 'Vector') {
    return (
      <span className="inline-flex items-center gap-1.5 text-xs px-2.5 py-1 rounded-full bg-blue-100 text-blue-700 border border-blue-200">
        <Sparkles size={12} /> Semantic search
      </span>
    )
  }
  // Show the actual strategy name for Graph, Raptor, ColBert, etc.
  return (
    <span className="inline-flex items-center gap-1.5 text-xs px-2.5 py-1 rounded-full bg-slate-100 text-slate-600 border border-slate-200">
      {strategy}
    </span>
  )
}

export default function AskPage() {
  const [question, setQuestion] = useState('')
  const [result, setResult] = useState<SearchResult | null>(null)
  const [routedTo, setRoutedTo] = useState<string | null>(null)
  const [loading, setLoading] = useState(false)
  const [error, setError] = useState<string | null>(null)

  async function ask(e: React.FormEvent) {
    e.preventDefault()
    if (!question.trim()) return
    setLoading(true)
    setError(null)
    try {
      const r = await search(question, COLLECTION)
      setResult(r)
      const strat = dominantStrategy(r.strategy_scores)
      setRoutedTo(strat)
      prependHistory({
        question,
        strategy: strat ?? 'unknown',
        topTitle: r.chunks[0]?.indexed_chunk.chunk.text.slice(0, 60) ?? '(no result)',
        ts: Date.now(),
      })
    } catch (err) {
      setError(String(err))
    } finally {
      setLoading(false)
    }
  }

  return (
    <div className="max-w-3xl">
      <h1 className="text-2xl font-semibold text-slate-900 mb-1">Ask a Question</h1>
      <p className="text-slate-500 text-sm mb-6">Search company HR, IT, and benefits policies.</p>

      <form onSubmit={ask} className="mb-4">
        <div className="flex gap-3">
          <div className="flex-1 flex items-center gap-2 bg-white border border-slate-300 rounded-lg px-4 py-3 focus-within:border-blue-500 focus-within:ring-2 focus-within:ring-blue-100 transition">
            <Search size={16} className="text-slate-400 flex-shrink-0" />
            <input
              value={question}
              onChange={e => setQuestion(e.target.value)}
              placeholder="e.g. How many days of PTO do I get in year one?"
              className="flex-1 bg-transparent text-slate-800 placeholder-slate-400 outline-none text-sm"
            />
          </div>
          <button
            type="submit"
            disabled={loading}
            className="px-5 py-3 bg-blue-600 hover:bg-blue-700 disabled:opacity-50 text-white rounded-lg text-sm font-medium transition-colors"
          >
            {loading ? 'Searching…' : 'Ask'}
          </button>
        </div>
      </form>

      {routedTo && (
        <div className="mb-6 flex items-center gap-2 text-sm text-slate-500">
          <span>Routed to:</span>
          <RoutingPill strategy={routedTo} />
        </div>
      )}

      {error && (
        <div className="mb-4 p-3 bg-red-50 border border-red-200 rounded-lg text-red-700 text-sm">
          {error}
        </div>
      )}

      {result && (
        <div className="space-y-3">
          {result.chunks.length === 0 && (
            <p className="text-slate-500 text-sm">No matching policy found. Try ingesting the sample files first.</p>
          )}
          {result.chunks.map((c, i) => (
            <div key={c.indexed_chunk.chunk.id ?? i} className="bg-white border border-slate-200 rounded-xl p-5 shadow-sm">
              {i === 0 && <div className="text-xs font-medium text-blue-600 mb-2 uppercase tracking-wide">Best answer</div>}
              <p className="text-sm text-slate-700 leading-relaxed whitespace-pre-wrap">
                {c.indexed_chunk.chunk.text.slice(0, 500)}
                {c.indexed_chunk.chunk.text.length > 500 ? '…' : ''}
              </p>
              <div className="mt-3 text-xs text-slate-400">
                {c.indexed_chunk.chunk.collection_id} · score {c.score.toFixed(3)}
              </div>
            </div>
          ))}
        </div>
      )}
    </div>
  )
}
