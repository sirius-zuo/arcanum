import { useEffect, useState } from 'react'
import { getCollections, addCollection, deleteCollection, CollectionInfo } from '../store/collections'
import { apiKey } from '../api/auth'
import { Database, RefreshCw, Plus, Trash2, Activity } from 'lucide-react'
import { useNavigate } from 'react-router-dom'

export default function CollectionsPage() {
  const [collections, setCollections] = useState<CollectionInfo[]>([])
  const [health, setHealth] = useState<'ok' | 'error' | 'unknown'>('unknown')
  const [showCreateModal, setShowCreateModal] = useState(false)
  const [newName, setNewName] = useState('')
  const [deleteTarget, setDeleteTarget] = useState<string | null>(null)
  const navigate = useNavigate()

  function load() {
    setCollections(getCollections())
  }

  async function loadHealth() {
    try {
      const res = await fetch('/admin/health', {
        headers: { Authorization: `Bearer ${apiKey}` },
      })
      const data = await res.json()
      setHealth(data?.vector_store === 'ok' ? 'ok' : 'error')
    } catch {
      setHealth('error')
    }
  }

  useEffect(() => {
    load()
    loadHealth()
  }, [])

  function handleCreate() {
    const name = newName.trim()
    if (!name) return
    addCollection(name)
    load()
    setNewName('')
    setShowCreateModal(false)
    navigate(`/docs?collection=${encodeURIComponent(name)}`)
  }

  function handleDelete(id: string) {
    deleteCollection(id)
    load()
    setDeleteTarget(null)
  }

  const healthColor = health === 'ok' ? 'text-green-400' : health === 'error' ? 'text-red-400' : 'text-slate-500'

  return (
    <div className="max-w-2xl">
      <div className="flex items-center justify-between mb-6">
        <h1 className="text-2xl font-mono text-slate-100">Collections</h1>
        <div className="flex items-center gap-3">
          <span className={`flex items-center gap-1.5 text-xs font-mono ${healthColor}`}>
            <Activity size={12} />
            {health === 'ok' ? 'vector store ok' : health === 'error' ? 'store error' : 'checking…'}
          </span>
          <button
            onClick={() => { load(); loadHealth() }}
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
          <span>No collections yet. Ingest some docs to create one.</span>
        </div>
      )}

      <div className="space-y-3">
        {collections.map(c => (
          <div key={c.id} className="bg-[#13131f] border border-slate-800 rounded-lg px-4 py-3">
            <div className="flex items-start justify-between gap-3">
              <div className="flex items-center gap-3">
                <Database size={16} className="text-blue-400 flex-shrink-0 mt-0.5" />
                <div>
                  <div className="text-sm text-slate-200 font-mono">{c.name}</div>
                  <div className="flex gap-3 text-xs text-slate-500 font-mono mt-0.5">
                    <span>{c.docCount} docs</span>
                    <span>{c.chunkCount} chunks</span>
                    <span title={c.lastIngested}>last: {new Date(c.lastIngested).toLocaleString()}</span>
                  </div>
                </div>
              </div>
              <button
                onClick={() => setDeleteTarget(c.id)}
                className="p-1.5 text-slate-600 hover:text-red-400 transition-colors rounded"
                title="Delete collection"
              >
                <Trash2 size={14} />
              </button>
            </div>
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
            <p className="text-xs text-slate-500">Creates a named slot and navigates to Docs to start ingesting.</p>
            <div className="flex gap-2 justify-end">
              <button onClick={() => { setShowCreateModal(false); setNewName('') }} className="px-4 py-2 text-sm text-slate-400 hover:text-slate-200 transition-colors">Cancel</button>
              <button onClick={handleCreate} disabled={!newName.trim()} className="px-4 py-2 text-sm bg-blue-600 hover:bg-blue-500 disabled:opacity-40 text-white rounded-md transition-colors">Create</button>
            </div>
          </div>
        </div>
      )}

      {deleteTarget && (
        <div className="fixed inset-0 bg-black/60 flex items-center justify-center z-50">
          <div className="bg-[#1e1e2e] border border-slate-700 rounded-xl p-6 w-80 space-y-4">
            <h2 className="text-sm font-mono text-slate-100">Delete "{deleteTarget}"?</h2>
            <p className="text-xs text-slate-500">Removes this collection from the list. Vector data on the server is not deleted.</p>
            <div className="flex gap-2 justify-end">
              <button onClick={() => setDeleteTarget(null)} className="px-4 py-2 text-sm text-slate-400 hover:text-slate-200 transition-colors">Cancel</button>
              <button onClick={() => handleDelete(deleteTarget)} className="px-4 py-2 text-sm bg-red-700 hover:bg-red-600 text-white rounded-md transition-colors">Delete</button>
            </div>
          </div>
        </div>
      )}
    </div>
  )
}
