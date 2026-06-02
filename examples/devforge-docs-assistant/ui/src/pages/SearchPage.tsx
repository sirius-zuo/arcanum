import { useState, useRef, useEffect, useCallback } from 'react'
import { search, SearchResult, RetrievedChunk } from '../api/search'
import { Search } from 'lucide-react'

const COLLECTION = 'devforge'

const SUGGESTED_QUERIES = [
  'invalid_api_key error',
  'X-RateLimit-Remaining header',
  'Bearer token format',
  'how do I authenticate with OAuth2?',
  'what happens when I hit the rate limit?',
  'getting started with the SDK',
]

const STRATEGY_LABELS: Record<string, string> = {
  Bm25: 'BM25',
  Vector: 'Vector',
  Graph: 'Graph',
  Raptor: 'Raptor',
  ColBert: 'ColBert',
}

function StrategyBadge({ strategy }: { strategy: RetrievedChunk['strategy'] }) {
  const styles: Record<string, string> = {
    Vector: 'bg-blue-500/20 text-blue-300 border-blue-500/30',
    Bm25:   'bg-orange-500/20 text-orange-300 border-orange-500/30',
  }
  return (
    <span className={`text-xs px-2 py-0.5 rounded border font-mono ${styles[strategy] ?? 'bg-slate-700 text-slate-300 border-slate-600'}`}>
      {STRATEGY_LABELS[strategy] ?? strategy}
    </span>
  )
}

function ScoreBar({ scores }: { scores: Record<string, number> }) {
  const entries = Object.entries(scores).sort((a, b) => b[1] - a[1])
  if (entries.length === 0) return null
  const max = entries[0][1]
  return (
    <div className="flex flex-col gap-1 mb-4">
      {entries.map(([strategy, score]) => (
        <div key={strategy} className="flex items-center gap-3 text-xs">
          <span className="w-14 font-mono text-slate-400 text-right shrink-0">
            {STRATEGY_LABELS[strategy] ?? strategy}
          </span>
          <div className="flex-1 bg-slate-800 rounded-full h-1.5 overflow-hidden">
            <div
              className="h-full bg-blue-500 rounded-full"
              style={{ width: `${max > 0 ? (score / max) * 100 : 0}%` }}
            />
          </div>
          <span className="font-mono text-slate-500 w-10 text-right">{score.toFixed(2)}</span>
        </div>
      ))}
    </div>
  )
}

export default function SearchPage() {
  const [query, setQuery] = useState('')
  const [result, setResult] = useState<SearchResult | null>(null)
  const [loading, setLoading] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const inputRef = useRef<HTMLInputElement>(null)

  // "/" to focus search input
  useEffect(() => {
    function onKeyDown(e: KeyboardEvent) {
      if (e.key === '/' && document.activeElement !== inputRef.current) {
        e.preventDefault()
        inputRef.current?.focus()
      }
    }
    window.addEventListener('keydown', onKeyDown)
    return () => window.removeEventListener('keydown', onKeyDown)
  }, [])

  const handleSearch = useCallback(async (e: React.FormEvent) => {
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
  }, [query])

  function runSuggestedQuery(q: string) {
    setQuery(q)
    setLoading(true)
    setError(null)
    search(q, COLLECTION)
      .then(setResult)
      .catch(err => setError(String(err)))
      .finally(() => setLoading(false))
  }

  return (
    <div className="max-w-3xl">
      <h1 className="text-2xl font-mono text-slate-100 mb-6">Search Docs</h1>

      <form onSubmit={handleSearch} className="mb-6">
        <div className="flex gap-3">
          <div className="flex-1 flex items-center gap-2 bg-[#13131f] border border-slate-700 rounded-lg px-4 py-3 focus-within:border-blue-500 transition-colors">
            <Search size={16} className="text-slate-500 flex-shrink-0" />
            <input
              ref={inputRef}
              value={query}
              onChange={e => setQuery(e.target.value)}
              placeholder="Search docs… (press / to focus)"
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

          {result.chunks.length === 0 ? (
            <div className="space-y-3">
              <p className="text-slate-500 text-sm">No results found. Try one of these:</p>
              <div className="flex flex-wrap gap-2">
                {SUGGESTED_QUERIES.map(q => (
                  <button
                    key={q}
                    onClick={() => runSuggestedQuery(q)}
                    className="text-xs px-3 py-1.5 bg-slate-800 hover:bg-slate-700 text-slate-300 rounded-md font-mono transition-colors"
                  >
                    {q}
                  </button>
                ))}
              </div>
            </div>
          ) : (
            <div className="space-y-3">
              {result.chunks.map((chunk, i) => {
                const meta = chunk.indexed_chunk.chunk.metadata
                const sourceUri = typeof meta?.source_uri === 'string'
                  ? meta.source_uri
                  : chunk.indexed_chunk.chunk.collection_id
                const fileName = sourceUri.split('/').pop() ?? sourceUri
                const charPos = chunk.indexed_chunk.chunk.position.start

                return (
                  <div key={chunk.indexed_chunk.chunk.id ?? i} className="bg-[#13131f] border border-slate-800 rounded-lg p-4">
                    <div className="flex items-start justify-between gap-3 mb-2">
                      <div className="flex flex-col gap-0.5">
                        <span className="text-xs text-slate-300 font-mono">{fileName}</span>
                        <span className="text-xs text-slate-600 font-mono">char {charPos}</span>
                      </div>
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
                )
              })}
            </div>
          )}
        </>
      )}
    </div>
  )
}
