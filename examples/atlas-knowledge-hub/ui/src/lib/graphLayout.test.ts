import { describe, expect, it } from 'vitest'
import { dedupeEdges, layoutGraph, neighbors } from './graphLayout'

const opts = { width: 800, height: 600, iterations: 60 }
const nodes = [
  { id: 'a', name: 'A', entity_type: 'Person' },
  { id: 'b', name: 'B', entity_type: 'Team' },
  { id: 'c', name: 'C', entity_type: 'Team' },
]

describe('dedupeEdges', () => {
  it('collapses_an_edge_listed_from_both_endpoints', () => {
    const e = { source: 'a', target: 'b', label: 'leads' }
    expect(dedupeEdges([e, { ...e }])).toEqual([e])
  })
  it('keeps_different_labels', () => {
    const out = dedupeEdges([
      { source: 'a', target: 'b', label: 'leads' },
      { source: 'a', target: 'b', label: 'owns' },
    ])
    expect(out).toHaveLength(2)
  })
})

describe('layoutGraph', () => {
  const edges = [
    { source: 'a', target: 'b', label: 'x' },
    { source: 'b', target: 'c', label: 'y' },
  ]
  it('is_deterministic', () => {
    expect(layoutGraph(nodes, edges, opts)).toEqual(layoutGraph(nodes, edges, opts))
  })
  it('returns_finite_coordinates_for_every_node', () => {
    const out = layoutGraph(nodes, edges, opts)
    expect(out.map((p) => p.id)).toEqual(['a', 'b', 'c'])
    for (const p of out) {
      expect(Number.isFinite(p.x)).toBe(true)
      expect(Number.isFinite(p.y)).toBe(true)
    }
  })
  it('handles_zero_and_one_node', () => {
    expect(layoutGraph([], [], opts)).toEqual([])
    const one = layoutGraph([nodes[0]], [], opts)
    expect(one).toHaveLength(1)
    expect(Number.isFinite(one[0].x)).toBe(true)
  })
  it('skips_edges_with_missing_nodes', () => {
    const out = layoutGraph(nodes, [{ source: 'a', target: 'zzz', label: 'x' }], opts)
    expect(out).toHaveLength(3)
    for (const p of out) expect(Number.isFinite(p.x)).toBe(true)
  })
})

describe('neighbors', () => {
  it('returns_both_directions', () => {
    const edges = [
      { source: 'a', target: 'b', label: 'x' },
      { source: 'c', target: 'a', label: 'y' },
      { source: 'b', target: 'c', label: 'z' },
    ]
    expect(neighbors('a', edges)).toEqual(new Set(['b', 'c']))
    expect(neighbors('q', edges)).toEqual(new Set())
  })
})
