import { useState } from 'react'
import { uploadFile, ingestSample } from '../api/ingest'
import { fetchGraph } from '../api/graph'
import { Upload, FileText, CheckCircle, AlertCircle, Loader, Network, FolderDown } from 'lucide-react'

const COLLECTION = 'helix_research'
const SAMPLE_FILES = [
  'egfr-inhibitors-abstract.md',
  'jak2-inhibitor-trial.md',
  'crispr-delivery-neurons.md',
  'compound-assay-results.md',
]

interface CorpusDoc {
  name: string
  status: 'indexing' | 'ready' | 'error'
}

export default function CorpusPage() {
  const [docs, setDocs] = useState<CorpusDoc[]>([])
  const [entityCount, setEntityCount] = useState<number | null>(null)
  const [dragging, setDragging] = useState(false)

  async function refreshEntityCount() {
    const g = await fetchGraph()
    setEntityCount(g.nodes.length)
  }

  async function processFile(file: File) {
    setDocs(prev => [...prev, { name: file.name, status: 'indexing' }])
    try {
      await uploadFile(file, COLLECTION, 'full')
      setDocs(prev => prev.map(d => d.name === file.name ? { ...d, status: 'ready' } : d))
      await refreshEntityCount()
    } catch {
      setDocs(prev => prev.map(d => d.name === file.name ? { ...d, status: 'error' } : d))
    }
  }

  // Bundled samples → POST /api/v1/ingest by server path (FileLoader reads them).
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
    await refreshEntityCount()
  }

  function onFileInput(e: React.ChangeEvent<HTMLInputElement>) {
    Array.from(e.target.files ?? []).forEach(processFile)
  }
  function onDrop(e: React.DragEvent) {
    e.preventDefault(); setDragging(false)
    Array.from(e.dataTransfer.files).forEach(processFile)
  }

  const statusIcon = (s: CorpusDoc['status']) => {
    if (s === 'ready') return <CheckCircle size={14} className="text-emerald-400" />
    if (s === 'error') return <AlertCircle size={14} className="text-red-400" />
    return <Loader size={14} className="text-teal-400 animate-spin" />
  }

  return (
    <div className="max-w-2xl">
      <h1 className="text-2xl font-semibold text-slate-100 mb-1">Research Corpus</h1>
      <p className="text-slate-500 text-sm mb-4">
        Upload papers and protocols. The Full pipeline extracts entities, builds a RAPTOR tree, and adds contextual prefixes.
      </p>

      <button onClick={loadSamples} className="mb-4 flex items-center gap-2 text-sm text-teal-400 hover:text-teal-300 transition-colors">
        <FolderDown size={15} /> Load bundled samples
      </button>

      <div
        onDragOver={e => { e.preventDefault(); setDragging(true) }}
        onDragLeave={() => setDragging(false)}
        onDrop={onDrop}
        onClick={() => document.getElementById('file-input')?.click()}
        className={`border-2 border-dashed rounded-xl p-12 text-center transition-colors cursor-pointer ${
          dragging ? 'border-teal-500 bg-teal-500/10' : 'border-slate-700 hover:border-slate-500'
        }`}
      >
        <Upload size={32} className="mx-auto mb-3 text-slate-500" />
        <p className="text-slate-400 text-sm">Drop research documents here</p>
        <p className="text-slate-600 text-xs mt-1">Markdown, text — or click to browse</p>
        <input id="file-input" type="file" multiple accept=".md,.txt" className="hidden" onChange={onFileInput} />
      </div>

      {entityCount !== null && (
        <div className="mt-4 flex items-center gap-2 text-sm text-purple-300">
          <Network size={15} /> {entityCount} entities extracted so far ·{' '}
          <a href="/graph" className="underline hover:text-purple-200">view knowledge graph</a>
        </div>
      )}

      {docs.length > 0 && (
        <div className="mt-6 space-y-2">
          {docs.map((d, i) => (
            <div key={i} className="flex items-center gap-3 bg-[#0f0f16] border border-slate-800 rounded-lg px-4 py-3">
              <FileText size={15} className="text-slate-500" />
              <span className="flex-1 text-sm text-slate-300 font-mono truncate">{d.name}</span>
              {statusIcon(d.status)}
              <span className="text-xs text-slate-500 font-mono">{d.status}</span>
            </div>
          ))}
        </div>
      )}
    </div>
  )
}
