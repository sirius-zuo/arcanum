import { useState, useCallback } from 'react'
import { uploadFile, ingestSample } from '../api/ingest'
import { Upload, FileText, CheckCircle, AlertCircle, Loader, FolderDown } from 'lucide-react'

const SAMPLE_FILES = [
  'api-authentication.md',
  'error-reference.md',
  'rate-limiting.md',
  'sdk-quickstart.md',
]

interface IngestedFile {
  name: string
  status: 'pending' | 'indexing' | 'ready' | 'error'
  operationId?: string
  error?: string
}

export default function DocsPage() {
  const [collection, setCollection] = useState('devforge')
  const [files, setFiles] = useState<IngestedFile[]>([])
  const [dragging, setDragging] = useState(false)

  function markReady(name: string, operationId?: string) {
    setFiles(prev => prev.map(f => f.name === name ? { ...f, status: 'ready', operationId } : f))
  }
  function markError(name: string, error: string) {
    setFiles(prev => prev.map(f => f.name === name ? { ...f, status: 'error', error } : f))
  }

  // Browser upload → POST /api/v1/upload (raw bytes).
  async function processFile(file: File) {
    setFiles(prev => [...prev, { name: file.name, status: 'indexing' }])
    try {
      const res = await uploadFile(file, collection)
      markReady(file.name, res.operation_id)
    } catch (err) {
      markError(file.name, String(err))
    }
  }

  // Bundled samples → POST /api/v1/ingest by server path (FileLoader reads them).
  async function loadSamples() {
    for (const name of SAMPLE_FILES) {
      setFiles(prev => [...prev, { name, status: 'indexing' }])
      try {
        const res = await ingestSample(`samples/${name}`, collection)
        markReady(name, res.operation_id)
      } catch (err) {
        markError(name, String(err))
      }
    }
  }

  const onDrop = useCallback((e: React.DragEvent) => {
    e.preventDefault()
    setDragging(false)
    Array.from(e.dataTransfer.files).forEach(processFile)
  }, [collection])

  function onFileInput(e: React.ChangeEvent<HTMLInputElement>) {
    Array.from(e.target.files ?? []).forEach(processFile)
  }

  const statusIcon = (s: IngestedFile['status']) => {
    if (s === 'ready') return <CheckCircle size={14} className="text-green-400" />
    if (s === 'error') return <AlertCircle size={14} className="text-red-400" />
    if (s === 'indexing') return <Loader size={14} className="text-blue-400 animate-spin" />
    return <FileText size={14} className="text-slate-500" />
  }

  return (
    <div className="max-w-2xl">
      <h1 className="text-2xl font-mono text-slate-100 mb-6">Ingest Docs</h1>

      <div className="mb-4 flex items-center gap-3">
        <label className="text-sm text-slate-400">Collection:</label>
        <input
          value={collection}
          onChange={e => setCollection(e.target.value)}
          className="bg-[#13131f] border border-slate-700 rounded px-3 py-1.5 text-sm text-slate-200 font-mono focus:border-blue-500 outline-none"
        />
        <button
          onClick={loadSamples}
          className="ml-auto flex items-center gap-2 text-sm text-blue-400 hover:text-blue-300 transition-colors"
        >
          <FolderDown size={15} /> Load bundled samples
        </button>
      </div>

      <div
        onDragOver={e => { e.preventDefault(); setDragging(true) }}
        onDragLeave={() => setDragging(false)}
        onDrop={onDrop}
        className={`border-2 border-dashed rounded-xl p-12 text-center transition-colors cursor-pointer ${
          dragging ? 'border-blue-500 bg-blue-500/10' : 'border-slate-700 hover:border-slate-500'
        }`}
        onClick={() => document.getElementById('file-input')?.click()}
      >
        <Upload size={32} className="mx-auto mb-3 text-slate-500" />
        <p className="text-slate-400 text-sm">Drop Markdown, text, or HTML files here</p>
        <p className="text-slate-600 text-xs mt-1">or click to browse</p>
        <input id="file-input" type="file" multiple accept=".md,.txt,.html" className="hidden" onChange={onFileInput} />
      </div>

      {files.length > 0 && (
        <div className="mt-6 space-y-2">
          <h2 className="text-sm font-medium text-slate-400 mb-3">Ingested files</h2>
          {files.map((f, i) => (
            <div key={i} className="flex items-center gap-3 bg-[#13131f] border border-slate-800 rounded-lg px-4 py-3">
              {statusIcon(f.status)}
              <span className="flex-1 text-sm text-slate-300 font-mono truncate">{f.name}</span>
              <span className={`text-xs font-mono ${
                f.status === 'ready' ? 'text-green-400' :
                f.status === 'error' ? 'text-red-400' :
                f.status === 'indexing' ? 'text-blue-400' : 'text-slate-500'
              }`}>{f.status}</span>
            </div>
          ))}
        </div>
      )}
    </div>
  )
}
