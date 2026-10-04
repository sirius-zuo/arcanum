import { afterEach, describe, expect, it, vi } from 'vitest'
import { render, screen, within } from '@testing-library/react'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { createClient } from '../api/client'
import { ANSWER, byteSpan, DOC_ID, evidence, sentence } from '../test/verifyFixtures'
import { EvidenceView } from './EvidenceView'

vi.mock('../state/bootstrap', () => ({
  useBootstrap: () => ({ data: { collection: 'halcyon' }, client: createClient(() => 'k') }),
}))

afterEach(() => vi.unstubAllGlobals())

const TEXT = 'Café 日本語 policy: 180 days, MFA mandatory'

function stubText() {
  vi.stubGlobal(
    'fetch',
    vi.fn(async () => new Response(JSON.stringify({ document_id: DOC_ID, version_num: 2, source_uri: 'p.md', status: 'Active', mime_type: 'text/markdown', text: TEXT }), { status: 200 })),
  )
}

function show(s: ReturnType<typeof sentence>) {
  return render(
    <QueryClientProvider client={new QueryClient()}>
      <EvidenceView sentence={s} />
    </QueryClientProvider>,
  )
}

describe('EvidenceView', () => {
  it('unmatched_quote_shows_notice_and_whole_passage_range', async () => {
    stubText()
    const [s, e] = byteSpan(TEXT, 'policy: 180 days')
    const sent = sentence(ANSWER, 'It also carries 80 kg [P1].', 'unsupported', { cited: ['P1'], invalid_refs: ['P9'] }, [
      { text: 'carries 80 kg', supported: false, evidence: [evidence({ quote: 'made up quote', quote_matched: false, offset_start: s, offset_end: e, version_status: 'superseded' })] },
    ])
    show(sent)
    expect(screen.getByText('Unsupported')).toBeInTheDocument()
    expect(screen.getByText('P9')).toBeInTheDocument()
    expect(screen.getByText(/invalid refs/i)).toBeInTheDocument()
    expect(screen.getByText('not supported')).toBeInTheDocument()
    expect(screen.getByText('judge quote not found in source; showing the whole passage range')).toBeInTheDocument()
    expect(screen.getByText('quote not matched')).toBeInTheDocument()
    expect(screen.getByText('superseded')).toBeInTheDocument()
    expect((await screen.findByTestId('source-highlight')).textContent).toBe('policy: 180 days')
  })

  it('matched_quote_shows_the_badge_the_chunk_id_and_the_quote', async () => {
    stubText()
    const [s, e] = byteSpan(TEXT, '180 days')
    const sent = sentence(ANSWER, 'The HX-2 carries 60 kg [P1].', 'supported', { cited: ['P1'] }, [
      { text: 'c', supported: true, evidence: [evidence({ quote: '180 days', quote_matched: true, offset_start: s, offset_end: e })] },
    ])
    const { container } = show(sent)
    expect(screen.getByText('quote matched')).toBeInTheDocument()
    expect(screen.queryByText(/judge quote not found/)).toBeNull()
    expect(container).toHaveTextContent('22222222-2222-4222-8222-222222222222')
    expect(screen.getByRole('button', { name: /copy chunk id/i })).toBeInTheDocument()
    const mark = await screen.findByTestId('source-highlight')
    expect(mark.textContent).toBe('180 days')
    expect(within(container).getByText('supported')).toBeInTheDocument()
  })

  it('no_claim_explains_there_is_nothing_to_check', () => {
    show(sentence(ANSWER, 'Thanks!', 'no_claim'))
    expect(screen.getByText(/no factual claim/i)).toBeInTheDocument()
  })
})
