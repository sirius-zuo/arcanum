import { forceCenter, forceCollide, forceLink, forceManyBody, forceSimulation } from 'd3-force'
import type { SimulationLinkDatum, SimulationNodeDatum } from 'd3-force'
import type { GraphEdge, GraphNode } from '../api/types'

export interface LayoutOptions {
  width: number
  height: number
  iterations: number
}

export interface Placed {
  id: string
  x: number
  y: number
}

/** The server may list an edge from each endpoint; keep one per source, target and label. */
export function dedupeEdges(edges: GraphEdge[]): GraphEdge[] {
  const seen = new Set<string>()
  const out: GraphEdge[] = []
  for (const e of edges) {
    const key = `${e.source}|${e.target}|${e.label}`
    if (seen.has(key)) continue
    seen.add(key)
    out.push(e)
  }
  return out
}

export function neighbors(id: string, edges: GraphEdge[]): Set<string> {
  const out = new Set<string>()
  for (const e of edges) {
    if (e.source === id) out.add(e.target)
    else if (e.target === id) out.add(e.source)
  }
  return out
}

/** FNV-1a, mapped to [0, 1). */
function hash01(s: string, salt: string): number {
  let h = 2166136261
  for (const c of salt + s) {
    h ^= c.codePointAt(0) ?? 0
    h = Math.imul(h, 16777619)
  }
  return (h >>> 0) / 4294967296
}

interface SimNode extends SimulationNodeDatum {
  id: string
}

/**
 * Deterministic force layout: start positions come from a hash of the id and the
 * simulation runs a fixed number of ticks synchronously. Edges that point at a
 * missing node are ignored.
 */
export function layoutGraph(nodes: GraphNode[], edges: GraphEdge[], opts: LayoutOptions): Placed[] {
  const { width, height, iterations } = opts
  const sim: SimNode[] = nodes.map((n) => ({
    id: n.id,
    x: width * (0.25 + 0.5 * hash01(n.id, 'x')),
    y: height * (0.25 + 0.5 * hash01(n.id, 'y')),
  }))
  const ids = new Set(nodes.map((n) => n.id))
  const links: SimulationLinkDatum<SimNode>[] = dedupeEdges(edges)
    .filter((e) => ids.has(e.source) && ids.has(e.target))
    .map((e) => ({ source: e.source, target: e.target }))

  const simulation = forceSimulation(sim)
    .force('link', forceLink<SimNode, SimulationLinkDatum<SimNode>>(links).id((d) => d.id).distance(90))
    .force('charge', forceManyBody().strength(-260))
    .force('center', forceCenter(width / 2, height / 2))
    .force('collide', forceCollide(22))
    .stop()
  for (let i = 0; i < iterations; i++) simulation.tick()

  return sim.map((n) => ({ id: n.id, x: n.x ?? width / 2, y: n.y ?? height / 2 }))
}
