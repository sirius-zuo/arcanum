import { useState } from 'react'
import { Link } from 'react-router-dom'
import { uploadFile, ingestSample } from '../api/ingest'
import { fetchGraph } from '../api/graph'
import { Upload, FileText, CheckCircle, AlertCircle, Loader, Users, FolderDown } from 'lucide-react'

const COLLECTION = 'vantage_contracts'
const SAMPLE_FILES = [
  'standard-nda-template.md',
  'vendor-saas-agreement.md',
  'data-processing-addendum.md',
  'employment-offer-letter.md',
]

interface ContractDoc {
  name: string
  status: 'indexing' | 'ready' | 'error'
}

export default function ContractLibraryPage() {
  const [docs, setDocs] = useState<ContractDoc[]>([])
  // Finding #5: renamed from partyCount — this counts all graph entities, not just parties.
  const [entityCount, setEntityCount] = useState<number | null>(null)

  // Finding #2 & #9: has its own try/catch so it never throws to the caller.
  // A graph API failure silently skips the count update without affecting upload status.
  async function refreshEntityCount() {
    try {
      const g = await fetchGraph()
      setEntityCount(g.nodes.length)
    } catch { /* entity count is supplemental — don't surface graph errors here */ }
  }

  async function processFile(file: File) {
    setDocs(prev => [...prev, { name: file.name, status: 'indexing' }])
    try {
      await uploadFile(file, COLLECTION, 'full')
      setDocs(prev => prev.map(d => d.name === file.name ? { ...d, status: 'ready' } : d))
    } catch {
      setDocs(prev => prev.map(d => d.name === file.name ? { ...d, status: 'error' } : d))
      return  // Don't refresh entity count if upload failed.
    }
    // Finding #2: refreshEntityCount is called AFTER the try/catch — its internal
    // try/catch means a graph failure cannot flip a successful upload to 'error'.
    await refreshEntityCount()
  }

  async function loadSamples() {
    for (const name of SAMPLE_FILES) {
      setDocs(prev => [...prev, { name, status: 'indexing' }])
      try {
        await ingestSample(`samples/${name}`, COLLECTION, 'full')
        setDocs(prev => prev.map(d => d.name === name ? { ...d, status: 'ready' } : d))
      } catch {
        setDocs(prev => prev.map(d => d.name === name ? { ...d, status: 'error' } : d))
      }
    }
    // Finding #9: safe to call — refreshEntityCount handles its own errors internally.
    await refreshEntityCount()
  }

  function onFileInput(e: React.ChangeEvent<HTMLInputElement>) {
    Array.from(e.target.files ?? []).forEach(processFile)
  }

  const statusIcon = (s: ContractDoc['status']) => {
    if (s === 'ready') return <CheckCircle size={14} className="text-emerald-600" />
    if (s === 'error') return <AlertCircle size={14} className="text-red-600" />
    return <Loader size={14} className="text-slate-600 animate-spin" />
  }

  return (
    <div className="max-w-3xl">
      <div className="flex items-center justify-between mb-6">
        <div>
          <h1 className="text-2xl text-slate-900">Contract Library</h1>
          <p className="text-slate-500 text-sm">Upload contracts. The Full pipeline extracts parties and builds clause summaries.</p>
        </div>
        <div className="flex items-center gap-3">
          <button onClick={loadSamples} className="flex items-center gap-2 text-sm text-slate-700 hover:text-slate-900 transition-colors">
            <FolderDown size={15} /> Load bundled samples
          </button>
          <label className="cursor-pointer inline-flex items-center gap-2 px-4 py-2 bg-slate-800 hover:bg-slate-900 text-white rounded-lg text-sm font-medium transition-colors">
            <Upload size={15} /> Upload
            <input type="file" multiple accept=".md,.txt,.pdf" className="hidden" onChange={onFileInput} />
          </label>
        </div>
      </div>

      {entityCount !== null && (
        <div className="mb-4 flex items-center gap-2 text-sm text-slate-600">
          {/* Finding #5: label changed from "parties extracted" — the count is all entity types */}
          <Users size={15} /> {entityCount} entities extracted ·{' '}
          {/* Finding #7: Link instead of <a> — prevents full page reload */}
          <Link to="/parties" className="underline hover:text-slate-900">view registry</Link>
        </div>
      )}

      {docs.length === 0 ? (
        <div className="text-slate-400 text-sm p-8 text-center border border-dashed border-slate-300 rounded-xl">
          No contracts yet. Upload files from <code>samples/</code>.
        </div>
      ) : (
        <div className="space-y-2">
          {docs.map((d, i) => (
            <div key={i} className="flex items-center gap-3 bg-white border border-slate-200 rounded-lg px-4 py-3 shadow-sm">
              <FileText size={16} className="text-slate-400 flex-shrink-0" />
              <span className="flex-1 text-sm text-slate-700 truncate">{d.name}</span>
              {statusIcon(d.status)}
              <span className="text-xs text-slate-500">{d.status}</span>
            </div>
          ))}
        </div>
      )}
    </div>
  )
}
