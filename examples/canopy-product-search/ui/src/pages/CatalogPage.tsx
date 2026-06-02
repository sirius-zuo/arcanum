import { useState } from 'react'
import { uploadFile, ingestSample } from '../api/ingest'
import { Upload, Package, CheckCircle, AlertCircle, Loader, FolderDown } from 'lucide-react'

const COLLECTION = 'canopy'
const CATEGORIES = ['Jackets', 'Tents', 'Sleeping Bags', 'Accessories', 'Policies'] as const
const SAMPLE_FILES = ['jackets-catalog.md', 'tents-catalog.md', 'sleeping-bags-catalog.md', 'return-policy.md']

interface CatalogDoc {
  name: string
  category: string
  status: 'indexing' | 'ready' | 'error'
}

export default function CatalogPage() {
  const [category, setCategory] = useState<string>('Jackets')
  const [docs, setDocs] = useState<CatalogDoc[]>([])

  async function processFile(file: File) {
    setDocs(prev => [...prev, { name: file.name, category, status: 'indexing' }])
    try {
      await uploadFile(file, COLLECTION)
      setDocs(prev => prev.map(d => d.name === file.name ? { ...d, status: 'ready' } : d))
    } catch {
      setDocs(prev => prev.map(d => d.name === file.name ? { ...d, status: 'error' } : d))
    }
  }

  // Bundled samples → POST /api/v1/ingest by server path (FileLoader reads them).
  async function loadSamples() {
    for (const name of SAMPLE_FILES) {
      setDocs(prev => [...prev, { name, category, status: 'indexing' }])
      try {
        await ingestSample(`samples/${name}`, COLLECTION)
        setDocs(prev => prev.map(d => d.name === name ? { ...d, status: 'ready' } : d))
      } catch {
        setDocs(prev => prev.map(d => d.name === name ? { ...d, status: 'error' } : d))
      }
    }
  }

  function onFileInput(e: React.ChangeEvent<HTMLInputElement>) {
    Array.from(e.target.files ?? []).forEach(processFile)
  }

  const statusIcon = (s: CatalogDoc['status']) => {
    if (s === 'ready') return <CheckCircle size={14} className="text-green-600" />
    if (s === 'error') return <AlertCircle size={14} className="text-red-600" />
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
          <button onClick={loadSamples} className="flex items-center gap-2 text-sm text-green-700 hover:text-green-800 transition-colors">
            <FolderDown size={15} /> Load bundled samples
          </button>
          <label className="cursor-pointer inline-flex items-center gap-2 px-4 py-2 bg-green-700 hover:bg-green-800 text-white rounded-lg text-sm font-medium transition-colors">
            <Upload size={15} /> Upload
            <input type="file" multiple accept=".md,.txt,.csv,.pdf" className="hidden" onChange={onFileInput} />
          </label>
        </div>
      </div>

      <div className="flex items-center gap-3 mb-6">
        <span className="text-sm text-stone-500">Category:</span>
        <select
          value={category}
          onChange={e => setCategory(e.target.value)}
          className="bg-white border border-stone-300 rounded-lg px-3 py-1.5 text-sm text-stone-700 outline-none focus:border-green-600"
        >
          {CATEGORIES.map(c => <option key={c}>{c}</option>)}
        </select>
      </div>

      {docs.length === 0 ? (
        <div className="text-stone-400 text-sm p-8 text-center border border-dashed border-stone-300 rounded-xl">
          No catalog items yet. Upload files from <code>samples/</code>.
        </div>
      ) : (
        <div className="space-y-2">
          {docs.map((d, i) => (
            <div key={i} className="flex items-center gap-3 bg-white border border-stone-200 rounded-lg px-4 py-3 shadow-sm">
              <Package size={16} className="text-stone-400 flex-shrink-0" />
              <span className="flex-1 text-sm text-stone-700 truncate">{d.name}</span>
              <span className="text-xs px-2 py-0.5 rounded-full bg-stone-100 text-stone-600">{d.category}</span>
              <span className="flex items-center gap-1 text-xs text-stone-500">{statusIcon(d.status)} {d.status}</span>
            </div>
          ))}
        </div>
      )}
    </div>
  )
}
