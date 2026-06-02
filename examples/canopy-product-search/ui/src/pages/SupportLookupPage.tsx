import { useState } from 'react'
import { search, SearchResult } from '../api/search'
import { Search, Copy } from 'lucide-react'

const COLLECTION = 'products'

function productName(text: string): string {
  const firstLine = text.split('\n')[0].replace(/^#+\s*/, '').trim()
  return firstLine.length > 0 ? firstLine.slice(0, 60) : 'Product'
}

function parseSku(text: string): string {
  const m = text.match(/\bSKU[:\s]+([A-Z]+-\d+)/i)
  return m ? m[1] : '—'
}

export default function SupportLookupPage() {
  const [query, setQuery]     = useState('')
  const [result, setResult]   = useState<SearchResult | null>(null)
  const [loading, setLoading] = useState(false)
  const [error, setError]     = useState<string | null>(null)  // Finding #6

  async function handleSearch(e: React.FormEvent) {
    e.preventDefault()
    if (!query.trim()) return
    setLoading(true)
    setError(null)
    try {
      setResult(await search(query, COLLECTION))
    } catch (err) {
      setError(String(err))   // Finding #6: surface errors to the agent
    } finally {
      setLoading(false)
    }
  }

  return (
    <div className="max-w-4xl">
      <h1 className="text-2xl font-semibold text-stone-900 mb-1">Support Lookup</h1>
      <p className="text-stone-500 text-sm mb-6">
        Fast lookups by SKU, model name, or issue. Optimised for support agents.
      </p>

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
          <button
            type="submit"
            disabled={loading}
            className="px-5 py-2.5 bg-stone-800 hover:bg-stone-900 disabled:opacity-50 text-white rounded-lg text-sm font-medium transition-colors"
          >
            {loading ? 'Searching…' : 'Look up'}
          </button>
        </div>
      </form>

      {/* Finding #6: show error instead of swallowing it */}
      {error && (
        <div className="mb-4 p-3 bg-red-50 border border-red-200 rounded-lg text-red-700 text-sm">{error}</div>
      )}

      {result && (
        <div className="bg-white border border-stone-200 rounded-xl overflow-hidden shadow-sm">
          <table className="w-full text-sm">
            <thead>
              {/* Finding #5: added SKU and Product columns */}
              <tr className="border-b border-stone-200 text-left text-stone-500 text-xs uppercase tracking-wide">
                <th className="px-4 py-2.5 font-medium w-8">#</th>
                <th className="px-4 py-2.5 font-medium w-24">SKU</th>
                <th className="px-4 py-2.5 font-medium w-40">Product</th>
                <th className="px-4 py-2.5 font-medium">Matched text</th>
                <th className="px-4 py-2.5 font-medium w-20">Strategy</th>
                <th className="px-4 py-2.5 font-medium w-10"></th>
              </tr>
            </thead>
            <tbody>
              {result.chunks.length === 0 && (
                <tr>
                  <td colSpan={6} className="px-4 py-6 text-center text-stone-400">No matches.</td>
                </tr>
              )}
              {result.chunks.map((c, i) => (
                <tr key={i} className="border-b border-stone-100 last:border-0 hover:bg-stone-50">
                  <td className="px-4 py-3 text-stone-400 font-mono text-xs">{i + 1}</td>
                  <td className="px-4 py-3 font-mono text-xs text-orange-700 font-medium">
                    {parseSku(c.indexed_chunk.chunk.text)}
                  </td>
                  <td className="px-4 py-3 text-xs text-stone-800 font-medium">
                    {productName(c.indexed_chunk.chunk.text)}
                  </td>
                  <td className="px-4 py-3 text-stone-700 font-mono text-xs">
                    {c.indexed_chunk.chunk.text.slice(0, 120)}…
                  </td>
                  <td className="px-4 py-3">
                    <span className={`text-xs px-2 py-0.5 rounded-full ${
                      c.strategy === 'Bm25' ? 'bg-orange-100 text-orange-700' : 'bg-green-100 text-green-700'
                    }`}>{c.strategy}</span>
                  </td>
                  <td className="px-4 py-3">
                    <button
                      onClick={() => navigator.clipboard.writeText(c.indexed_chunk.chunk.text)}
                      className="text-stone-400 hover:text-stone-700"
                    >
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
