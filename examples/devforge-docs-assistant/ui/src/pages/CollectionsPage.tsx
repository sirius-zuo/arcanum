import { useEffect, useState } from 'react'

import { listVectorCollections, getVectorCollectionStats, createVectorCollection, deleteVectorCollection, listCollectionDocuments, deleteCollectionDocument } from '../api/ingest'
import type { DocumentEntry } from '../api/ingest'
import { Database, RefreshCw, Plus, Trash2, ChevronRight, ChevronDown, FileText } from 'lucide-react'
import { useNavigate } from 'react-router-dom'

interface DisplayCollection {
  name: string
  docCount: number | null
}

export default function CollectionsPage() {
  const [collections, setCollections] = useState<DisplayCollection[]>([])
  const [showCreateModal, setShowCreateModal] = useState(false)
  const [newName, setNewName] = useState('')
  const [createError, setCreateError] = useState('')
  const [collectionDeleteTarget, setCollectionDeleteTarget] = useState<string | null>(null)
  const [loading, setLoading] = useState(true)
  const navigate = useNavigate()

  const [expanded, setExpanded] = useState<Set<string>>(new Set())
  const [docsByCollection, setDocsByCollection] = useState<Record<string, DocumentEntry[]>>({})
  const [loadingDocs, setLoadingDocs] = useState<Set<string>>(new Set())
  const [fileDeleteTarget, setFileDeleteTarget] = useState<{ col: string; uri: string } | null>(null)
  const [docErrors, setDocErrors] = useState<Set<string>>(new Set())

  async function load() {
    try {
      const remote = await listVectorCollections()
      setCollections(remote.map(r => ({ name: r.id, docCount: null })))
      await Promise.all(
        remote.map(async r => {
          const count = await getVectorCollectionStats(r.id)
          setCollections(prev => prev.map(c => c.name === r.id ? { ...c, docCount: count } : c))
        })
      )
    } catch {
      // Silent failure — show empty state
    } finally {
      setLoading(false)
    }
  }

  useEffect(() => { load() }, [])

  async function handleCreate() {
    const name = newName.trim()
    if (!name) return
    setCreateError('')
    const result = await createVectorCollection(name)
    if (result.conflict) {
      setCreateError('Collection already exists')
      return
    }
    if (result.ok) {
      load()
      setShowCreateModal(false)
      setNewName('')
      navigate(`/docs?collection=${encodeURIComponent(name)}`)
    } else {
      setCreateError('Server error — please try again')
    }
  }

  async function handleDelete(name: string) {
    await deleteVectorCollection(name)
    load()
    setCollectionDeleteTarget(null)
  }

  async function toggleExpand(name: string) {
    setExpanded(prev => {
      const next = new Set(prev)
      if (next.has(name)) {
        next.delete(name)
      } else {
        next.add(name)
      }
      return next
    })
    // Lazy-load on first expand only (also retry after an error)
    if (!expanded.has(name) && (docsByCollection[name] === undefined || docErrors.has(name))) {
      setDocErrors(prev => { const s = new Set(prev); s.delete(name); return s })
      setLoadingDocs(prev => new Set(prev).add(name))
      try {
        const docs = await listCollectionDocuments(name)
        setDocsByCollection(prev => ({ ...prev, [name]: docs }))
      } catch {
        setDocErrors(prev => new Set(prev).add(name))
      } finally {
        setLoadingDocs(prev => { const s = new Set(prev); s.delete(name); return s })
      }
    }
  }

  async function handleFileDelete(col: string, uri: string) {
    await deleteCollectionDocument(col, uri)
    setDocsByCollection(prev => ({
      ...prev,
      [col]: (prev[col] ?? []).filter(d => d.source_uri !== uri),
    }))
    setCollections(prev => prev.map(c =>
      c.name === col && c.docCount !== null
        ? { ...c, docCount: c.docCount - 1 }
        : c
    ))
    setFileDeleteTarget(null)
  }

  function formatTimestamp(unixSecs: number): string {
    return new Date(unixSecs * 1000).toLocaleString(undefined, {
      month: 'short', day: 'numeric',
      hour: 'numeric', minute: '2-digit',
    })
  }

  function renderFileList(col: string) {
    if (loadingDocs.has(col)) {
      return (
        <div className="flex items-center gap-2 px-4 py-3 text-xs text-slate-500 font-mono border-t border-slate-800/60">
          <RefreshCw size={11} className="animate-spin" /> Loading…
        </div>
      )
    }
    if (docErrors.has(col)) {
      return (
        <div
          className="px-4 py-3 text-xs text-red-400 font-mono border-t border-slate-800/60 cursor-pointer hover:text-red-300"
          onClick={() => toggleExpand(col)}
        >
          Could not load files — click to retry.
        </div>
      )
    }
    const docs = docsByCollection[col]
    if (!docs) return null
    if (docs.length === 0) {
      return (
        <div className="px-4 py-3 text-xs text-slate-600 font-mono border-t border-slate-800/60">
          No files indexed yet.
        </div>
      )
    }
    return (
      <div className="border-t border-slate-800/60">
        {docs.map(doc => {
          const isDeleting = fileDeleteTarget?.col === col && fileDeleteTarget.uri === doc.source_uri
          return (
            <div key={doc.source_uri} className="flex items-center justify-between px-4 py-2 border-b border-slate-900 last:border-0 text-xs font-mono">
              {isDeleting ? (
                <>
                  <span className="text-slate-400 truncate mr-3">Remove <span className="text-slate-200">{doc.source_uri}</span>?</span>
                  <div className="flex items-center gap-2 shrink-0">
                    <button onClick={() => setFileDeleteTarget(null)} className="text-slate-500 hover:text-slate-300 transition-colors">Cancel</button>
                    <button onClick={() => handleFileDelete(col, doc.source_uri)} className="text-red-400 hover:text-red-300 transition-colors">Remove</button>
                  </div>
                </>
              ) : (
                <>
                  <div className="flex items-center gap-2 text-slate-400 min-w-0">
                    <FileText size={11} className="shrink-0 text-slate-600" />
                    <span className="truncate">{doc.source_uri}</span>
                  </div>
                  <div className="flex items-center gap-4 shrink-0 ml-3">
                    <span className="text-slate-600">{formatTimestamp(doc.registered_at)}</span>
                    <button
                      onClick={() => setFileDeleteTarget({ col, uri: doc.source_uri })}
                      className="text-slate-700 hover:text-red-400 transition-colors"
                      title="Remove file"
                    >
                      <Trash2 size={12} />
                    </button>
                  </div>
                </>
              )}
            </div>
          )
        })}
      </div>
    )
  }

  if (loading) {
    return (
      <div className="max-w-2xl">
        <h1 className="text-2xl font-mono text-slate-100 mb-6">Collections</h1>
        <div className="text-slate-500 text-sm flex items-center gap-2 p-6 border border-slate-800 rounded-xl">
          <RefreshCw size={14} className="animate-spin" />
          Loading…
        </div>
      </div>
    )
  }

  return (
    <div className="max-w-2xl">
      <div className="flex items-center justify-between mb-6">
        <h1 className="text-2xl font-mono text-slate-100">Collections</h1>
        <div className="flex items-center gap-3">
          <button
            onClick={() => load()}
            className="flex items-center gap-2 text-sm text-slate-400 hover:text-slate-200 transition-colors"
          >
            <RefreshCw size={14} />
            Refresh
          </button>
          <button
            onClick={() => setShowCreateModal(true)}
            className="flex items-center gap-2 text-sm px-3 py-1.5 bg-blue-600 hover:bg-blue-500 text-white rounded-md transition-colors"
          >
            <Plus size={14} />
            New
          </button>
        </div>
      </div>

      {collections.length === 0 && (
        <div className="text-slate-500 text-sm flex items-center gap-3 p-6 border border-slate-800 rounded-xl">
          <Database size={20} />
          <span>No collections yet. Create one to start ingesting.</span>
        </div>
      )}

      <div className="space-y-3">
        {collections.map(c => (
          <div key={c.name} className="bg-[#13131f] border border-slate-800 rounded-lg overflow-hidden">
            {/* Collection header — clickable to expand */}
            <div
              className="flex items-start justify-between gap-3 px-4 py-3 cursor-pointer hover:bg-[#16162a] transition-colors"
              onClick={() => toggleExpand(c.name)}
            >
              <div className="flex items-center gap-3">
                <Database size={16} className="text-blue-400 flex-shrink-0 mt-0.5" />
                <div>
                  <div className="text-sm text-slate-200 font-mono">{c.name}</div>
                  <div className="flex gap-3 text-xs text-slate-500 font-mono mt-0.5">
                    <span>{c.docCount !== null ? `${c.docCount} docs` : '…'}</span>
                  </div>
                </div>
              </div>
              <div className="flex items-center gap-2 mt-0.5">
                {expanded.has(c.name)
                  ? <ChevronDown size={14} className="text-slate-500" />
                  : <ChevronRight size={14} className="text-slate-600" />}
                <button
                  onClick={e => { e.stopPropagation(); setCollectionDeleteTarget(c.name) }}
                  className="p-1 text-slate-600 hover:text-red-400 transition-colors rounded"
                  title="Delete collection"
                >
                  <Trash2 size={14} />
                </button>
              </div>
            </div>
            {/* File list — only rendered when expanded */}
            {expanded.has(c.name) && renderFileList(c.name)}
          </div>
        ))}
      </div>

      {showCreateModal && (
        <div className="fixed inset-0 bg-black/60 flex items-center justify-center z-50">
          <div className="bg-[#1e1e2e] border border-slate-700 rounded-xl p-6 w-80 space-y-4">
            <h2 className="text-sm font-mono text-slate-100">New collection</h2>
            <input
              value={newName}
              onChange={e => setNewName(e.target.value)}
              onKeyDown={e => e.key === 'Enter' && handleCreate()}
              placeholder="collection-name"
              className="w-full bg-[#13131f] border border-slate-700 rounded px-3 py-2 text-sm text-slate-200 font-mono focus:border-blue-500 outline-none"
              autoFocus
            />
            {createError && <p className="text-xs text-red-400">{createError}</p>}
            <p className="text-xs text-slate-500">Creates a named slot and navigates to Docs to start ingesting.</p>
            <div className="flex gap-2 justify-end">
              <button onClick={() => { setShowCreateModal(false); setNewName('') }} className="px-4 py-2 text-sm text-slate-400 hover:text-slate-200 transition-colors">Cancel</button>
              <button onClick={handleCreate} disabled={!newName.trim()} className="px-4 py-2 text-sm bg-blue-600 hover:bg-blue-500 disabled:opacity-40 text-white rounded-md transition-colors">Create</button>
            </div>
          </div>
        </div>
      )}

      {collectionDeleteTarget && (
        <div className="fixed inset-0 bg-black/60 flex items-center justify-center z-50">
          <div className="bg-[#1e1e2e] border border-slate-700 rounded-xl p-6 w-80 space-y-4">
            <h2 className="text-sm font-mono text-slate-100">Delete "{collectionDeleteTarget}"?</h2>
            <p className="text-xs text-slate-500">Permanently deletes this collection and all its vector data from the server.</p>
            <div className="flex gap-2 justify-end">
              <button onClick={() => setCollectionDeleteTarget(null)} className="px-4 py-2 text-sm text-slate-400 hover:text-slate-200 transition-colors">Cancel</button>
              <button onClick={() => handleDelete(collectionDeleteTarget)} className="px-4 py-2 text-sm bg-red-700 hover:bg-red-600 text-white rounded-md transition-colors">Delete</button>
            </div>
          </div>
        </div>
      )}
    </div>
  )
}
