import { useState } from 'react'
import { uploadFile, ingestSample } from '../api/ingest'
import { Upload, FileText, CheckCircle, AlertCircle, Loader, FolderDown } from 'lucide-react'

const COLLECTION = 'meridian'
const CATEGORIES = ['HR', 'IT', 'Benefits', 'Finance', 'Legal'] as const
const SAMPLE_FILES = ['pto-policy.md', 'parental-leave.md', '401k-benefits.md', 'performance-review.md']

interface PolicyDoc {
  name: string
  category: string
  status: 'indexing' | 'ready' | 'error'
}

export default function PolicyLibraryPage() {
  const [category, setCategory] = useState<string>('HR')
  const [docs, setDocs] = useState<PolicyDoc[]>([])
  const [activeFilter, setActiveFilter] = useState<string>('All')

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

  const visible = activeFilter === 'All' ? docs : docs.filter(d => d.category === activeFilter)

  const statusIcon = (s: PolicyDoc['status']) => {
    if (s === 'ready') return <CheckCircle size={14} className="text-green-600" />
    if (s === 'error') return <AlertCircle size={14} className="text-red-600" />
    return <Loader size={14} className="text-blue-600 animate-spin" />
  }

  return (
    <div className="max-w-4xl">
      <div className="flex items-center justify-between mb-6">
        <div>
          <h1 className="text-2xl font-semibold text-slate-900">Policy Library</h1>
          <p className="text-slate-500 text-sm">Upload and browse company policies.</p>
        </div>
        <div className="flex items-center gap-3">
          <button onClick={loadSamples} className="flex items-center gap-2 text-sm text-blue-600 hover:text-blue-700 transition-colors">
            <FolderDown size={15} /> Load bundled samples
          </button>
          <label className="cursor-pointer inline-flex items-center gap-2 px-4 py-2 bg-blue-600 hover:bg-blue-700 text-white rounded-lg text-sm font-medium transition-colors">
            <Upload size={15} /> Upload
            <input type="file" multiple accept=".md,.txt,.pdf" className="hidden" onChange={onFileInput} />
          </label>
        </div>
      </div>

      <div className="flex items-center gap-3 mb-6">
        <span className="text-sm text-slate-500">Category for upload:</span>
        <select
          value={category}
          onChange={e => setCategory(e.target.value)}
          className="bg-white border border-slate-300 rounded-lg px-3 py-1.5 text-sm text-slate-700 outline-none focus:border-blue-500"
        >
          {CATEGORIES.map(c => <option key={c}>{c}</option>)}
        </select>
      </div>

      <div className="flex gap-2 mb-5 border-b border-slate-200">
        {['All', ...CATEGORIES].map(c => (
          <button
            key={c}
            onClick={() => setActiveFilter(c)}
            className={`px-3 py-2 text-sm transition-colors border-b-2 -mb-px ${
              activeFilter === c
                ? 'border-blue-600 text-blue-700 font-medium'
                : 'border-transparent text-slate-500 hover:text-slate-800'
            }`}
          >
            {c}
          </button>
        ))}
      </div>

      {visible.length === 0 ? (
        <div className="text-slate-400 text-sm p-8 text-center border border-dashed border-slate-300 rounded-xl">
          No policies in this category yet. Upload files from the <code>samples/</code> directory.
        </div>
      ) : (
        <div className="grid grid-cols-2 gap-3">
          {visible.map((d, i) => (
            <div key={i} className="bg-white border border-slate-200 rounded-xl p-4 shadow-sm flex items-start gap-3">
              <FileText size={18} className="text-slate-400 mt-0.5 flex-shrink-0" />
              <div className="flex-1 min-w-0">
                <div className="text-sm font-medium text-slate-800 truncate">{d.name}</div>
                <div className="flex items-center gap-2 mt-1">
                  <span className="text-xs px-2 py-0.5 rounded-full bg-slate-100 text-slate-600">{d.category}</span>
                  <span className="flex items-center gap-1 text-xs text-slate-500">
                    {statusIcon(d.status)} {d.status}
                  </span>
                </div>
              </div>
            </div>
          ))}
        </div>
      )}
    </div>
  )
}
