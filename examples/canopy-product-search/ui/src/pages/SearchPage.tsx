import { useState } from 'react'
import { search, SearchResult, RetrievedChunk } from '../api/search'
import { Search } from 'lucide-react'

const COLLECTION = 'canopy'

// Extract a product name heuristically from the chunk text (first line / first sentence).
function productName(text: string): string {
  const firstLine = text.split('\n')[0].trim()
  return firstLine.length > 0 ? firstLine.slice(0, 80) : 'Product'
}

function FusionTooltip({ chunk, scores }: { chunk: RetrievedChunk; scores: Record<string, number> }) {
  const vector = scores['Vector'] ?? 0
  const bm25 = scores['Bm25'] ?? 0
  return (
    <div className="absolute z-10 hidden group-hover:block bottom-full left-0 mb-2 bg-stone-800 text-white text-xs rounded-lg px-3 py-2 shadow-lg whitespace-nowrap">
      <div className="font-medium mb-1">Fusion breakdown</div>
      <div>BM25 {bm25.toFixed(2)} · Vector {vector.toFixed(2)}</div>
      <div className="text-stone-400 mt-0.5">This result: {chunk.score.toFixed(2)} ({chunk.strategy})</div>
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
      setResult(await search(query, COLLECTION))
    } catch (err) {
      setError(String(err))
    } finally {
      setLoading(false)
    }
  }

  return (
    <div>
      <h1 className="text-2xl font-semibold text-stone-900 mb-1">Search</h1>
      <p className="text-stone-500 text-sm mb-6">Find gear by description or SKU. Both keyword and semantic search run together.</p>

      <form onSubmit={handleSearch} className="mb-8">
        <div className="flex gap-3 max-w-2xl">
          <div className="flex-1 flex items-center gap-2 bg-stone-50 border border-stone-300 rounded-xl px-4 py-3 focus-within:border-green-600 focus-within:ring-2 focus-within:ring-green-100 transition">
            <Search size={18} className="text-stone-400 flex-shrink-0" />
            <input
              value={query}
              onChange={e => setQuery(e.target.value)}
              placeholder="Search products, manuals, SKUs…"
              className="flex-1 bg-transparent text-stone-800 placeholder-stone-400 outline-none"
            />
          </div>
          <button
            type="submit"
            disabled={loading}
            className="px-6 py-3 bg-green-700 hover:bg-green-800 disabled:opacity-50 text-white rounded-xl text-sm font-medium transition-colors"
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
          {result.chunks.length === 0 && (
            <p className="text-stone-400 text-sm">
              No products found. Try "GORE-TEX jacket" or "SKU TN-4892" — and upload the sample catalogs first.
            </p>
          )}
          <div className="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 gap-4">
            {result.chunks.map((chunk, i) => (
              <div key={i} className="group relative bg-white border border-stone-200 rounded-xl p-5 shadow-sm hover:shadow-md transition-shadow">
                <div className="h-24 -mx-5 -mt-5 mb-4 rounded-t-xl bg-gradient-to-br from-green-100 to-stone-100" />
                <h3 className="text-sm font-semibold text-stone-900 mb-2">{productName(chunk.indexed_chunk.chunk.text)}</h3>
                <p className="text-xs text-stone-600 leading-relaxed line-clamp-3">
                  {chunk.indexed_chunk.chunk.text.slice(0, 160)}…
                </p>
                <div className="mt-4 flex items-center justify-between">
                  <span className={`text-xs px-2 py-0.5 rounded-full ${
                    chunk.strategy === 'Bm25' ? 'bg-orange-100 text-orange-700' : 'bg-green-100 text-green-700'
                  }`}>{chunk.strategy}</span>
                  <div className="h-1 w-20 bg-stone-100 rounded-full overflow-hidden">
                    <div className="h-full bg-green-600" style={{ width: `${Math.min(chunk.score * 100, 100)}%` }} />
                  </div>
                </div>
                <FusionTooltip chunk={chunk} scores={result.strategy_scores} />
              </div>
            ))}
          </div>
        </>
      )}
    </div>
  )
}
