import { useState } from 'react'
import { search, resultType, ResultType, SearchResult } from '../api/search'
import { Search } from 'lucide-react'

const COLLECTION = 'folio_library'

// Finding #6: colours for each strategy in the contribution footer.
const STRATEGY_COLORS: Record<string, string> = {
  Vector: '#0891b2',  // cyan
  Bm25:   '#d97706',  // amber
  Graph:  '#9333ea',  // purple
  Raptor: '#059669',  // emerald
}

function TypeBadge({ type }: { type: ResultType }) {
  const styles: Record<ResultType, string> = {
    'Passage':         'bg-amber-100 text-amber-800',
    'Chapter Summary': 'bg-teal-100 text-teal-800',
    'Book Summary':    'bg-emerald-100 text-emerald-800',
    'Graph':           'bg-purple-100 text-purple-800',
    'Match':           'bg-stone-100 text-stone-700',
  }
  return <span className={`text-xs px-2 py-0.5 rounded-full ${styles[type]}`}>{type}</span>
}

// Finding #6: small coloured dots + labels showing which strategies contributed.
// Only strategies with >= 10% relative contribution are shown to avoid clutter.
function StrategyFooter({ scores }: { scores: Record<string, number> }) {
  const max = Math.max(...Object.values(scores), 0.001)
  const contributors = Object.entries(scores)
    .filter(([, v]) => v / max >= 0.1)
    .sort((a, b) => b[1] - a[1])
  if (contributors.length === 0) return null
  return (
    <div className="mt-3 pt-3 border-t border-stone-100 flex items-center gap-3 flex-wrap">
      <span className="text-[10px] text-stone-400 uppercase tracking-wide">Via:</span>
      {contributors.map(([s, v]) => {
        const rel = v / max
        const color = STRATEGY_COLORS[s] ?? '#94a3b8'
        return (
          <div key={s} className="flex items-center gap-1">
            <div
              className="w-2 h-2 rounded-full flex-shrink-0"
              style={{ backgroundColor: color, opacity: 0.35 + rel * 0.65 }}
            />
            <span
              className="text-[10px]"
              style={{ color, opacity: 0.45 + rel * 0.55 }}
            >
              {s}
            </span>
          </div>
        )
      })}
    </div>
  )
}

export default function SearchPage() {
  const [query, setQuery]           = useState('')
  const [result, setResult]         = useState<SearchResult | null>(null)
  const [loading, setLoading]       = useState(false)
  const [error, setError]           = useState<string | null>(null)
  const [filter, setFilter]         = useState<ResultType | 'All'>('All')
  // Finding #10: text filter applied on top of type filter.
  const [bookFilter, setBookFilter] = useState('')

  async function go(e: React.FormEvent) {
    e.preventDefault()
    if (!query.trim()) return
    // Reset the book filter on each new search.
    setLoading(true); setError(null); setBookFilter('')
    try { setResult(await search(query, COLLECTION)) }
    catch (err) { setError(String(err)) }
    finally { setLoading(false) }
  }

  const chunks = result?.chunks ?? []
  const visible = chunks
    .filter(c => filter === 'All' || resultType(c) === filter)
    // Finding #10: chunk text is enriched with book title + chapter, so title/author
    // text appears in the chunk body — simple substring filter is effective.
    .filter(c =>
      !bookFilter.trim() ||
      c.indexed_chunk.chunk.text.toLowerCase().includes(bookFilter.toLowerCase())
    )

  // Capture strategy_scores once to avoid null-check inside the map callback.
  const strategyScores = result?.strategy_scores ?? {}

  return (
    <div>
      <h1 className="text-2xl text-stone-900 mb-1">Search</h1>
      <p className="text-stone-500 text-sm mb-6">
        Search by passage, author, character, or theme. Results mix passages, summaries, and graph matches.
      </p>

      <form onSubmit={go} className="mb-6">
        <div className="flex gap-3 max-w-2xl">
          <div className="flex-1 flex items-center gap-2 bg-white border border-stone-300 rounded-xl px-4 py-3 focus-within:border-amber-600 focus-within:ring-2 focus-within:ring-amber-100 transition">
            <Search size={18} className="text-stone-400 flex-shrink-0" />
            <input
              value={query}
              onChange={e => setQuery(e.target.value)}
              placeholder="Search by passage, author, character, or theme…"
              className="flex-1 bg-transparent text-stone-800 placeholder-stone-400 outline-none"
            />
          </div>
          <button
            type="submit"
            disabled={loading}
            className="px-6 py-3 bg-amber-700 hover:bg-amber-800 disabled:opacity-50 text-white rounded-xl text-sm font-medium transition-colors"
          >
            {loading ? 'Searching…' : 'Search'}
          </button>
        </div>
      </form>

      {error && (
        <div className="mb-4 p-3 bg-red-50 border border-red-200 rounded-lg text-red-700 text-sm">{error}</div>
      )}

      {result && (
        <>
          {/* Result-type filter pills */}
          <div className="flex gap-2 mb-3 flex-wrap">
            {(['All', 'Passage', 'Chapter Summary', 'Book Summary', 'Graph'] as const).map(f => (
              <button
                key={f}
                onClick={() => setFilter(f)}
                className={`px-3 py-1.5 rounded-full text-xs transition-colors ${
                  filter === f
                    ? 'bg-amber-700 text-white'
                    : 'bg-white border border-stone-200 text-stone-600 hover:bg-stone-50'
                }`}
              >{f}</button>
            ))}
          </div>

          {/* Finding #10: book title / author text filter */}
          <div className="flex items-center gap-2 mb-5">
            <span className="text-xs text-stone-400">Filter by title or author:</span>
            <input
              value={bookFilter}
              onChange={e => setBookFilter(e.target.value)}
              placeholder="e.g. Moby Dick, Tolkien…"
              className="px-2.5 py-1 text-xs border border-stone-200 rounded-lg outline-none focus:border-amber-500 bg-white text-stone-700 w-48"
            />
            {bookFilter && (
              <button
                onClick={() => setBookFilter('')}
                className="text-xs text-stone-400 hover:text-stone-600"
              >
                Clear
              </button>
            )}
          </div>

          <div className="space-y-3 max-w-3xl">
            {visible.length === 0 && (
              <p className="text-stone-400 text-sm">
                No results{bookFilter ? ` matching "${bookFilter}"` : ''}. Upload books on My Library first.
              </p>
            )}
            {visible.map((c, i) => {
              const type = resultType(c)
              return (
                <div key={i} className="bg-white border border-stone-200 rounded-xl p-5 shadow-sm">
                  <div className="flex items-center gap-2 mb-2">
                    <TypeBadge type={type} />
                    <span className="text-xs text-stone-400">score {c.score.toFixed(3)}</span>
                  </div>
                  <p className="text-sm text-stone-700 leading-relaxed whitespace-pre-wrap font-serif">
                    {c.indexed_chunk.chunk.text.length > 450
                      ? c.indexed_chunk.chunk.text.slice(0, 450) + '…'
                      : c.indexed_chunk.chunk.text}
                  </p>
                  {/* Finding #6: strategy contribution footer */}
                  <StrategyFooter scores={strategyScores} />
                </div>
              )
            })}
          </div>
        </>
      )}
    </div>
  )
}
