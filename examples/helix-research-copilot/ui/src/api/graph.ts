import { arcanumFetch } from './client'

export interface GraphNode {
  id: string
  name: string
  entity_type: string
}

export interface GraphEdge {
  source: string
  target: string
  label: string
}

export interface GraphView {
  nodes: GraphNode[]
  edges: GraphEdge[]
}

// Finding #1: throws on non-ok so callers can distinguish failure from empty corpus.
export async function fetchGraph(): Promise<GraphView> {
  const res = await arcanumFetch('/api/v1/graph')
  if (!res.ok) throw new Error(`Graph API error: ${res.status}`)
  return res.json()
}
