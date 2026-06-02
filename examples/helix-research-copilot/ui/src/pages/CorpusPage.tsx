import { useState } from 'react'
import { Link } from 'react-router-dom'
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

// Maps entity type strings to the same colour palette used by KnowledgeGraphPage.
function entityTypeColor(type: string): string {
  const t = type.toLowerCase()
  if (t.includes('compound')) return '#60a5fa'
  if (t.includes('protein'))  return '#34d399'
  if (t.includes('gene'))     return '#a78bfa'
  if (t.includes('pathway'))  return '#fbbf24'
  return '#94a3b8'
}

interface CorpusDoc {
  name: string
  status: 'indexing' | 'ready' | 'error'
  entityTypes?: string[]  // entity types in the graph after this doc was ingested
}

export default function CorpusPage() {
  const [docs, setDocs]               = useState<CorpusDoc[]>([])
  const [entityCount, setEntityCount] = useState<number | null>(null)
  const [dragging, setDragging]       = useState(false)
  // Finding #5: prevents double-click re-triggering loadSamples
  const [samplesLoading, setSamplesLoading] = useState(false)

  // Best-effort graph poll after a doc ingests.
  // Failure doesn't affect the doc's ingestion status — graph data is supplemental.
  async function refreshGraph(docName: string) {
    try {
      const g = await fetchGraph()
      const entityTypes = [...new Set(g.nodes.map(n => n.entity_type))]
      setEntityCount(g.nodes.length)
      setDocs(prev => prev.map(d => d.name === docName ? { ...d, entityTypes } : d))
    } catch {
      // Graph API unreachable — entity preview stays empty, corpus upload still succeeded.
    }
  }

  async function processFile(file: File) {
    setDocs(prev => [...prev, { name: file.name, status: 'indexing' }])
    try {
      await uploadFile(file, COLLECTION, 'full')
      setDocs(prev => prev.map(d => d.name === file.name ? { ...d, status: 'ready' } : d))
      await refreshGraph(file.name)
    } catch {
      setDocs(prev => prev.map(d => d.name === file.name ? { ...d, status: 'error' } : d))
    }
  }

  async function loadSamples() {
    // Finding #5: guard prevents re-entry while in progress
    if (samplesLoading) return
    setSamplesLoading(true)
    try {
      for (const name of SAMPLE_FILES) {
        setDocs(prev => [...prev, { name, status: 'indexing' }])
        try {
          await ingestSample(`samples/${name}`, COLLECTION, 'full')
          setDocs(prev => prev.map(d => d.name === name ? { ...d, status: 'ready' } : d))
          await refreshGraph(name)
        } catch {
          setDocs(prev => prev.map(d => d.name === name ? { ...d, status: 'error' } : d))
        }
      }
    } finally {
      setSamplesLoading(false)
    }
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
    if (s === 'error') return <AlertCircle  size={14} className="text-red-400"     />
    return <Loader size={14} className="text-teal-400 animate-spin" />
  }

  return (
    <div className="max-w-2xl">
      <h1 className="text-2xl font-semibold text-slate-100 mb-1">Research Corpus</h1>
      <p className="text-slate-500 text-sm mb-4">
        Upload papers and protocols. The Full pipeline extracts entities, builds a RAPTOR tree, and adds contextual prefixes.
      </p>

      {/* Finding #5: disabled while loading to prevent duplicate rows */}
      <button
        onClick={loadSamples}
        disabled={samplesLoading}
        className="mb-4 flex items-center gap-2 text-sm text-teal-400 hover:text-teal-300 transition-colors disabled:opacity-50 disabled:cursor-not-allowed"
      >
        <FolderDown size={15} /> {samplesLoading ? 'Loading samples…' : 'Load bundled samples'}
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
        {/* Finding #3: note updated to mention PDF; accept attribute now includes .pdf */}
        <p className="text-slate-600 text-xs mt-1">PDF, Markdown, text — or click to browse</p>
        <input
          id="file-input"
          type="file"
          multiple
          accept=".md,.txt,.pdf"
          className="hidden"
          onChange={onFileInput}
        />
      </div>

      {entityCount !== null && (
        <div className="mt-4 flex items-center gap-2 text-sm text-purple-300">
          <Network size={15} /> {entityCount} entities extracted ·{' '}
          {/* Finding #2: Link instead of <a> — no full page reload */}
          <Link to="/graph" className="underline hover:text-purple-200">
            view knowledge graph
          </Link>
        </div>
      )}

      {docs.length > 0 && (
        <div className="mt-6 space-y-2">
          {docs.map((d, i) => (
            <div key={i} className="bg-[#0f0f16] border border-slate-800 rounded-lg px-4 py-3">
              <div className="flex items-center gap-3">
                <FileText size={15} className="text-slate-500 flex-shrink-0" />
                <span className="flex-1 text-sm text-slate-300 font-mono truncate">{d.name}</span>
                {statusIcon(d.status)}
                <span className="text-xs text-slate-500 font-mono">{d.status}</span>
              </div>
              {/* Finding #9: entity type preview — coloured dots per entity type */}
              {d.entityTypes && d.entityTypes.length > 0 && (
                <div className="mt-2 flex items-center gap-1.5 pl-6 flex-wrap">
                  {d.entityTypes.map(type => (
                    <span
                      key={type}
                      title={type}
                      style={{ backgroundColor: entityTypeColor(type) }}
                      className="w-2 h-2 rounded-full flex-shrink-0"
                    />
                  ))}
                  <span className="text-xs text-slate-600 font-mono ml-0.5">
                    {d.entityTypes.join(', ')}
                  </span>
                </div>
              )}
            </div>
          ))}
        </div>
      )}
    </div>
  )
}
