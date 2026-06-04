import { useEffect, useState } from 'react'
import { getKnownCollections, rememberCollection, forgetCollection } from '../store/collections'
import { listVectorCollections, getVectorCollectionStats, createVectorCollection, deleteVectorCollection } from '../api/ingest'
import { Database, RefreshCw, Plus, Trash2 } from 'lucide-react'
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
  const [deleteTarget, setDeleteTarget] = useState<string | null>(null)
  const [loading, setLoading] = useState(true)
  const navigate = useNavigate()

  async function load() {
    try {
      const remote = await listVectorCollections()
      const localNames = getKnownCollections()
      const allNames = Array.from(new Set([...remote.map(r => r.id), ...localNames]))
      const remoteSet = new Set(remote.map(r => r.id))
      setCollections(allNames.map(name => ({
        name,
        docCount: remoteSet.has(name) ? null : 0,
      })))
      // Fetch counts for collections that exist on server
      for (const name of allNames.filter(n => remoteSet.has(n))) {
        const count = await getVectorCollectionStats(name)
        setCollections(prev => prev.map(c => c.name === name ? { ...c, docCount: count } : c))
      }
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
      rememberCollection(name)
      load()
      setShowCreateModal(false)
      setNewName('')
      navigate(`/docs?collection=${encodeURIComponent(name)}`)
    }
  }

  async function handleDelete(name: string) {
    await deleteVectorCollection(name)
    forgetCollection(name)
    load()
    setDeleteTarget(null)
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
          <div key={c.name} className="bg-[#13131f] border border-slate-800 rounded-lg px-4 py-3">
            <div className="flex items-start justify-between gap-3">
              <div className="flex items-center gap-3">
                <Database size={16} className="text-blue-400 flex-shrink-0 mt-0.5" />
                <div>
                  <div className="text-sm text-slate-200 font-mono">{c.name}</div>
                  <div className="flex gap-3 text-xs text-slate-500 font-mono mt-0.5">
                    <span>{c.docCount !== null ? `${c.docCount} docs` : '…'}</span>
                  </div>
                </div>
              </div>
              <button
                onClick={() => setDeleteTarget(c.name)}
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
            {createError && <p className="text-xs text-red-400">{createError}</p>}
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
            <p className="text-xs text-slate-500">Permanently deletes this collection and all its vector data from the server.</p>
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
