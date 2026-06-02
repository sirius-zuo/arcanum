import { useState } from 'react'
import { uploadFile, ingestSample } from '../api/ingest'
import { fetchGraph, GraphNode } from '../api/graph'
import { Upload, BookOpen, CheckCircle, AlertCircle, Loader, FolderDown } from 'lucide-react'

const COLLECTION = 'folio_library'
const SAMPLE_FILES = [
  'moby-dick-opening.md',
  'hobbit-riddles-chapter.md',
  'pride-prejudice-opening.md',
  'left-hand-darkness-opening.md',
  'anna-karenina-opening.md',
]

interface Book {
  name: string
  status: 'indexing' | 'ready' | 'error'
}

// Deterministic warm gradient from a title string.
function coverGradient(title: string): string {
  let h = 0
  for (const ch of title) h = (h * 31 + ch.charCodeAt(0)) % 360
  return `linear-gradient(135deg, hsl(${h},45%,72%), hsl(${(h + 40) % 360},40%,58%))`
}

export default function LibraryPage() {
  const [books, setBooks] = useState<Book[]>([])
  const [authors, setAuthors] = useState<GraphNode[]>([])
  const [series, setSeries] = useState<GraphNode[]>([])
  // Finding #4: prevents re-entry while samples are loading.
  const [samplesLoading, setSamplesLoading] = useState(false)

  // Findings #2 & #7: internal try/catch means a graph API failure never propagates
  // to the caller — author/series display is supplemental and must not affect upload status.
  async function refreshEntities() {
    try {
      const g = await fetchGraph()
      setAuthors(g.nodes.filter(n => n.entity_type.toLowerCase() === 'author'))
      setSeries(g.nodes.filter(n => n.entity_type.toLowerCase() === 'series'))
    } catch { /* graph fetch is supplemental — don't affect upload/index status */ }
  }

  async function processFile(file: File) {
    setBooks(prev => [...prev, { name: file.name, status: 'indexing' }])
    try {
      await uploadFile(file, COLLECTION, 'full')
      setBooks(prev => prev.map(b => b.name === file.name ? { ...b, status: 'ready' } : b))
    } catch {
      setBooks(prev => prev.map(b => b.name === file.name ? { ...b, status: 'error' } : b))
      return
    }
    // Finding #2: refreshEntities is OUTSIDE the try/catch. Its internal try/catch
    // means a graph failure cannot flip a successfully-indexed book back to 'error'.
    await refreshEntities()
  }

  async function loadSamples() {
    // Finding #4: guard prevents a second concurrent load on double-click.
    if (samplesLoading) return
    setSamplesLoading(true)
    try {
      for (const name of SAMPLE_FILES) {
        setBooks(prev => [...prev, { name, status: 'indexing' }])
        try {
          await ingestSample(`samples/${name}`, COLLECTION, 'full')
          setBooks(prev => prev.map(b => b.name === name ? { ...b, status: 'ready' } : b))
        } catch {
          setBooks(prev => prev.map(b => b.name === name ? { ...b, status: 'error' } : b))
        }
      }
      // Finding #7: refreshEntities handles its own errors — safe to await without a
      // surrounding try/catch here.
      await refreshEntities()
    } finally {
      setSamplesLoading(false)
    }
  }

  function onFileInput(e: React.ChangeEvent<HTMLInputElement>) {
    Array.from(e.target.files ?? []).forEach(processFile)
  }

  const statusIcon = (s: Book['status']) => {
    if (s === 'ready') return <CheckCircle size={13} className="text-emerald-600" />
    if (s === 'error') return <AlertCircle  size={13} className="text-red-600"     />
    return <Loader size={13} className="text-amber-700 animate-spin" />
  }

  return (
    <div>
      <div className="flex items-center justify-between mb-6">
        <div>
          <h1 className="text-2xl text-stone-900">My Library</h1>
          <p className="text-stone-500 text-sm">
            Upload ePub/PDF books. The Full pipeline extracts authors, characters, and series,
            and builds whole-book summaries.
          </p>
        </div>
        <div className="flex items-center gap-3">
          {/* Finding #4: disabled while loading prevents duplicate entries */}
          <button
            onClick={loadSamples}
            disabled={samplesLoading}
            className="flex items-center gap-2 text-sm text-amber-700 hover:text-amber-800 transition-colors disabled:opacity-50 disabled:cursor-not-allowed"
          >
            <FolderDown size={15} />
            {samplesLoading ? 'Loading…' : 'Load bundled samples'}
          </button>
          <label className="cursor-pointer inline-flex items-center gap-2 px-4 py-2 bg-amber-700 hover:bg-amber-800 text-white rounded-lg text-sm font-medium transition-colors">
            <Upload size={15} /> Upload book
            <input type="file" multiple accept=".md,.txt,.epub,.pdf" className="hidden" onChange={onFileInput} />
          </label>
        </div>
      </div>

      {(authors.length > 0 || series.length > 0) && (
        <div className="flex gap-8 mb-6 text-sm">
          {authors.length > 0 && (
            <div>
              <div className="text-xs font-medium text-stone-500 uppercase tracking-wide mb-1.5">Authors</div>
              <div className="flex flex-wrap gap-2">
                {authors.map(a => (
                  <span key={a.id} className="px-2.5 py-1 rounded-full bg-white border border-stone-200 text-stone-700 text-xs">
                    {a.name}
                  </span>
                ))}
              </div>
            </div>
          )}
          {series.length > 0 && (
            <div>
              <div className="text-xs font-medium text-stone-500 uppercase tracking-wide mb-1.5">Series</div>
              <div className="flex flex-wrap gap-2">
                {series.map(s => (
                  <span key={s.id} className="px-2.5 py-1 rounded-full bg-amber-50 border border-amber-200 text-amber-800 text-xs">
                    {s.name}
                  </span>
                ))}
              </div>
            </div>
          )}
        </div>
      )}

      {books.length === 0 ? (
        <div className="text-stone-400 text-sm p-10 text-center border border-dashed border-stone-300 rounded-xl">
          No books yet. Upload the excerpts from <code>samples/</code> to get started.
        </div>
      ) : (
        <div className="grid grid-cols-2 sm:grid-cols-3 lg:grid-cols-5 gap-4">
          {books.map((b, i) => (
            // Finding #9: `group` class enables the CSS hover overlay below.
            <div key={i} className="group relative bg-white border border-stone-200 rounded-xl overflow-hidden shadow-sm">
              <div className="h-32 flex items-end p-3" style={{ background: coverGradient(b.name) }}>
                <BookOpen size={18} className="text-white/80" />
              </div>
              <div className="p-3">
                <div className="text-sm font-serif text-stone-800 truncate">
                  {b.name.replace(/\.(md|txt|epub|pdf)$/i, '')}
                </div>
                <div className="flex items-center gap-1 mt-1.5 text-xs text-stone-500">
                  {statusIcon(b.status)} {b.status}
                </div>
                {/* Finding #9: RAPTOR level badges — Full pipeline always builds L0/L1/L2 */}
                {b.status === 'ready' && (
                  <div className="flex gap-1 mt-2">
                    {(['L0', 'L1', 'L2'] as const).map(l => (
                      <span
                        key={l}
                        className="text-[10px] px-1.5 py-0.5 rounded bg-emerald-50 text-emerald-700 border border-emerald-100"
                      >
                        {l} ✓
                      </span>
                    ))}
                  </div>
                )}
              </div>
              {/* Finding #9: hover overlay summarising pipeline depth */}
              {b.status === 'ready' && (
                <div className="absolute inset-0 bg-stone-900/75 rounded-xl opacity-0 group-hover:opacity-100 transition-opacity flex flex-col items-center justify-center gap-1 pointer-events-none">
                  <div className="text-white text-xs font-medium">Full pipeline ✓</div>
                  <div className="text-white/70 text-[10px]">L0 · L1 · L2 summaries ready</div>
                </div>
              )}
            </div>
          ))}
        </div>
      )}
    </div>
  )
}
