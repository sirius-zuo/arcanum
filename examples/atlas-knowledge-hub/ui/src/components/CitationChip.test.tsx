import { describe, expect, it, vi } from 'vitest'
import { fireEvent, render, screen } from '@testing-library/react'
import { CitationChip } from './CitationChip'
import type { ContextPassage } from '../api/types'

const passage: ContextPassage = {
  ref_id: 'P1',
  document_id: 'd1',
  version_num: 3,
  source_uri: 'security-policy.md',
  snapshot_uri: 's',
  canonical_uri: null,
  section: null,
  page: null,
  offset_start: 120,
  offset_end: 480,
  text: 'Passwords rotate every 90 days.',
  chunk_ids: ['c1'],
  strategies: ['vector'],
  score: 0.03,
}

describe('CitationChip', () => {
  it('unknown_ref_has_warning_state_and_label', () => {
    render(<CitationChip refId="P9" onSelect={() => {}} />)
    const btn = screen.getByRole('button', { name: /P9.*unknown/i })
    expect(btn).toHaveAttribute('data-state', 'unknown')
    expect(screen.getByRole('tooltip')).toHaveTextContent(/no passage P9/i)
  })

  it('background_refs_explain_why_they_are_not_citable', () => {
    render(<CitationChip refId="S1" onSelect={() => {}} />)
    expect(screen.getByRole('tooltip')).toHaveTextContent(/background summary/i)
  })

  it('known_ref_shows_document_version_and_offsets_and_selects_on_click', () => {
    const onSelect = vi.fn()
    render(<CitationChip refId="P1" passage={passage} onSelect={onSelect} />)
    const btn = screen.getByRole('button', { name: /P1.*security-policy\.md/i })
    expect(btn).toHaveAttribute('data-state', 'known')
    const tip = screen.getByRole('tooltip')
    expect(tip).toHaveTextContent('security-policy.md')
    expect(tip).toHaveTextContent('v3')
    expect(tip).toHaveTextContent('120-480')
    fireEvent.click(btn)
    expect(onSelect).toHaveBeenCalledWith('P1')
  })

  it('is_reachable_by_keyboard_through_the_tooltip_description', () => {
    render(<CitationChip refId="P1" passage={passage} onSelect={() => {}} />)
    const btn = screen.getByRole('button')
    expect(btn.tagName).toBe('BUTTON')
    const tip = screen.getByRole('tooltip')
    expect(btn.getAttribute('aria-describedby')).toBe(tip.id)
  })
})
