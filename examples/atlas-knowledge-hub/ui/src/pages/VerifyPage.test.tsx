import { afterEach, describe, expect, it, vi } from 'vitest'
import { fireEvent, render, screen, waitFor, within } from '@testing-library/react'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { MemoryRouter } from 'react-router-dom'
import { createClient } from '../api/client'
import type { Samples, VerifyResponse } from '../api/types'
import { byteSpan, CHUNK_ID, DOC_ID } from '../test/verifyFixtures'
import VerifyPage from './VerifyPage'

vi.mock('../state/bootstrap', () => ({
  useBootstrap: () => ({
    data: {
      collection: 'halcyon',
      judge: 'local',
      generators: [
        { name: 'local', protocol: 'openai', model: 'qwen2.5', is_default: true },
        { name: 'claude', protocol: 'anthropic', model: 'claude', is_default: false },
      ],
      features: { verify: true, context: true },
    },
    client: createClient(() => 'k'),
  }),
}))

afterEach(() => vi.unstubAllGlobals())

const ANSWER = 'The HX-2 has a payload of 60 kg [P1]. The HX-2 can carry a payload of 80 kg [P1]. The older HX-1 runs for 5 hours per charge [P1].'
const SENTENCES = ['The HX-2 has a payload of 60 kg [P1].', 'The HX-2 can carry a payload of 80 kg [P1].', 'The older HX-1 runs for 5 hours per charge [P1].']

const samples: Samples = {
  files: [],
  golden: [],
  tour: [],
  flawed_answers: [
    { id: 'supported', title: 'Every sentence is supported', question: 'What payload?', answer: 'x [P1].', expected: ['supported'] },
    { id: 'flawed', title: 'Wrong number plus a wrong-robot claim', question: 'Compare HX-2 and HX-1.', answer: ANSWER, expected: ['supported', 'unsupported', 'miscited'] },
  ],
}

const contextBody = {
  resolved_query: 'q',
  resolved_query_source: 'original',
  passages: [
    { ref_id: 'P1', document_id: DOC_ID, version_num: 1, source_uri: 'hx2-datasheet.md', chunk_ids: [CHUNK_ID], strategies: ['Vector'], score: 0.03, text: 't', offset_start: 0, offset_end: 5, snapshot_uri: '', canonical_uri: null, section: null, page: null },
  ],
  background: [],
  usage: { budget: 4000, used: 10, passages: 10, background: 0, dropped_passages: 0, counter: 'c' },
  retrieval: { queries: ['q'], strategies_ok: ['Vector'], strategies_failed: [] },
}

const verdicts = ['supported', 'unsupported', 'supported'] as const
const verifyBody: VerifyResponse = {
  verdict: 'fail',
  strict_citations: false,
  counts: { supported: 2, miscited: 0, uncited_supported: 0, partial: 0, unsupported: 1, no_claim: 0 },
  sentences: SENTENCES.map((t, i) => ({ span: byteSpan(ANSWER, t), text: t, verdict: verdicts[i], cited: ['P1'], invalid_refs: [], claims: [] })),
  passages_unavailable: [],
  judge: { name: 'local', model: 'qwen2.5' },
  usage: { input_tokens: 10, output_tokens: 5, judge_calls: 1 },
}

function stub(verify: () => Response = () => new Response(JSON.stringify(verifyBody), { status: 200 })) {
  const fn = vi.fn(async (url: string) => {
    if (url === '/demo/samples') return new Response(JSON.stringify(samples), { status: 200 })
    if (url === '/api/v1/context') return new Response(JSON.stringify(contextBody), { status: 200 })
    if (url === '/api/v1/verify') return verify()
    return new Response('{"error":"nope"}', { status: 404 })
  })
  vi.stubGlobal('fetch', fn)
  return fn
}

function body(fn: ReturnType<typeof stub>, path: string): Record<string, unknown> {
  const call = fn.mock.calls.find((c) => c[0] === path) as unknown as [string, RequestInit]
  return JSON.parse(call[1].body as string) as Record<string, unknown>
}

