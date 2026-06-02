import { useEffect, useState } from 'react'
import { listCollections } from '../api/ingest'
import { Database, RefreshCw } from 'lucide-react'

interface Collection {
  id: string
  name: string
}

export default function CollectionsPage() {
  const [collections, setCollections] = useState<Collection[]>([])
  const [loading, setLoading] = useState(false)

  async function load() {
    setLoading(true)
    try {
      const data = await listCollections()
      setCollections(data)
    } finally {
      setLoading(false)
    }
  }

  useEffect(() => { load() }, [])

  return (
    <div className="max-w-2xl">
      <div className="flex items-center justify-between mb-6">
        <h1 className="text-2xl font-mono text-slate-100">Collections</h1>
        <button onClick={load} className="flex items-center gap-2 text-sm text-slate-400 hover:text-slate-200 transition-colors">
          <RefreshCw size={14} className={loading ? 'animate-spin' : ''} />
          Refresh
        </button>
      </div>

      {collections.length === 0 && !loading && (
        <div className="text-slate-500 text-sm flex items-center gap-3 p-6 border border-slate-800 rounded-xl">
          <Database size={20} />
          <span>No collections yet. Ingest some docs to create one.</span>
        </div>
      )}

      <div className="space-y-3">
        {collections.map(c => (
          <div key={c.id} className="bg-[#13131f] border border-slate-800 rounded-lg px-4 py-3 flex items-center gap-3">
            <Database size={16} className="text-blue-400 flex-shrink-0" />
            <div>
              <div className="text-sm text-slate-200 font-mono">{c.name || c.id}</div>
              <div className="text-xs text-slate-500 font-mono">{c.id}</div>
            </div>
          </div>
        ))}
      </div>
    </div>
  )
}
