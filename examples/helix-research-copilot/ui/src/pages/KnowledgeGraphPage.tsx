import { useEffect, useState } from 'react'
import { fetchGraph, GraphView, GraphNode } from '../api/graph'
import { RefreshCw, Network } from 'lucide-react'

// Color by entity type.
function nodeColor(type: string): string {
  const t = type.toLowerCase()
  if (t.includes('compound')) return '#60a5fa'   // blue
  if (t.includes('protein')) return '#34d399'    // green
  if (t.includes('gene')) return '#a78bfa'       // purple
  if (t.includes('pathway')) return '#fbbf24'    // yellow
  return '#94a3b8'                               // slate default
}

export default function KnowledgeGraphPage() {
  const [graph, setGraph] = useState<GraphView>({ nodes: [], edges: [] })
  const [loading, setLoading] = useState(false)
  const [selected, setSelected] = useState<GraphNode | null>(null)

  async function load() {
    setLoading(true)
    try { setGraph(await fetchGraph()) }
    finally { setLoading(false) }
  }
  useEffect(() => { load() }, [])

  // Radial layout: place nodes evenly on a circle.
  const W = 720, H = 520, cx = W / 2, cy = H / 2, R = Math.min(W, H) / 2 - 60
  const n = graph.nodes.length
  const pos = new Map<string, { x: number; y: number }>()
  graph.nodes.forEach((node, i) => {
    const angle = (2 * Math.PI * i) / Math.max(n, 1) - Math.PI / 2
    pos.set(node.id, { x: cx + R * Math.cos(angle), y: cy + R * Math.sin(angle) })
  })

  return (
    <div>
      <div className="flex items-center justify-between mb-6">
        <div>
          <h1 className="text-2xl font-semibold text-slate-100 flex items-center gap-2">
            <Network size={20} className="text-purple-400" /> Knowledge Graph
          </h1>
          <p className="text-slate-500 text-sm">Entities and relationships extracted during ingestion.</p>
        </div>
        <button onClick={load} className="flex items-center gap-2 text-sm text-slate-400 hover:text-slate-200">
          <RefreshCw size={14} className={loading ? 'animate-spin' : ''} /> Refresh
        </button>
      </div>

      {graph.nodes.length === 0 ? (
        <div className="text-slate-500 text-sm p-8 text-center border border-dashed border-slate-700 rounded-xl">
          No entities yet. Ingest research papers on the Corpus page to populate the graph.
        </div>
      ) : (
        <div className="flex gap-6">
          <svg width={W} height={H} className="bg-[#0f0f16] border border-slate-800 rounded-xl flex-shrink-0">
            {/* Edges */}
            {graph.edges.map((e, i) => {
              const a = pos.get(e.source), b = pos.get(e.target)
              if (!a || !b) return null
              return (
                <g key={i}>
                  <line x1={a.x} y1={a.y} x2={b.x} y2={b.y} stroke="#334155" strokeWidth={1} />
                  <text x={(a.x + b.x) / 2} y={(a.y + b.y) / 2} fill="#64748b" fontSize={9} textAnchor="middle">{e.label}</text>
                </g>
              )
            })}
            {/* Nodes */}
            {graph.nodes.map(node => {
              const p = pos.get(node.id)!
              return (
                <g key={node.id} onClick={() => setSelected(node)} className="cursor-pointer">
                  <circle cx={p.x} cy={p.y} r={10} fill={nodeColor(node.entity_type)} stroke="#0f0f16" strokeWidth={2} />
                  <text x={p.x} y={p.y - 16} fill="#cbd5e1" fontSize={11} textAnchor="middle" className="font-mono">{node.name}</text>
                </g>
              )
            })}
          </svg>

          <div className="flex-1">
            {selected ? (
              <div className="bg-[#0f0f16] border border-slate-800 rounded-xl p-5">
                <div className="text-lg font-mono text-slate-100">{selected.name}</div>
                <div className="text-sm mt-1" style={{ color: nodeColor(selected.entity_type) }}>{selected.entity_type}</div>
                <div className="text-xs text-slate-600 mt-3 font-mono break-all">{selected.id}</div>
              </div>
            ) : (
              <p className="text-slate-500 text-sm">Click a node to inspect the entity.</p>
            )}
            <div className="mt-4 text-xs text-slate-500">
              {graph.nodes.length} entities · {graph.edges.length} relationships
            </div>
          </div>
        </div>
      )}
    </div>
  )
}
