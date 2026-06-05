import { useState, useCallback, useRef, useEffect } from 'react'
import { useSearchParams } from 'react-router-dom'
import { uploadFile, ingestSample, listCollections } from '../api/ingest'
import { apiKey } from '../api/auth'

import { createVectorCollection } from '../api/ingest'
import { Upload, FileText, CheckCircle, AlertCircle, Loader, FolderDown, RefreshCw } from 'lucide-react'

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
  sourcePath?: string    // server-side path for sample files — enables re-ingest
  error?: string
  chunkCount?: number
  ingestedAt?: string    // ISO timestamp set when ingest is submitted
}

export default function DocsPage() {
  const [searchParams] = useSearchParams()
  const [collection, setCollection] = useState('devforge')
  const [files, setFiles] = useState<IngestedFile[]>([])
  const [dragging, setDragging] = useState(false)
  const [serverCollections, setServerCollections] = useState<string[]>([])
  const [showNewCollection, setShowNewCollection] = useState(false)
  const [newCollectionName, setNewCollectionName] = useState('')

  const wsRef = useRef<WebSocket | null>(null)
  const pendingOps = useRef<Map<string, string>>(new Map())

  useEffect(() => {
    const col = searchParams.get('collection')
    if (col) setCollection(col)
  }, []) // intentional: read once on mount

  useEffect(() => {
    listCollections()
      .then(remote => {
        const names = Array.from(new Set(['devforge', ...remote.map(r => r.id)]))
        setServerCollections(names)
      })
      .catch(() => setServerCollections(['devforge']))
  }, [])

  function connectWs(collectionId: string) {
    if (wsRef.current?.readyState === WebSocket.OPEN || wsRef.current?.readyState === WebSocket.CONNECTING) return
    const ws = new WebSocket(`ws://${location.host}/ws/events`, ['arcanum-v1', apiKey])
    ws.onopen = () => {
      ws.send(JSON.stringify({ subscribe: [`ingestion:${collectionId}`] }))
    }
    ws.onmessage = evt => {
      try {
        const msg = JSON.parse(evt.data as string)
        if (msg.type !== 'event') return
        const { operation_id, status, report } = msg.payload as {
          operation_id: string
          status: string
          report?: { total_chunks?: number }
        }
        const fileName = pendingOps.current.get(operation_id)
        if (!fileName) return

        if (status === 'completed') {
          const chunks = report?.total_chunks ?? 0
          setFiles(prev => prev.map(f =>
            f.name === fileName
              ? { ...f, status: 'ready', operationId: operation_id, chunkCount: chunks }
              : f
          ))
          pendingOps.current.delete(operation_id)
        } else if (status === 'skipped') {
          setFiles(prev => prev.map(f =>
            f.name === fileName ? { ...f, status: 'ready', operationId: operation_id } : f
          ))
          pendingOps.current.delete(operation_id)
        }
      } catch {}
    }
    ws.onerror = () => { wsRef.current = null }
    ws.onclose = () => { wsRef.current = null }
    wsRef.current = ws
  }

  useEffect(() => () => { wsRef.current?.close() }, [])

  function markError(name: string, error: string) {
    setFiles(prev => prev.map(f => f.name === name ? { ...f, status: 'error', error } : f))
  }

  async function processFile(file: File) {
    const entry: IngestedFile = { name: file.name, status: 'indexing', ingestedAt: new Date().toISOString() }
    setFiles(prev => [...prev, entry])
    connectWs(collection)
    try {
      const res = await uploadFile(file, collection)
      pendingOps.current.set(res.operation_id, file.name)
    } catch (err) {
      markError(file.name, String(err))
    }
  }

  async function loadSamples() {
    connectWs(collection)
    await Promise.all(SAMPLE_FILES.map(async name => {
      const path = `samples/${name}`
      const entry: IngestedFile = { name, status: 'indexing', sourcePath: path, ingestedAt: new Date().toISOString() }
      setFiles(prev => [...prev, entry])
      try {
        const res = await ingestSample(path, collection)
        pendingOps.current.set(res.operation_id, name)
      } catch (err) {
        markError(name, String(err))
      }
    }))
  }

  async function reIngestFile(f: IngestedFile) {
    if (!f.sourcePath) return
    setFiles(prev => prev.map(x => x.name === f.name
      ? { ...x, status: 'indexing', chunkCount: undefined, ingestedAt: new Date().toISOString() }
      : x
    ))
    connectWs(collection)
    try {
      const res = await ingestSample(f.sourcePath, collection, undefined, true)
      pendingOps.current.set(res.operation_id, f.name)
    } catch (err) {
      markError(f.name, String(err))
    }
  }

  const onDrop = useCallback((e: React.DragEvent) => {
    e.preventDefault()
    setDragging(false)
    Array.from(e.dataTransfer.files).forEach(processFile)
  }, [collection]) // eslint-disable-line react-hooks/exhaustive-deps

  function onFileInput(e: React.ChangeEvent<HTMLInputElement>) {
    Array.from(e.target.files ?? []).forEach(processFile)
  }

  function statusIcon(s: IngestedFile['status']) {
    if (s === 'ready')    return <CheckCircle size={14} className="text-green-400" />
    if (s === 'error')    return <AlertCircle size={14} className="text-red-400" />
    if (s === 'indexing') return <Loader size={14} className="text-blue-400 animate-spin" />
    return <FileText size={14} className="text-slate-500" />
  }

  function handleCollectionChange(value: string) {
    if (value === '__new__') {
      setShowNewCollection(true)
    } else {
      setCollection(value)
      setShowNewCollection(false)
    }
  }

  async function confirmNewCollection() {
    const name = newCollectionName.trim()
    if (!name) return
    const result = await createVectorCollection(name)
    if (result.conflict) return
    if (!result.ok) return
    setServerCollections(prev => Array.from(new Set([...prev, name])))
    setCollection(name)
    setShowNewCollection(false)
    setNewCollectionName('')
  }

  return (
    <div className="max-w-2xl">
      <h1 className="text-2xl font-mono text-slate-100 mb-6">Ingest Docs</h1>

      <div className="mb-4 space-y-2">
        <div className="flex items-center gap-3">
          <label className="text-sm text-slate-400 shrink-0">Collection:</label>
          <select
            value={showNewCollection ? '__new__' : collection}
            onChange={e => handleCollectionChange(e.target.value)}
            className="bg-[#13131f] border border-slate-700 rounded px-3 py-1.5 text-sm text-slate-200 font-mono focus:border-blue-500 outline-none"
          >
            {serverCollections.map(name => (
              <option key={name} value={name}>{name}</option>
            ))}
            <option value="__new__">+ New collection…</option>
          </select>
          <button
            onClick={loadSamples}
            className="ml-auto flex items-center gap-2 text-sm text-blue-400 hover:text-blue-300 transition-colors"
          >
            <FolderDown size={15} /> Load bundled samples
          </button>
        </div>

        {showNewCollection && (
          <div className="flex gap-2 pl-20">
            <input
              value={newCollectionName}
              onChange={e => setNewCollectionName(e.target.value)}
              onKeyDown={e => e.key === 'Enter' && confirmNewCollection()}
              placeholder="Collection name"
              className="flex-1 bg-[#13131f] border border-slate-700 rounded px-3 py-1.5 text-sm text-slate-200 font-mono focus:border-blue-500 outline-none"
              autoFocus
            />
            <button onClick={confirmNewCollection} className="px-3 py-1.5 bg-blue-600 hover:bg-blue-500 text-white rounded text-sm">Create</button>
            <button onClick={() => { setShowNewCollection(false); setNewCollectionName('') }} className="px-3 py-1.5 bg-slate-700 hover:bg-slate-600 text-slate-300 rounded text-sm">Cancel</button>
          </div>
        )}
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
              <div className="flex-1 min-w-0">
                <div className="text-sm text-slate-300 font-mono truncate">{f.name}</div>
                <div className="flex gap-3 text-xs text-slate-600 font-mono mt-0.5">
                  {f.chunkCount != null && <span>{f.chunkCount} chunks</span>}
                  {f.ingestedAt && (
                    <span title={f.ingestedAt}>{new Date(f.ingestedAt).toLocaleTimeString()}</span>
                  )}
                  {f.error && <span className="text-red-400 truncate">{f.error}</span>}
                </div>
              </div>
              <span className={`text-xs font-mono shrink-0 ${
                f.status === 'ready'     ? 'text-green-400' :
                f.status === 'error'    ? 'text-red-400'   :
                f.status === 'indexing' ? 'text-blue-400'  : 'text-slate-500'
              }`}>{f.status}</span>
              {f.sourcePath && (
                <button onClick={() => reIngestFile(f)} title="Re-ingest (force)" className="p-1 text-slate-600 hover:text-slate-300 transition-colors">
                  <RefreshCw size={13} />
                </button>
              )}
            </div>
          ))}
        </div>
      )}
    </div>
  )
}
