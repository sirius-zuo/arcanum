import { afterEach, describe, expect, it, vi } from 'vitest'
import { fireEvent, render, screen } from '@testing-library/react'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { createClient } from '../api/client'
import type { ProofChain } from '../api/types'
import { byteSpan, DOC_ID } from '../test/verifyFixtures'
import { ProofTree } from './ProofTree'

vi.mock('../state/bootstrap', () => ({
  useBootstrap: () => ({ data: { collection: 'halcyon' }, client: createClient(() => 'k') }),
}))

afterEach(() => vi.unstubAllGlobals())

const TEXT = 'Café 日本語 policy: 180 days, MFA mandatory'
const [S, E] = byteSpan(TEXT, '180 days')

const chain: ProofChain = {
  root: {
    id: 'chunk-1',
    kind: 'Chunk',
    label: 'Chunk 1',
    metadata: { score: 0.5 },
    children: [
      { id: 'node-1', kind: 'TreeNode', label: 'Summary level 1', metadata: {}, children: [{ id: 'ent-1', kind: 'Entity', label: 'Acme Corp', metadata: { entity_type: 'Org' }, children: [] }] },
    ],
  },
  raw_sources: [
    { document_id: DOC_ID, version_num: 2, source_uri: 'policy.md', snapshot_uri: 's', canonical_uri: null, page: null, section: 'Security', block_ids: [], offset_start: S, offset_end: E },
  ],
}

describe('ProofTree', () => {
  it('renders_nested_nodes_and_raw_sources', async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn(async () => new Response(JSON.stringify({ document_id: DOC_ID, version_num: 2, source_uri: 'policy.md', status: 'Active', mime_type: 'text/markdown', text: TEXT }), { status: 200 })),
    )
    render(
      <QueryClientProvider client={new QueryClient()}>
        <ProofTree chain={chain} />
      </QueryClientProvider>,
    )
    expect(screen.getByText('Chunk 1')).toBeInTheDocument()
    expect(screen.getByText('Summary level 1')).toBeInTheDocument()
    expect(screen.getByText('Acme Corp')).toBeInTheDocument()
    expect(screen.getAllByText('Entity').length).toBeGreaterThan(0)
    expect(screen.getByText('policy.md')).toBeInTheDocument()

    // collapse hides descendants
    fireEvent.click(screen.getByRole('button', { name: /collapse chunk 1/i }))
    expect(screen.queryByText('Acme Corp')).toBeNull()
    fireEvent.click(screen.getByRole('button', { name: /expand chunk 1/i }))
    expect(screen.getByText('Acme Corp')).toBeInTheDocument()

    // metadata JSON toggle
    expect(screen.queryByText(/"entity_type"/)).toBeNull()
    fireEvent.click(screen.getAllByRole('button', { name: /show metadata/i })[2])
    expect(screen.getByText(/"entity_type"/)).toBeInTheDocument()

    // raw source opens the viewer at the byte range
    fireEvent.click(screen.getByRole('button', { name: /show source for policy\.md/i }))
    expect((await screen.findByTestId('source-highlight')).textContent).toBe('180 days')
  })
})
