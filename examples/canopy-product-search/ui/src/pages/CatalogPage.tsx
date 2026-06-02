import { useRef, useState } from 'react'
import { uploadFile, ingestSample } from '../api/ingest'
import { Upload, Package, CheckCircle, AlertCircle, Loader, FolderDown } from 'lucide-react'

const COLLECTIONS = [
  { id: 'products', label: 'Products' },
  { id: 'manuals',  label: 'Manuals'  },
  { id: 'policies', label: 'Policies' },
] as const

type CollectionId = typeof COLLECTIONS[number]['id']

// Each bundled sample belongs to a specific collection.
// Return policy goes into 'products' so it is found by the Search page's default query.
const SAMPLE_FILE_COLLECTIONS: Record<string, CollectionId> = {
  'jackets-catalog.md':       'products',
  'tents-catalog.md':         'products',
  'sleeping-bags-catalog.md': 'products',
  'return-policy.md':         'products',
}

const SAMPLE_FILES = Object.keys(SAMPLE_FILE_COLLECTIONS) as (keyof typeof SAMPLE_FILE_COLLECTIONS)[]

interface CatalogDoc {
  id:         number
  name:       string
  collection: CollectionId
  status:     'indexing' | 'ready' | 'error'
}

export default function CatalogPage() {
  const [collection, setCollection] = useState<CollectionId>('products')
  const [docs, setDocs]             = useState<CatalogDoc[]>([])
  // useRef so the counter is stable across re-renders and doesn't reset on each render.
  const nextId = useRef(0)

  // Single ingest helper used by both file uploads and sample loads.
  // Matches doc entries by numeric id, not filename, so duplicate filenames don't collide.
  async function ingestDoc(
    name: string,
    apiFn: () => Promise<unknown>,
    targetCollection: CollectionId,
  ) {
    const id = nextId.current++
    setDocs(prev => [...prev, { id, name, collection: targetCollection, status: 'indexing' }])
    try {
      await apiFn()
      setDocs(prev => prev.map(d => d.id === id ? { ...d, status: 'ready' } : d))
    } catch {
      setDocs(prev => prev.map(d => d.id === id ? { ...d, status: 'error' } : d))
    }
  }

  function handleFiles(files: File[]) {
    // Fire all uploads concurrently; each one independently tracks its own status row.
    files.forEach(file =>
      ingestDoc(file.name, () => uploadFile(file, collection), collection)
    )
  }

  // Ingest all bundled samples in parallel, each into its canonical collection.
  async function loadSamples() {
    await Promise.all(
      SAMPLE_FILES.map(name =>
        ingestDoc(
          name,
          () => ingestSample(`samples/${name}`, SAMPLE_FILE_COLLECTIONS[name]),
          SAMPLE_FILE_COLLECTIONS[name],
        )
      )
    )
  }

  function onFileInput(e: React.ChangeEvent<HTMLInputElement>) {
    handleFiles(Array.from(e.target.files ?? []))
  }

  const statusIcon = (s: CatalogDoc['status']) => {
    if (s === 'ready') return <CheckCircle size={14} className="text-green-600" />
    if (s === 'error') return <AlertCircle  size={14} className="text-red-600"   />
    return <Loader size={14} className="text-green-700 animate-spin" />
  }

  return (
    <div className="max-w-4xl">
      <div className="flex items-center justify-between mb-6">
        <div>
          <h1 className="text-2xl font-semibold text-stone-900">Catalog</h1>
          <p className="text-stone-500 text-sm">Upload product specs, manuals, and policies.</p>
        </div>
        <div className="flex items-center gap-3">
          <button
            onClick={loadSamples}
            className="flex items-center gap-2 text-sm text-green-700 hover:text-green-800 transition-colors"
          >
            <FolderDown size={15} /> Load bundled samples
          </button>
          <label className="cursor-pointer inline-flex items-center gap-2 px-4 py-2 bg-green-700 hover:bg-green-800 text-white rounded-lg text-sm font-medium transition-colors">
            <Upload size={15} /> Upload
            <input type="file" multiple accept=".md,.txt,.csv,.pdf" className="hidden" onChange={onFileInput} />
          </label>
        </div>
      </div>

      {/* Finding #4: collection selector wired to actual Arcanum collection_id */}
      <div className="flex items-center gap-3 mb-6">
        <span className="text-sm text-stone-500">Collection:</span>
        <div className="flex gap-2">
          {COLLECTIONS.map(c => (
            <button
              key={c.id}
              onClick={() => setCollection(c.id)}
              className={`px-3 py-1.5 text-sm rounded-lg border transition-colors ${
                collection === c.id
                  ? 'bg-green-700 text-white border-green-700'
                  : 'border-stone-300 text-stone-600 hover:border-stone-400'
              }`}
            >
              {c.label}
            </button>
          ))}
        </div>
      </div>

      {docs.length === 0 ? (
        <div className="text-stone-400 text-sm p-8 text-center border border-dashed border-stone-300 rounded-xl">
          No catalog items yet. Upload files or click <strong>Load bundled samples</strong>.
        </div>
      ) : (
        <div className="space-y-2">
          {docs.map(d => (
            <div key={d.id} className="flex items-center gap-3 bg-white border border-stone-200 rounded-lg px-4 py-3 shadow-sm">
              <Package size={16} className="text-stone-400 flex-shrink-0" />
              <span className="flex-1 text-sm text-stone-700 truncate">{d.name}</span>
              <span className="text-xs px-2 py-0.5 rounded-full bg-stone-100 text-stone-600">{d.collection}</span>
              <span className="flex items-center gap-1 text-xs text-stone-500">
                {statusIcon(d.status)} {d.status}
              </span>
            </div>
          ))}
        </div>
      )}
    </div>
  )
}
