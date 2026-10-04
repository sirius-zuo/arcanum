import { Minus, Plus, Search } from 'lucide-react'
import { useEffect, useId, useMemo, useRef, useState } from 'react'
import type { KeyboardEvent, PointerEvent } from 'react'
import { Link } from 'react-router-dom'
import type { GraphView } from '../api/types'
import { dedupeEdges, layoutGraph, neighbors } from '../lib/graphLayout'
import { Chip } from './Chip'

const W = 900
const H = 600
const PALETTE = ['--n1', '--n2', '--n3', '--n4', '--n5', '--n6']
const MIN_K = 0.3
const MAX_K = 4

interface Transform {
  x: number
  y: number
  k: number
}

const clampK = (k: number) => Math.min(MAX_K, Math.max(MIN_K, k))
const fill = (v: string) => `rgb(var(${v}))`

export function GraphCanvas({ graph }: { graph: GraphView }) {
  const edges = useMemo(() => dedupeEdges(graph.edges), [graph.edges])
  const positions = useMemo(() => {
    const m = new Map<string, { x: number; y: number }>()
    for (const p of layoutGraph(graph.nodes, edges, { width: W, height: H, iterations: 300 })) m.set(p.id, p)
    return m
  }, [graph.nodes, edges])
  const byId = useMemo(() => new Map(graph.nodes.map((n) => [n.id, n])), [graph.nodes])
  const types = useMemo(() => [...new Set(graph.nodes.map((n) => n.entity_type))].sort(), [graph.nodes])
  const colorOf = (type: string) => PALETTE[types.indexOf(type) % PALETTE.length]

  const [selected, setSelected] = useState<string | null>(null)
  const [hoverEdge, setHoverEdge] = useState<number | null>(null)
  const [query, setQuery] = useState('')
  const [t, setT] = useState<Transform>({ x: 0, y: 0, k: 1 })
  const svgRef = useRef<SVGSVGElement>(null)
  const drag = useRef<{ x: number; y: number } | null>(null)
  const panned = useRef(false)
  const listId = useId()

  // Wheel zoom needs a non-passive listener to stop the page from scrolling.
  useEffect(() => {
    const el = svgRef.current
    if (!el) return
    const onWheel = (e: WheelEvent) => {
      e.preventDefault()
      const f = e.deltaY < 0 ? 1.1 : 1 / 1.1
      setT((c) => zoomAbout(c, f, W / 2, H / 2))
    }
    el.addEventListener('wheel', onWheel, { passive: false })
    return () => el.removeEventListener('wheel', onWheel)
  }, [])

  const near = useMemo(() => (selected ? neighbors(selected, edges) : null), [selected, edges])
  const matches = useMemo(() => {
    const q = query.trim().toLowerCase()
    if (!q) return []
    return graph.nodes.filter((n) => n.name.toLowerCase().includes(q)).slice(0, 8)
  }, [query, graph.nodes])

  const select = (id: string | null) => {
    setSelected(id)
    setHoverEdge(null)
  }

  const focusNode = (id: string) => {
    const p = positions.get(id)
    if (p) setT((c) => ({ ...c, x: W / 2 - p.x * c.k, y: H / 2 - p.y * c.k }))
    select(id)
    setQuery('')
  }

  const onPointerDown = (e: PointerEvent<SVGSVGElement>) => {
    drag.current = { x: e.clientX, y: e.clientY }
    panned.current = false
    e.currentTarget.setPointerCapture?.(e.pointerId)
  }
  const onPointerMove = (e: PointerEvent<SVGSVGElement>) => {
    const d = drag.current
    if (!d) return
    const rect = e.currentTarget.getBoundingClientRect()
    const scale = rect.width > 0 ? W / rect.width : 1
    const dx = (e.clientX - d.x) * scale
    const dy = (e.clientY - d.y) * scale
    if (Math.abs(dx) + Math.abs(dy) > 0) panned.current = true
    d.x = e.clientX
    d.y = e.clientY
    setT((c) => ({ ...c, x: c.x + dx, y: c.y + dy }))
  }
  const onPointerUp = () => {
    drag.current = null
  }

  const onNodeKey = (e: KeyboardEvent, id: string) => {
    if (e.key === 'Enter' || e.key === ' ') {
      e.preventDefault()
      select(id)
    }
  }

  const sel = selected ? byId.get(selected) : undefined
  const selEdges = selected ? edges.filter((e) => e.source === selected || e.target === selected) : []
  const dim = (id: string) => selected !== null && id !== selected && !near?.has(id)

  return (
    <div className="grid gap-4 lg:grid-cols-[minmax(0,1fr)_320px]">
      <div className="min-w-0">
        <div className="mb-3 flex flex-wrap items-center gap-3">
          <div className="relative w-full max-w-xs">
            <label htmlFor={`${listId}-q`} className="sr-only">
              Find entity
            </label>
            <Search className="pointer-events-none absolute left-2.5 top-2.5 h-4 w-4 text-muted" aria-hidden="true" />
            <input
              id={`${listId}-q`}
              type="search"
              value={query}
              onChange={(e) => setQuery(e.target.value)}
              placeholder="Find entity..."
              aria-controls={matches.length > 0 ? `${listId}-r` : undefined}
              className="h-9 w-full rounded-lg border border-border bg-surface pl-8 pr-2 text-sm"
            />
            {matches.length > 0 && (
              <ul
                id={`${listId}-r`}
                aria-label="Matching entities"
                className="absolute z-10 mt-1 w-full overflow-hidden rounded-lg border border-border bg-surface shadow-lift"
              >
                {matches.map((n) => (
                  <li key={n.id}>
                    <button
                      type="button"
                      onClick={() => focusNode(n.id)}
                      className="flex w-full items-center justify-between gap-2 px-3 py-1.5 text-left text-sm hover:bg-surface-2"
                    >
                      <span className="truncate">{n.name}</span>
                      <span className="text-xs text-muted">{n.entity_type}</span>
                    </button>
                  </li>
                ))}
              </ul>
            )}
            {query.trim() !== '' && matches.length === 0 && (
              <p className="absolute mt-1 text-xs text-muted" role="status">
                No entity matches.
              </p>
            )}
          </div>
          <div className="flex items-center gap-1" role="group" aria-label="Zoom">
            <button type="button" aria-label="Zoom in" onClick={() => setT((c) => zoomAbout(c, 1.25, W / 2, H / 2))} className="grid h-9 w-9 place-items-center rounded-lg border border-border bg-surface hover:bg-surface-2">
              <Plus className="h-4 w-4" aria-hidden="true" />
            </button>
            <button type="button" aria-label="Zoom out" onClick={() => setT((c) => zoomAbout(c, 0.8, W / 2, H / 2))} className="grid h-9 w-9 place-items-center rounded-lg border border-border bg-surface hover:bg-surface-2">
              <Minus className="h-4 w-4" aria-hidden="true" />
            </button>
            <button type="button" onClick={() => setT({ x: 0, y: 0, k: 1 })} className="h-9 rounded-lg border border-border bg-surface px-3 text-xs hover:bg-surface-2">
              Reset view
            </button>
          </div>
          <ul className="flex flex-wrap gap-1.5" aria-label="Entity types">
            {types.map((ty) => (
              <li key={ty}>
                <Chip icon={<span className="h-2.5 w-2.5 rounded-full" style={{ background: fill(colorOf(ty)) }} aria-hidden="true" />}>{ty}</Chip>
              </li>
            ))}
          </ul>
        </div>

        <div className="overflow-hidden rounded-card border border-border bg-surface">
          <svg
            ref={svgRef}
            viewBox={`0 0 ${W} ${H}`}
            role="group"
            aria-label={`Knowledge graph with ${graph.nodes.length} entities and ${edges.length} relations. Tab to a node and press Enter to inspect it.`}
            className="block h-[min(70vh,600px)] w-full cursor-grab touch-none select-none active:cursor-grabbing"
            onPointerDown={onPointerDown}
            onPointerMove={onPointerMove}
            onPointerUp={onPointerUp}
            onPointerCancel={onPointerUp}
            onClick={(e) => {
              if (e.target === e.currentTarget && !panned.current) select(null)
            }}
          >
            <g transform={`translate(${t.x} ${t.y}) scale(${t.k})`}>
              {edges.map((e, i) => {
                const a = positions.get(e.source)
                const b = positions.get(e.target)
                if (!a || !b) return null
                const active = selected !== null && (e.source === selected || e.target === selected)
                const faded = selected !== null && !active
                const showLabel = hoverEdge === i || active
                return (
                  <g key={`${e.source}|${e.target}|${e.label}`} opacity={faded ? 0.15 : 1}>
                    <line x1={a.x} y1={a.y} x2={b.x} y2={b.y} stroke="rgb(var(--muted))" strokeOpacity={active ? 0.9 : 0.5} strokeWidth={active ? 2 : 1.2} />
                    <line x1={a.x} y1={a.y} x2={b.x} y2={b.y} stroke="transparent" strokeWidth={10} onMouseEnter={() => setHoverEdge(i)} onMouseLeave={() => setHoverEdge(null)}>
                      <title>{e.label}</title>
                    </line>
                    {showLabel && (
                      <text x={(a.x + b.x) / 2} y={(a.y + b.y) / 2 - 4} textAnchor="middle" fontSize={11} fill="rgb(var(--text))" stroke="rgb(var(--surface))" strokeWidth={3} paintOrder="stroke">
                        {e.label}
                      </text>
                    )}
                  </g>
                )
              })}
              {graph.nodes.map((n) => {
                const p = positions.get(n.id)
                if (!p) return null
                const isSel = n.id === selected
                return (
                  <g
                    key={n.id}
                    data-node={n.id}
                    transform={`translate(${p.x} ${p.y})`}
                    role="button"
                    tabIndex={0}
                    aria-label={`${n.name}, ${n.entity_type}`}
                    aria-pressed={isSel}
                    opacity={dim(n.id) ? 0.2 : 1}
                    className="cursor-pointer outline-none [&:focus-visible>circle:first-child]:stroke-[rgb(var(--accent))]"
                    onClick={(e) => {
                      e.stopPropagation()
                      select(n.id)
                    }}
                    onPointerDown={(e) => e.stopPropagation()}
                    onKeyDown={(e) => onNodeKey(e, n.id)}
                  >
                    <circle r={isSel ? 13 : 10} fill={fill(colorOf(n.entity_type))} stroke={isSel ? 'rgb(var(--text))' : 'rgb(var(--surface))'} strokeWidth={isSel ? 3 : 2} />
                    <text y={26} textAnchor="middle" fontSize={11} fill="rgb(var(--text))" stroke="rgb(var(--surface))" strokeWidth={3} paintOrder="stroke">
                      {n.name}
                    </text>
                  </g>
                )
              })}
            </g>
          </svg>
        </div>
      </div>

      <aside aria-label="Entity inspector" aria-live="polite" className="rounded-card border border-border bg-surface p-4 text-sm">
        {sel ? (
          <div className="space-y-4">
            <div>
              <h2 className="break-words text-base font-semibold">{sel.name}</h2>
              <p className="mt-1 flex items-center gap-2 text-muted">
                <span className="h-2.5 w-2.5 rounded-full" style={{ background: fill(colorOf(sel.entity_type)) }} aria-hidden="true" />
                {sel.entity_type}
              </p>
            </div>
            <div>
              <h3 className="mb-1 text-xs font-medium uppercase tracking-wide text-muted">Connections ({selEdges.length})</h3>
              {selEdges.length === 0 ? (
                <p className="text-muted">No relations.</p>
              ) : (
                <ul className="space-y-1">
                  {selEdges.map((e) => {
                    const out = e.source === sel.id
                    const otherId = out ? e.target : e.source
                    const other = byId.get(otherId)
                    return (
                      <li key={`${e.source}|${e.target}|${e.label}`} className="flex flex-wrap items-center gap-1">
                        <span className="text-muted">{out ? 'to' : 'from'}</span>
                        <button type="button" onClick={() => focusNode(otherId)} className="font-medium text-accent underline-offset-2 hover:underline">
                          {other?.name ?? otherId}
                        </button>
                        <Chip mono>{e.label}</Chip>
                      </li>
                    )
                  })}
                </ul>
              )}
            </div>
            <Link to={`/evidence?entity=${encodeURIComponent(sel.id)}`} className="inline-flex h-9 items-center rounded-lg bg-accent px-4 text-sm font-medium text-accent-fg hover:opacity-90">
              Open evidence
            </Link>
          </div>
        ) : (
          <p className="text-muted">Select an entity to see its type, connections and evidence. Scroll to zoom, drag to pan.</p>
        )}
      </aside>
    </div>
  )
}

function zoomAbout(c: Transform, f: number, cx: number, cy: number): Transform {
  const k = clampK(c.k * f)
  const r = k / c.k
  return { k, x: cx - (cx - c.x) * r, y: cy - (cy - c.y) * r }
}
