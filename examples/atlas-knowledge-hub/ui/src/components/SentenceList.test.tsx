import { describe, expect, it, vi } from 'vitest'
import { fireEvent, render, screen } from '@testing-library/react'
import type { VerifiedSentence } from '../api/types'
import { ANSWER, response } from '../test/verifyFixtures'
import { SentenceList } from './SentenceList'

describe('SentenceList', () => {
  it('maps_spans_to_verdict_tones_and_selects_on_click', () => {
    const res = response()
    const onSelect = vi.fn()
    const { container } = render(<SentenceList answer={ANSWER} sentences={res.sentences} selected={null} onSelect={onSelect} />)
    const items = screen.getAllByRole('button')
    expect(items).toHaveLength(3)
    expect(items[0]).toHaveAccessibleName(/The HX-2 carries 60 kg \[P1\]\s*\..*Supported/)
    expect(items[1]).toHaveAccessibleName(/It also carries 80 kg \[P1\]\s*\..*Unsupported/)
    expect(items[2]).toHaveAccessibleName(/Thanks!.*No claim/)
    expect(items[0]).toHaveAttribute('data-verdict', 'supported')
    expect(items[1]).toHaveAttribute('data-verdict', 'unsupported')
    expect(items[1].className).toContain('decoration-v-unsupported')
    // no_claim stays calm: no alarming tone anywhere on it
    expect(items[2].className).not.toMatch(/v-(unsupported|miscited|partial)/)
    // the multibyte sentence in the middle is plain text, covered by no span, and still rendered
    expect(container).toHaveTextContent(`${'日本語 is fine.'}`)
    fireEvent.click(items[1])
    expect(onSelect).toHaveBeenCalledWith(1)
    fireEvent.keyDown(items[2], { key: 'Enter' })
    expect(onSelect).toHaveBeenCalledWith(2)
  })

  it('underlines_the_right_sentences_in_a_multibyte_answer', () => {
    const answer = 'Acme 株式会社は成立于1998年。[P1] Zwei Sätze für über 3 Mäuse.'
    const enc = new TextEncoder()
    const a = 'Acme 株式会社は成立于1998年。[P1]'
    const b = 'Zwei Sätze für über 3 Mäuse.'
    const s0 = enc.encode(a).length
    const s1 = enc.encode(answer.slice(0, answer.indexOf(b))).length
    const sentences: VerifiedSentence[] = [
      { span: [0, s0], text: a, verdict: 'supported', cited: ['P1'], invalid_refs: [], claims: [] },
      { span: [s1, s1 + enc.encode(b).length], text: b, verdict: 'partial', cited: [], invalid_refs: [], claims: [] },
    ]
    render(<SentenceList answer={answer} sentences={sentences} selected={1} onSelect={() => {}} />)
    const items = screen.getAllByRole('button')
    expect(items[0].textContent).toContain(a)
    expect(items[1].textContent).toContain(b)
    expect(items[1]).toHaveAttribute('aria-pressed', 'true')
    expect(items[0]).toHaveAttribute('aria-pressed', 'false')
  })

  it('renders_text_outside_every_span_plain', () => {
    const answer = '# Heading\n\nSupported fact [P1].\n\n```\ncode\n```'
    const enc = new TextEncoder()
    const fact = 'Supported fact [P1].'
    const start = enc.encode(answer.slice(0, answer.indexOf(fact))).length
    render(
      <SentenceList
        answer={answer}
        sentences={[{ span: [start, start + fact.length], text: fact, verdict: 'supported', cited: ['P1'], invalid_refs: [], claims: [] }]}
        selected={null}
        onSelect={() => {}}
      />,
    )
    expect(screen.getAllByRole('button')).toHaveLength(1)
    expect(screen.getByTestId('sentence-list')).toHaveTextContent('# Heading')
    expect(screen.getByTestId('sentence-list')).toHaveTextContent('code')
  })
})
