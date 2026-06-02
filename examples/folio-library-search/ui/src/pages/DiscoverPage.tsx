import { useState } from 'react'
import { search, resultType, SearchResult, RetrievedChunk } from '../api/search'
import { Compass } from 'lucide-react'

const COLLECTION = 'folio_library'

export default function DiscoverPage() {
  const [query, setQuery] = useState('')
  const [result, setResult] = useState<SearchResult | null>(null)
  const [loading, setLoading] = useState(false)

  async function go(e: React.FormEvent) {
    e.preventDefault()
    if (!query.trim()) return
    setLoading(true)
    try { setResult(await search(query, COLLECTION)) }
    finally { setLoading(false) }
  }

  // Group results by result type so "Book Summary" matches surface as discovery anchors.
  const groups: Record<string, RetrievedChunk[]> = {}
  for (const c of result?.chunks ?? []) {
    const t = resultType(c)
    ;(groups[t] ??= []).push(c)
  }
  // Prefer Book Summary first, then Chapter Summary, then Passage.
  const order = ['Book Summary', 'Chapter Summary', 'Passage', 'Graph', 'Match']

  return (
    <div className="max-w-3xl">
      <h1 className="text-2xl text-stone-900 mb-1 flex items-center gap-2"><Compass size={22} className="text-amber-700" /> Discover</h1>
      <p className="text-stone-500 text-sm mb-6">Explore by theme. Whole-book summaries anchor each discovery.</p>

      <form onSubmit={go} className="mb-8">
        <div className="flex gap-3">
          <input
            value={query}
            onChange={e => setQuery(e.target.value)}
            placeholder="Find books about… obsession and fate / cold desolate places"
            className="flex-1 bg-white border border-stone-300 rounded-xl px-4 py-3 text-sm text-stone-800 placeholder-stone-400 outline-none focus:border-amber-600 transition"
          />
          <button type="submit" disabled={loading} className="px-6 py-3 bg-amber-700 hover:bg-amber-800 disabled:opacity-50 text-white rounded-xl text-sm font-medium transition-colors">
            {loading ? 'Exploring…' : 'Explore'}
          </button>
        </div>
      </form>

      {result && (
        <div className="space-y-8">
          {(result.chunks.length === 0) && <p className="text-stone-400 text-sm">Nothing yet. Upload books on My Library first.</p>}
          {order.filter(t => groups[t]?.length).map(type => (
            <section key={type}>
              <h2 className="text-sm font-medium text-stone-500 uppercase tracking-wide mb-3">{type}</h2>
              <div className="space-y-3">
                {groups[type].map((c, i) => (
                  <div key={i} className="bg-white border border-stone-200 rounded-xl p-5 shadow-sm">
                    <p className="text-sm text-stone-700 leading-relaxed font-serif whitespace-pre-wrap">
                      {c.indexed_chunk.chunk.text.slice(0, 400)}{c.indexed_chunk.chunk.text.length > 400 ? '…' : ''}
                    </p>
                  </div>
                ))}
              </div>
            </section>
          ))}
        </div>
      )}
    </div>
  )
}
