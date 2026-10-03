import { afterEach, describe, expect, it, vi } from 'vitest'
import { fireEvent, render, screen } from '@testing-library/react'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { createClient } from '../api/client'
import { ANSWER, response } from '../test/verifyFixtures'
import { MemoryRouter } from 'react-router-dom'
import { TurnCard } from './TurnCard'
import { INITIAL_ASK } from '../state/ask'
import type { AskState } from '../state/ask'
import type { ContextResponse, GenerateDone } from '../api/types'

vi.mock('../state/bootstrap', () => ({
  useBootstrap: () => ({ data: { collection: 'halcyon' }, client: createClient(() => 'k') }),
}))

afterEach(() => vi.unstubAllGlobals())

const done = (over: Partial<GenerateDone> = {}): GenerateDone => ({
  status: 'ok',
  citations: [],
  unknown_refs: [],
  stop_reason: 'end_turn',
  usage: { input_tokens: 10, output_tokens: 5 },
  generator: { name: 'local', model: 'qwen2.5' },
  ...over,
})

const ctx = { passages: [], background: [] } as unknown as ContextResponse

function show(state: Partial<AskState>, verify = false) {
  return render(
    <QueryClientProvider client={new QueryClient()}>
      <MemoryRouter>
        <TurnCard meta={{ question: 'q?', mode: 'answer', verify }} state={{ ...INITIAL_ASK, phase: 'done', ...state }} />
      </MemoryRouter>
    </QueryClientProvider>,
  )
}

describe('TurnCard', () => {
  it('no_context_shows_the_message_and_a_corpus_hint', () => {
    show({ answer: 'No relevant information was found.', context: ctx, outcome: done({ status: 'no_context' }), verification: null }, true)
    expect(screen.getByText('No relevant information was found.')).toBeInTheDocument()
    expect(screen.getByRole('link', { name: /load it in the library/i })).toBeInTheDocument()
  })

  it('judge_failure_shows_a_banner_and_keeps_the_answer', () => {
    show(
      { answer: 'Sixty days [P9].', context: ctx, outcome: done({ unknown_refs: ['P9'] }), verification: { status: 'error', code: 'judge_timeout', message: 'judge timed out' } },
      true,
    )
    expect(screen.getByText(/Sixty days/)).toBeInTheDocument()
    expect(screen.getByRole('alert')).toHaveTextContent(/judge timed out/)
    expect(screen.getByText(/unknown references/i)).toHaveTextContent('P9')
    expect(screen.getByRole('button', { name: /P9.*unknown/i })).toBeInTheDocument()
  })

  it('pre_stream_verify_503_shows_the_exact_server_message', () => {
    show({ phase: 'error', error: 'verification requires a configured judge', errorStatus: 503 }, true)
    expect(screen.getByRole('alert')).toHaveTextContent('verification requires a configured judge')
    expect(screen.getByText('Verification is unavailable')).toBeInTheDocument()
  })

  it('shows_the_outcome_strip', () => {
    show({ answer: 'Hi', context: ctx, outcome: done(), ttft: 420, elapsed: 2300 })
    const strip = screen.getByRole('group', { name: /outcome/i })
    expect(strip).toHaveTextContent('ok')
    expect(strip).toHaveTextContent('end_turn')
    expect(strip).toHaveTextContent('tokens in10')
    expect(strip).toHaveTextContent('out5')
    expect(strip).toHaveTextContent('local (qwen2.5)')
    expect(strip).toHaveTextContent('420 ms')
    expect(strip).toHaveTextContent('2.3 s')
  })

  it('verification_ok_renders_the_overall_verdict_and_underlined_sentences', () => {
    show({ answer: ANSWER, context: ctx, outcome: done(), verification: { status: 'ok', ...response() } }, true)
    expect(screen.getByText(/Verified: fail/)).toBeInTheDocument()
    const sentences = screen.getByTestId('sentence-list')
    expect(sentences).toHaveTextContent('日本語 is fine.')
    expect(screen.getAllByRole('button', { name: /Unsupported/ })).toHaveLength(1)
    fireEvent.click(screen.getByRole('button', { name: /It also carries 80 kg/ }))
    expect(screen.getByText('carries 80 kg')).toBeInTheDocument()
  })

  it('verification_requested_but_missing_shows_a_notice', () => {
    show({ answer: 'Sixty days [P1].', context: ctx, outcome: done() }, true)
    expect(screen.getByRole('status')).toHaveTextContent(/verification was requested but no result arrived/i)
  })

  it('stop_after_done_does_not_claim_the_answer_is_incomplete', () => {
    show({ phase: 'idle', stopped: true, answer: 'Sixty days.', context: ctx, outcome: done() })
    expect(screen.queryByText(/text above is incomplete/i)).toBeNull()
  })

  it('duplicate_citation_markers_do_not_collide', () => {
    const spy = vi.spyOn(console, 'error').mockImplementation(() => {})
    show({ answer: 'Both say so [P1, P1].', context: ctx, outcome: done() })
    expect(spy.mock.calls.flat().join(' ')).not.toMatch(/same key/i)
    spy.mockRestore()
  })
})
