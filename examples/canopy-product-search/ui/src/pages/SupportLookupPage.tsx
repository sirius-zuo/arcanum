import { useState } from 'react'
import { search, SearchResult } from '../api/search'
import { Search, Copy } from 'lucide-react'

const COLLECTION = 'canopy'

export default function SupportLookupPage() {
  const [query, setQuery] = useState('')
  const [result, setResult] = useState<SearchResult | null>(null)
  const [loading, setLoading] = useState(false)

  async function handleSearch(e: React.FormEvent) {
    e.preventDefault()
    if (!query.trim()) return
    setLoading(true)
    try {
      setResult(await search(query, COLLECTION))
    } finally {
      setLoading(false)
    }
  }

  function copy(text: string) {
    navigator.clipboard.writeText(text)
  }

  return (
    <div className="max-w-4xl">
      <h1 className="text-2xl font-semibold text-stone-900 mb-1">Support Lookup</h1>
      <p className="text-stone-500 text-sm mb-6">Fast lookups by SKU, model name, or issue. Optimized for support agents.</p>

      <form onSubmit={handleSearch} className="mb-6">
        <div className="flex gap-3">
          <div className="flex-1 flex items-center gap-2 bg-stone-50 border border-stone-300 rounded-lg px-4 py-2.5 focus-within:border-green-600 transition">
            <Search size={16} className="text-stone-400 flex-shrink-0" />
            <input
              value={query}
              onChange={e => setQuery(e.target.value)}
              placeholder="SKU, model name, or issue description…"
              className="flex-1 bg-transparent text-stone-800 placeholder-stone-400 outline-none text-sm font-mono"
            />
          </div>
          <button type="submit" disabled={loading} className="px-5 py-2.5 bg-stone-800 hover:bg-stone-900 disabled:opacity-50 text-white rounded-lg text-sm font-medium transition-colors">
            {loading ? 'Searching…' : 'Look up'}
          </button>
        </div>
      </form>

      {result && (
        <div className="bg-white border border-stone-200 rounded-xl overflow-hidden shadow-sm">
          <table className="w-full text-sm">
            <thead>
              <tr className="border-b border-stone-200 text-left text-stone-500 text-xs uppercase tracking-wide">
                <th className="px-4 py-2.5 font-medium w-10">#</th>
                <th className="px-4 py-2.5 font-medium">Matched text</th>
                <th className="px-4 py-2.5 font-medium w-20">Strategy</th>
                <th className="px-4 py-2.5 font-medium w-10"></th>
              </tr>
            </thead>
            <tbody>
              {result.chunks.length === 0 && (
                <tr><td colSpan={4} className="px-4 py-6 text-center text-stone-400">No matches.</td></tr>
              )}
              {result.chunks.map((c, i) => (
                <tr key={i} className="border-b border-stone-100 last:border-0 hover:bg-stone-50">
                  <td className="px-4 py-3 text-stone-400 font-mono text-xs">{i + 1}</td>
                  <td className="px-4 py-3 text-stone-700 font-mono text-xs">{c.indexed_chunk.chunk.text.slice(0, 120)}…</td>
                  <td className="px-4 py-3">
                    <span className={`text-xs px-2 py-0.5 rounded-full ${
                      c.strategy === 'Bm25' ? 'bg-orange-100 text-orange-700' : 'bg-green-100 text-green-700'
                    }`}>{c.strategy}</span>
                  </td>
                  <td className="px-4 py-3">
                    <button onClick={() => copy(c.indexed_chunk.chunk.text)} className="text-stone-400 hover:text-stone-700">
                      <Copy size={14} />
                    </button>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
    </div>
  )
}
