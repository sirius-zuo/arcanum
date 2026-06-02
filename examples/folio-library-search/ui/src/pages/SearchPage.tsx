import { useState } from 'react'
import { search, resultType, ResultType, SearchResult } from '../api/search'
import { Search } from 'lucide-react'

const COLLECTION = 'folio_library'

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

export default function SearchPage() {
  const [query, setQuery] = useState('')
  const [result, setResult] = useState<SearchResult | null>(null)
  const [loading, setLoading] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [filter, setFilter] = useState<ResultType | 'All'>('All')

  async function go(e: React.FormEvent) {
    e.preventDefault()
    if (!query.trim()) return
    setLoading(true); setError(null)
    try { setResult(await search(query, COLLECTION)) }
    catch (err) { setError(String(err)) }
    finally { setLoading(false) }
  }

  const chunks = result?.chunks ?? []
  const visible = filter === 'All' ? chunks : chunks.filter(c => resultType(c) === filter)

  return (
    <div>
      <h1 className="text-2xl text-stone-900 mb-1">Search</h1>
      <p className="text-stone-500 text-sm mb-6">Search by passage, author, character, or theme. Results mix passages, summaries, and graph matches.</p>

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
          <button type="submit" disabled={loading} className="px-6 py-3 bg-amber-700 hover:bg-amber-800 disabled:opacity-50 text-white rounded-xl text-sm font-medium transition-colors">
            {loading ? 'Searching…' : 'Search'}
          </button>
        </div>
      </form>

      {error && <div className="mb-4 p-3 bg-red-50 border border-red-200 rounded-lg text-red-700 text-sm">{error}</div>}

      {result && (
        <>
          <div className="flex gap-2 mb-5">
            {(['All', 'Passage', 'Chapter Summary', 'Book Summary', 'Graph'] as const).map(f => (
              <button
                key={f}
                onClick={() => setFilter(f)}
                className={`px-3 py-1.5 rounded-full text-xs transition-colors ${
                  filter === f ? 'bg-amber-700 text-white' : 'bg-white border border-stone-200 text-stone-600 hover:bg-stone-50'
                }`}
              >{f}</button>
            ))}
          </div>

          <div className="space-y-3 max-w-3xl">
            {visible.length === 0 && <p className="text-stone-400 text-sm">No results. Upload books on My Library first.</p>}
            {visible.map((c, i) => {
              const type = resultType(c)
              return (
                <div key={i} className="bg-white border border-stone-200 rounded-xl p-5 shadow-sm">
                  <div className="flex items-center gap-2 mb-2">
                    <TypeBadge type={type} />
                    <span className="text-xs text-stone-400">score {c.score.toFixed(3)}</span>
                  </div>
                  <p className="text-sm text-stone-700 leading-relaxed whitespace-pre-wrap font-serif">
                    {c.indexed_chunk.chunk.text.slice(0, 450)}{c.indexed_chunk.chunk.text.length > 450 ? '…' : ''}
                  </p>
                </div>
              )
            })}
          </div>
        </>
      )}
    </div>
  )
}