function renderPage() {
  return render(
    <QueryClientProvider client={new QueryClient({ defaultOptions: { queries: { retry: false } } })}>
      <MemoryRouter>
        <VerifyPage />
      </MemoryRouter>
    </QueryClientProvider>,
  )
}

async function pickFlawed() {
  await screen.findByRole('option', { name: /wrong number/i })
  const picker = await screen.findByLabelText(/prepared answer/i)
  fireEvent.change(picker, { target: { value: 'flawed' } })
}

describe('VerifyPage', () => {
  it('shows_expected_vs_actual_chips_and_calls_context_then_verify', async () => {
    const fn = stub()
    renderPage()
    await pickFlawed()
    expect(screen.getByLabelText(/^answer$/i)).toHaveValue(ANSWER)
    expect(screen.getByDisplayValue('Compare HX-2 and HX-1.')).toBeInTheDocument()
    fireEvent.click(screen.getByRole('button', { name: /run verification/i }))
    const note = await screen.findByRole('region', { name: /expected verdicts/i })
    const matches = within(note).getAllByText(/^match$/i)
    const mismatches = within(note).getAllByText(/^mismatch$/i)
    expect(matches).toHaveLength(2)
    expect(mismatches).toHaveLength(1)
    expect(within(note).getByText(/expected miscited, got supported/i)).toBeInTheDocument()

    const ctx = body(fn, '/api/v1/context')
    expect(ctx).toMatchObject({ collection_id: 'halcyon', query: 'Compare HX-2 and HX-1.', token_budget: 4000, background_share: 0.2, candidate_k: 50, render: 'xml' })
    const ver = body(fn, '/api/v1/verify')
    expect(ver).toEqual({ collection_id: 'halcyon', answer: ANSWER, passages: [{ ref_id: 'P1', chunk_ids: [CHUNK_ID] }] })
    expect(screen.getByText('hx2-datasheet.md')).toBeInTheDocument()
  })

  it('sends_judge_and_strict_only_when_changed', async () => {
    const fn = stub()
    renderPage()
    await pickFlawed()
    fireEvent.change(screen.getByLabelText(/^judge$/i), { target: { value: 'claude' } })
    fireEvent.click(screen.getByLabelText(/strict_citations/i))
    fireEvent.click(screen.getByRole('button', { name: /run verification/i }))
    await screen.findByRole('region', { name: /expected verdicts/i })
    expect(body(fn, '/api/v1/verify')).toMatchObject({ judge: 'claude', strict_citations: true })
  })

  it('shows_the_server_message_for_a_400', async () => {
    stub(() => new Response(JSON.stringify({ error: 'passages exceed the judge input budget' }), { status: 400 }))
    renderPage()
    await pickFlawed()
    fireEvent.click(screen.getByRole('button', { name: /run verification/i }))
    await waitFor(() => expect(screen.getByRole('alert')).toHaveTextContent('passages exceed the judge input budget'))
  })

  it('lets_the_user_rebuild_passages_with_a_larger_candidate_k', async () => {
    const fn = stub()
    renderPage()
    await pickFlawed()
    fireEvent.click(screen.getByRole('button', { name: /run verification/i }))
    await screen.findByRole('region', { name: /expected verdicts/i })
    fireEvent.change(screen.getByLabelText(/candidate_k/i), { target: { value: '120' } })
    fireEvent.click(screen.getByRole('button', { name: /rebuild passages/i }))
    await waitFor(() => expect(fn.mock.calls.filter((c) => c[0] === '/api/v1/context')).toHaveLength(2))
    const second = (fn.mock.calls.filter((c) => c[0] === '/api/v1/context')[1] as unknown as [string, RequestInit])[1]
    expect(JSON.parse(second.body as string)).toMatchObject({ candidate_k: 120 })
    expect(screen.getAllByText(/needs the HX-1 datasheet/i).length).toBeGreaterThan(0)
  })
})
