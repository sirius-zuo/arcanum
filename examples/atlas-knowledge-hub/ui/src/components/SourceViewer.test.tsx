import { afterEach, describe, expect, it, vi } from 'vitest'
import { render, screen, waitFor } from '@testing-library/react'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { createClient } from '../api/client'
import { byteSpan, DOC_ID } from '../test/verifyFixtures'
import { SourceViewer } from './SourceViewer'

vi.mock('../state/bootstrap', () => ({
  useBootstrap: () => ({ data: { collection: 'halcyon' }, client: createClient(() => 'k') }),
}))

afterEach(() => vi.unstubAllGlobals())

const TEXT = 'Café 日本語 policy: 180 days, MFA mandatory'

function stubText(text: string, status = 200) {
  vi.stubGlobal(
    'fetch',
    vi.fn(async () =>
      status === 200
        ? new Response(JSON.stringify({ document_id: DOC_ID, version_num: 1, source_uri: 'p.md', status: 'Active', mime_type: 'text/markdown', text }), { status })
        : new Response(JSON.stringify({ error: 'not found' }), { status }),
    ),
  )
}

function show(start: number, end: number) {
  return render(
    <QueryClientProvider client={new QueryClient()}>
      <SourceViewer documentId={DOC_ID} version={1} start={start} end={end} />
    </QueryClientProvider>,
  )
}

describe('SourceViewer', () => {
  it('highlights_byte_range_in_multibyte_text', async () => {
    stubText(TEXT)
    const [s, e] = byteSpan(TEXT, '180 days')
    expect(s).toBeGreaterThan('Café 日本語 policy: '.length) // bytes, not UTF-16 indices
    show(s, e)
    const mark = await screen.findByTestId('source-highlight')
    expect(mark.textContent).toBe('180 days')
    expect(screen.getByRole('region', { name: /source text/i })).toHaveTextContent(TEXT)
  })

  it('highlights_a_range_at_the_very_end_and_a_four_byte_character', async () => {
    const text = 'start 😀 end'
    stubText(text)
    const [s, e] = byteSpan(text, '😀 end')
    show(s, e)
    expect((await screen.findByTestId('source-highlight')).textContent).toBe('😀 end')
  })

  it('shows_an_error_state_when_the_text_is_missing', async () => {
    stubText('', 404)
    show(0, 3)
    await waitFor(() => expect(screen.getByRole('alert')).toHaveTextContent(/source text/i))
  })
})
