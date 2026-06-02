import { useState } from 'react'
import { search, SearchResult, RetrievedChunk } from '../api/search'
import { Search } from 'lucide-react'

const COLLECTION = 'devforge'

function StrategyBadge({ strategy }: { strategy: RetrievedChunk['strategy'] }) {
  const styles: Record<string, string> = {
    Vector: 'bg-blue-500/20 text-blue-300 border-blue-500/30',
    Bm25:   'bg-orange-500/20 text-orange-300 border-orange-500/30',
  }
  return (
    <span className={`text-xs px-2 py-0.5 rounded border font-mono ${styles[strategy] ?? 'bg-slate-700 text-slate-300'}`}>
      {strategy}
    </span>
  )
}

function ScoreBar({ scores }: { scores: Record<string, number> }) {
  const entries = Object.entries(scores).sort((a, b) => b[1] - a[1])
  if (entries.length === 0) return null
  return (
    <div className="flex gap-4 text-xs text-slate-400 mb-4">
      {entries.map(([strategy, score]) => (
        <span key={strategy}>
          <span className="font-mono text-slate-300">{strategy}</span>{' '}
          <span className="text-slate-500">{score.toFixed(2)}</span>
        </span>
      ))}
    </div>
  )
}

export default function SearchPage() {
  const [query, setQuery] = useState('')
  const [result, setResult] = useState<SearchResult | null>(null)
  const [loading, setLoading] = useState(false)
  const [error, setError] = useState<string | null>(null)

  async function handleSearch(e: React.FormEvent) {
    e.preventDefault()
    if (!query.trim()) return
    setLoading(true)
    setError(null)
    try {
      const r = await search(query, COLLECTION)
      setResult(r)
    } catch (err) {
      setError(String(err))
    } finally {
      setLoading(false)
    }
  }

  return (
    <div className="max-w-3xl">
      <h1 className="text-2xl font-mono text-slate-100 mb-6">Search Docs</h1>

      <form onSubmit={handleSearch} className="mb-6">
        <div className="flex gap-3">
          <div className="flex-1 flex items-center gap-2 bg-[#13131f] border border-slate-700 rounded-lg px-4 py-3 focus-within:border-blue-500 transition-colors">
            <Search size={16} className="text-slate-500 flex-shrink-0" />
            <input
              value={query}
              onChange={e => setQuery(e.target.value)}
              placeholder="Search docs… e.g. 'how do I authenticate with OAuth2?'"
              className="flex-1 bg-transparent text-slate-200 placeholder-slate-500 outline-none text-sm"
            />
          </div>
          <button
            type="submit"
            disabled={loading}
            className="px-5 py-3 bg-blue-600 hover:bg-blue-500 disabled:opacity-50 text-white rounded-lg text-sm font-medium transition-colors"
          >
            {loading ? 'Searching…' : 'Search'}
          </button>
        </div>
      </form>

      {error && (
        <div className="mb-4 p-3 bg-red-900/30 border border-red-700/50 rounded text-red-300 text-sm">
          {error}
        </div>
      )}

      {result && (
        <>
          <ScoreBar scores={result.strategy_scores} />
          <div className="space-y-3">
            {result.chunks.length === 0 && (
              <p className="text-slate-500 text-sm">No results. Try ingesting some docs first.</p>
            )}
            {result.chunks.map((chunk, i) => (
              <div key={i} className="bg-[#13131f] border border-slate-800 rounded-lg p-4">
                <div className="flex items-start justify-between gap-3 mb-2">
                  <span className="text-xs text-slate-500 font-mono">
                    {chunk.indexed_chunk.chunk.collection_id}
                  </span>
                  <StrategyBadge strategy={chunk.strategy} />
                </div>
                <p className="text-sm text-slate-300 leading-relaxed whitespace-pre-wrap">
                  {chunk.indexed_chunk.chunk.text.slice(0, 400)}
                  {chunk.indexed_chunk.chunk.text.length > 400 ? '…' : ''}
                </p>
                <div className="mt-2 text-xs text-slate-600 font-mono">
                  score: {chunk.score.toFixed(3)}
                </div>
              </div>
            ))}
          </div>
        </>
      )}
    </div>
  )
}
