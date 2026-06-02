import { useState } from 'react'
import { search, resultType, SearchResult } from '../api/search'
import { Compass } from 'lucide-react'

const COLLECTION = 'folio_library'

// Extract a display title from the first line of contextually-enriched chunk text.
function bookTitle(text: string): string {
  const first = text.split('\n')[0].replace(/^#+\s*/, '').trim()
  return first.slice(0, 80) || 'Discovery'
}

export default function DiscoverPage() {
  const [query, setQuery]     = useState('')
  const [result, setResult]   = useState<SearchResult | null>(null)
  const [loading, setLoading] = useState(false)
  const [error, setError]     = useState<string | null>(null)  // Finding #3

  async function go(e: React.FormEvent) {
    e.preventDefault()
    if (!query.trim()) return
    setLoading(true)
    setError(null)
    try {
      setResult(await search(query, COLLECTION))
    } catch (err) {
      // Finding #3: surface search errors to the patron instead of swallowing them.
      setError(String(err))
    } finally {
      setLoading(false)
    }
  }

  const chunks = result?.chunks ?? []
  // Finding #5: group by semantic purpose with L2 Book Summaries as discovery anchors.
  // Each L2 summary represents a full book's thematic footprint — the spec's intended
  // "root summary basis for thematic grouping". Other types appear as supporting sections.
  const bookAnchors   = chunks.filter(c => resultType(c) === 'Book Summary')
  const chapterChunks = chunks.filter(c => resultType(c) === 'Chapter Summary')
  const passages      = chunks.filter(c => resultType(c) === 'Passage')
  const entities      = chunks.filter(c => resultType(c) === 'Graph')

  return (
    <div className="max-w-3xl">
      <h1 className="text-2xl text-stone-900 mb-1 flex items-center gap-2">
        <Compass size={22} className="text-amber-700" /> Discover
      </h1>
      <p className="text-stone-500 text-sm mb-6">
        Explore by theme. Whole-book summaries anchor each discovery.
      </p>

      <form onSubmit={go} className="mb-8">
        <div className="flex gap-3">
          <input
            value={query}
            onChange={e => setQuery(e.target.value)}
            placeholder="Find books about… obsession and fate / cold desolate places"
            className="flex-1 bg-white border border-stone-300 rounded-xl px-4 py-3 text-sm text-stone-800 placeholder-stone-400 outline-none focus:border-amber-600 transition"
          />
          <button
            type="submit"
            disabled={loading}
            className="px-6 py-3 bg-amber-700 hover:bg-amber-800 disabled:opacity-50 text-white rounded-xl text-sm font-medium transition-colors"
          >
            {loading ? 'Exploring…' : 'Explore'}
          </button>
        </div>
      </form>

      {/* Finding #3: error banner */}
      {error && (
        <div className="mb-4 p-3 bg-red-50 border border-red-200 rounded-lg text-red-700 text-sm">{error}</div>
      )}

      {result && chunks.length === 0 && (
        <p className="text-stone-400 text-sm">Nothing yet. Upload books on My Library first.</p>
      )}

      {result && chunks.length > 0 && (
        <div className="space-y-8">

          {/* Finding #5: L2 Book Summaries as prominent thematic discovery anchors */}
          {bookAnchors.length > 0 && (
            <div className="space-y-4">
              {bookAnchors.map((c, i) => (
                <div key={i} className="bg-white border border-stone-200 rounded-xl p-5 shadow-sm">
                  <div className="text-base font-serif text-stone-900 mb-2">
                    {bookTitle(c.indexed_chunk.chunk.text)}
                  </div>
                  <p className="text-sm text-stone-600 leading-relaxed font-serif whitespace-pre-wrap">
                    {c.indexed_chunk.chunk.text.length > 400
                      ? c.indexed_chunk.chunk.text.slice(0, 400) + '…'
                      : c.indexed_chunk.chunk.text}
                  </p>
                </div>
              ))}
            </div>
          )}

          {chapterChunks.length > 0 && (
            <section>
              <h2 className="text-xs font-medium text-stone-400 uppercase tracking-wide mb-3">
                Related Chapters
              </h2>
              <div className="space-y-2">
                {chapterChunks.map((c, i) => (
                  <div key={i} className="bg-stone-50 border border-stone-100 rounded-xl p-4">
                    <p className="text-sm text-stone-600 leading-relaxed font-serif whitespace-pre-wrap">
                      {c.indexed_chunk.chunk.text.length > 300
                        ? c.indexed_chunk.chunk.text.slice(0, 300) + '…'
                        : c.indexed_chunk.chunk.text}
                    </p>
                  </div>
                ))}
              </div>
            </section>
          )}

          {passages.length > 0 && (
            <section>
              <h2 className="text-xs font-medium text-stone-400 uppercase tracking-wide mb-3">
                Relevant Passages
              </h2>
              <div className="space-y-2">
                {passages.map((c, i) => (
                  <div key={i} className="bg-stone-50 border border-stone-100 rounded-xl p-4">
                    <p className="text-sm text-stone-700 leading-relaxed font-serif whitespace-pre-wrap">
                      {c.indexed_chunk.chunk.text.length > 250
                        ? c.indexed_chunk.chunk.text.slice(0, 250) + '…'
                        : c.indexed_chunk.chunk.text}
                    </p>
                  </div>
                ))}
              </div>
            </section>
          )}

          {entities.length > 0 && (
            <section>
              <h2 className="text-xs font-medium text-stone-400 uppercase tracking-wide mb-3">
                Authors & Characters
              </h2>
              <div className="space-y-2">
                {entities.map((c, i) => (
                  <div key={i} className="bg-purple-50 border border-purple-100 rounded-xl p-4">
                    <p className="text-sm text-stone-700 font-serif">
                      {c.indexed_chunk.chunk.text.length > 200
                        ? c.indexed_chunk.chunk.text.slice(0, 200) + '…'
                        : c.indexed_chunk.chunk.text}
                    </p>
                  </div>
                ))}
              </div>
            </section>
          )}

        </div>
      )}
    </div>
  )
}
