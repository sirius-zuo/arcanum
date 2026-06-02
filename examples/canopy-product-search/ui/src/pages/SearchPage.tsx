import { useState } from 'react'
import { search, SearchResult, RetrievedChunk } from '../api/search'
import { Search } from 'lucide-react'

// Changed from 'canopy' → 'products' to match the spec's collection names.
// Re-run "Load bundled samples" in the Catalog page if you have existing data.
const COLLECTION = 'products'

const CATEGORIES = ['Jackets', 'Tents', 'Sleeping Bags', 'Accessories', 'Policies'] as const

// Keyword used to filter chunk text by category.
const CATEGORY_KEYWORDS: Record<string, string> = {
  'Jackets':       'jacket',
  'Tents':         'tent',
  'Sleeping Bags': 'sleeping bag',
  'Accessories':   'accessor',
  'Policies':      'policy',
}

const SAMPLE_QUERIES = [
  'best jacket for winter hiking',
  'lightweight tent for solo backpacking',
  'SKU TN-4892',
  'warranty claim process',
]

function productName(text: string): string {
  const firstLine = text.split('\n')[0].replace(/^#+\s*/, '').trim()
  return firstLine.length > 0 ? firstLine.slice(0, 80) : 'Product'
}

function parseSku(text: string): string {
  const m = text.match(/\bSKU[:\s]+([A-Z]+-\d+)/i)
  return m ? m[1] : '—'
}

function StrategyBadge({ strategy }: { strategy: string }) {
  return (
    <span className={`text-xs px-2 py-0.5 rounded-full ${
      strategy === 'Bm25' ? 'bg-orange-100 text-orange-700' : 'bg-green-100 text-green-700'
    }`}>{strategy}</span>
  )
}

// Findings #1 & #2: show Combined (chunk.score) inline; label clarifies BM25/Vector are
// response-level averages, not per-chunk breakdowns.
function FusionTooltip({ chunk, scores }: { chunk: RetrievedChunk; scores: Record<string, number> }) {
  const vector = scores['Vector'] ?? 0
  const bm25   = scores['Bm25']   ?? 0
  return (
    <div className="absolute z-10 hidden group-hover:block bottom-full left-0 mb-2 bg-stone-800 text-white text-xs rounded-lg px-3 py-2 shadow-lg whitespace-nowrap">
      <div className="font-medium mb-1">Fusion scores</div>
      <div>BM25 {bm25.toFixed(2)} · Vector {vector.toFixed(2)} · Combined {chunk.score.toFixed(2)}</div>
      <div className="text-stone-400 mt-0.5">Dominant strategy: {chunk.strategy}</div>
    </div>
  )
}

export default function SearchPage() {
  const [query, setQuery]                   = useState('')
  const [result, setResult]                 = useState<SearchResult | null>(null)
  const [loading, setLoading]               = useState(false)
  const [error, setError]                   = useState<string | null>(null)
  const [viewMode, setViewMode]             = useState<'customer' | 'support'>('customer')
  const [filterCategory, setFilterCategory] = useState<string | null>(null)
  const [sortOrder, setSortOrder]           = useState<'relevance' | 'alpha'>('relevance')

  async function handleSearch(e: React.FormEvent) {
    e.preventDefault()
    if (!query.trim()) return
    setLoading(true)
    setError(null)
    setFilterCategory(null)
    try {
      setResult(await search(query, COLLECTION))
    } catch (err) {
      setError(String(err))
    } finally {
      setLoading(false)
    }
  }

  const baseChunks = result?.chunks ?? []
  const filteredChunks = filterCategory
    ? baseChunks.filter(c =>
        c.indexed_chunk.chunk.text.toLowerCase().includes(
          CATEGORY_KEYWORDS[filterCategory] ?? filterCategory.toLowerCase()
        )
      )
    : baseChunks
  const displayChunks = sortOrder === 'alpha'
    ? [...filteredChunks].sort((a, b) =>
        productName(a.indexed_chunk.chunk.text).localeCompare(
          productName(b.indexed_chunk.chunk.text)
        )
      )
    : filteredChunks

  return (
    <div>
      <h1 className="text-2xl font-semibold text-stone-900 mb-1">Search</h1>
      <p className="text-stone-500 text-sm mb-6">
        Find gear by description or SKU. Both keyword and semantic search run together.
      </p>

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

      {/* Finding #8: pre-search empty state with sample query chips */}
      {result === null && !loading && (
        <div className="text-center py-12">
          <p className="text-stone-500 text-sm mb-4">
            Try searching for <strong>'GORE-TEX jacket'</strong> or <strong>'SKU TN-4892'</strong>:
          </p>
          <div className="flex flex-wrap gap-2 justify-center">
            {SAMPLE_QUERIES.map(q => (
              <button
                key={q}
                onClick={() => setQuery(q)}
                className="px-3 py-1.5 bg-stone-100 hover:bg-stone-200 text-stone-700 rounded-full text-sm transition-colors"
              >
                {q}
              </button>
            ))}
          </div>
        </div>
      )}

      {/* Post-search: filter sidebar + results */}
      {result !== null && (
        <div className="flex gap-6">

          {/* Finding #9: category filter + sort sidebar */}
          <aside className="w-44 flex-shrink-0 space-y-6">
            <div>
              <p className="text-xs font-semibold text-stone-500 uppercase tracking-wide mb-2">Category</p>
              <div className="space-y-0.5">
                <button
                  onClick={() => setFilterCategory(null)}
                  className={`w-full text-left text-sm px-2 py-1.5 rounded-lg transition-colors ${
                    filterCategory === null ? 'bg-green-50 text-green-800 font-medium' : 'text-stone-600 hover:bg-stone-100'
                  }`}
                >
                  All
                </button>
                {CATEGORIES.map(cat => (
                  <button
                    key={cat}
                    onClick={() => setFilterCategory(filterCategory === cat ? null : cat)}
                    className={`w-full text-left text-sm px-2 py-1.5 rounded-lg transition-colors ${
                      filterCategory === cat ? 'bg-green-50 text-green-800 font-medium' : 'text-stone-600 hover:bg-stone-100'
                    }`}
                  >
                    {cat}
                  </button>
                ))}
              </div>
            </div>
            <div>
              <p className="text-xs font-semibold text-stone-500 uppercase tracking-wide mb-2">Sort</p>
              <div className="space-y-0.5">
                {(['relevance', 'alpha'] as const).map(order => (
                  <button
                    key={order}
                    onClick={() => setSortOrder(order)}
                    className={`w-full text-left text-sm px-2 py-1.5 rounded-lg transition-colors ${
                      sortOrder === order ? 'bg-green-50 text-green-800 font-medium' : 'text-stone-600 hover:bg-stone-100'
                    }`}
                  >
                    {order === 'relevance' ? 'Relevance' : 'Alphabetical'}
                  </button>
                ))}
              </div>
            </div>
          </aside>

          {/* Results area */}
          <div className="flex-1 min-w-0">

            {/* Finding #3: Customer / Support view toggle */}
            <div className="flex items-center gap-2 mb-4">
              <span className="text-xs text-stone-500">View:</span>
              {(['customer', 'support'] as const).map(mode => (
                <button
                  key={mode}
                  onClick={() => setViewMode(mode)}
                  className={`px-3 py-1 text-xs rounded-full border transition-colors ${
                    viewMode === mode
                      ? 'bg-green-700 text-white border-green-700'
                      : 'border-stone-300 text-stone-600 hover:border-stone-400'
                  }`}
                >
                  {mode === 'customer' ? 'Customer view' : 'Support view'}
                </button>
              ))}
              <span className="text-xs text-stone-400 ml-2">
                {displayChunks.length} result{displayChunks.length !== 1 ? 's' : ''}
              </span>
            </div>

            {displayChunks.length === 0 && (
              <p className="text-stone-400 text-sm">
                No products found. Try "GORE-TEX jacket" or "SKU TN-4892" — and upload the sample catalogs first.
              </p>
            )}

            {/* Customer view — product card grid */}
            {viewMode === 'customer' && displayChunks.length > 0 && (
              <div className="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 gap-4">
                {displayChunks.map((chunk, i) => (
                  <div key={i} className="group relative bg-white border border-stone-200 rounded-xl p-5 shadow-sm hover:shadow-md transition-shadow">
                    <div className="h-24 -mx-5 -mt-5 mb-4 rounded-t-xl bg-gradient-to-br from-green-100 to-stone-100" />
                    <h3 className="text-sm font-semibold text-stone-900 mb-2">
                      {productName(chunk.indexed_chunk.chunk.text)}
                    </h3>
                    <p className="text-xs text-stone-600 leading-relaxed line-clamp-3">
                      {chunk.indexed_chunk.chunk.text.length > 160
                        ? chunk.indexed_chunk.chunk.text.slice(0, 160) + '…'
                        : chunk.indexed_chunk.chunk.text}
                    </p>
                    <div className="mt-4 flex items-center justify-between">
                      <StrategyBadge strategy={chunk.strategy} />
                      <div className="h-1 w-20 bg-stone-100 rounded-full overflow-hidden">
                        <div className="h-full bg-green-600" style={{ width: `${Math.min(chunk.score * 100, 100)}%` }} />
                      </div>
                    </div>
                    <FusionTooltip chunk={chunk} scores={result.strategy_scores} />
                  </div>
                ))}
              </div>
            )}

            {/* Support view — compact table with SKU column prominent */}
            {viewMode === 'support' && displayChunks.length > 0 && (
              <div className="bg-white border border-stone-200 rounded-xl overflow-hidden shadow-sm">
                <table className="w-full text-sm">
                  <thead>
                    <tr className="border-b border-stone-200 text-left text-stone-500 text-xs uppercase tracking-wide">
                      <th className="px-4 py-2.5 font-medium w-8">#</th>
                      <th className="px-4 py-2.5 font-medium w-24">SKU</th>
                      <th className="px-4 py-2.5 font-medium w-40">Product</th>
                      <th className="px-4 py-2.5 font-medium">Excerpt</th>
                      <th className="px-4 py-2.5 font-medium w-20">Strategy</th>
                    </tr>
                  </thead>
                  <tbody>
                    {displayChunks.map((chunk, i) => (
                      <tr key={i} className="border-b border-stone-100 last:border-0 hover:bg-stone-50">
                        <td className="px-4 py-3 text-stone-400 font-mono text-xs">{i + 1}</td>
                        <td className="px-4 py-3 font-mono text-xs text-orange-700 font-medium">
                          {parseSku(chunk.indexed_chunk.chunk.text)}
                        </td>
                        <td className="px-4 py-3 text-xs text-stone-800 font-medium">
                          {productName(chunk.indexed_chunk.chunk.text)}
                        </td>
                        <td className="px-4 py-3 text-xs text-stone-600 font-mono">
                          {chunk.indexed_chunk.chunk.text.slice(0, 100)}…
                        </td>
                        <td className="px-4 py-3">
                          <StrategyBadge strategy={chunk.strategy} />
                        </td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>
            )}

          </div>
        </div>
      )}
    </div>
  )
}
