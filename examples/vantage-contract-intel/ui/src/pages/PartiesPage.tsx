import { useEffect, useState } from 'react'
import { fetchGraph, GraphView, GraphNode } from '../api/graph'
import { Users, RefreshCw } from 'lucide-react'

export default function PartiesPage() {
  const [graph, setGraph] = useState<GraphView>({ nodes: [], edges: [] })
  const [loading, setLoading] = useState(false)
  const [selected, setSelected] = useState<GraphNode | null>(null)

  async function load() {
    setLoading(true)
    try { setGraph(await fetchGraph()) }
    finally { setLoading(false) }
  }
  useEffect(() => { load() }, [])

  // Relationships for the selected entity (edges where it is the source).
  const related = selected
    ? graph.edges.filter(e => e.source === selected.id).map(e => {
        const target = graph.nodes.find(n => n.id === e.target)
        return { label: e.label, name: target?.name ?? e.target, type: target?.entity_type ?? '' }
      })
    : []

  return (
    <div className="max-w-4xl">
      <div className="flex items-center justify-between mb-6">
        <div>
          <h1 className="text-2xl text-slate-900 flex items-center gap-2"><Users size={20} /> Parties</h1>
          <p className="text-slate-500 text-sm">Entities extracted across the contract portfolio.</p>
        </div>
        <button onClick={load} className="flex items-center gap-2 text-sm text-slate-500 hover:text-slate-800">
          <RefreshCw size={14} className={loading ? 'animate-spin' : ''} /> Refresh
        </button>
      </div>

      {graph.nodes.length === 0 ? (
        <div className="text-slate-400 text-sm p-8 text-center border border-dashed border-slate-300 rounded-xl">
          No parties yet. Upload contracts on the Library page.
        </div>
      ) : (
        <div className="flex gap-6">
          <div className="flex-1 bg-white border border-slate-200 rounded-xl overflow-hidden shadow-sm">
            <table className="w-full text-sm">
              <thead>
                <tr className="border-b border-slate-200 text-left text-slate-500 text-xs uppercase tracking-wide">
                  <th className="px-4 py-2.5 font-medium">Entity</th>
                  <th className="px-4 py-2.5 font-medium">Type</th>
                </tr>
              </thead>
              <tbody>
                {graph.nodes.map(n => (
                  <tr key={n.id} onClick={() => setSelected(n)} className={`border-b border-slate-100 last:border-0 cursor-pointer hover:bg-slate-50 ${selected?.id === n.id ? 'bg-slate-50' : ''}`}>
                    <td className="px-4 py-3 text-slate-800">{n.name}</td>
                    <td className="px-4 py-3"><span className="text-xs px-2 py-0.5 rounded-full bg-slate-100 text-slate-600">{n.entity_type}</span></td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>

          <div className="w-72 flex-shrink-0">
            {selected ? (
              <div className="bg-white border border-slate-200 rounded-xl p-5 shadow-sm">
                <div className="text-lg text-slate-900">{selected.name}</div>
                <div className="text-xs text-slate-500 mb-4">{selected.entity_type}</div>
                <div className="text-xs font-medium text-slate-500 uppercase tracking-wide mb-2">Relationships</div>
                {related.length === 0 ? (
                  <p className="text-sm text-slate-400">No outgoing relationships.</p>
                ) : (
                  <ul className="space-y-2">
                    {related.map((r, i) => (
                      <li key={i} className="text-sm text-slate-700">
                        <span className="text-slate-400">{r.label} →</span> {r.name}
                      </li>
                    ))}
                  </ul>
                )}
              </div>
            ) : (
              <p className="text-slate-500 text-sm">Select a party to see its relationships.</p>
            )}
          </div>
        </div>
      )}
    </div>
  )
}
