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

export async function fetchGraph(): Promise<GraphView> {
  const res = await arcanumFetch('/api/v1/graph')
  if (!res.ok) return { nodes: [], edges: [] }
  return res.json()
}
