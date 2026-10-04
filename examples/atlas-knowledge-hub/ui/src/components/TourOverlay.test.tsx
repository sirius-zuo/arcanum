import { describe, expect, it } from 'vitest'
import { act, fireEvent, render, screen, within } from '@testing-library/react'
import { MemoryRouter } from 'react-router-dom'
import { TourOverlay } from './TourOverlay'
import { TourStore, useTour } from '../state/tour'
import type { TourStep } from '../api/types'

const steps: TourStep[] = [
  { id: 'search', title: 'Search it', why: 'because', action: 'type', route: '/search', completes_when: 'searched', payoff: 'Hits appear.' },
  { id: 'ask', title: 'Ask it', why: 'because', action: 'press', route: '/ask', completes_when: 'asked', payoff: 'An answer appears.' },
]

function Harness() {
  const { start, signal } = useTour()
  return (
    <>
      <button onClick={start}>go</button>
      <button onClick={() => signal('searched')}>fire</button>
    </>
  )
}

function setup() {
  localStorage.clear()
  render(
    <MemoryRouter>
      <TourStore steps={steps}>
        <Harness />
        <TourOverlay />
      </TourStore>
    </MemoryRouter>,
  )
  fireEvent.click(screen.getByText('go'))
}

describe('TourOverlay', () => {
  it('shows_payoff_when_step_completes_and_supports_keyboard', () => {
    setup()
    const card = screen.getByRole('complementary', { name: 'Guided tour' })
    expect(screen.getByText('Search it')).toBeInTheDocument()
    expect(screen.getByRole('link', { name: /take me there/i })).toHaveAttribute('href', '/search')
    expect(screen.queryByText('Hits appear.')).toBeNull()

    act(() => screen.getByText('fire').click())
    expect(screen.getByText(/Hits appear\./)).toBeInTheDocument()
    expect(within(card).getByRole('status')).toHaveTextContent('Hits appear.')

    fireEvent.keyDown(card, { key: 'ArrowRight' })
    expect(screen.getByText('Ask it')).toBeInTheDocument()
    fireEvent.keyDown(card, { key: 'ArrowLeft' })
    expect(screen.getByText('Search it')).toBeInTheDocument()

    fireEvent.keyDown(card, { key: 'Escape' })
    expect(screen.queryByRole('complementary', { name: 'Guided tour' })).toBeNull()
  })

  it('announces_completion_of_a_step_that_is_not_viewed_and_returns_focus_on_dismiss', () => {
    setup()
    fireEvent.keyDown(screen.getByRole('complementary', { name: 'Guided tour' }), { key: 'ArrowRight' })
    act(() => screen.getByText('fire').click())
    expect(screen.getAllByRole('status').some((n) => /Step 1 completed/.test(n.textContent ?? ''))).toBe(true)
    expect(screen.getByText('Ask it')).toBeInTheDocument()
  })

  it('reset_tour_clears_progress_and_closes_the_card', () => {
    setup()
    act(() => screen.getByText('fire').click())
    expect(JSON.parse(localStorage.getItem('atlas.tour') ?? '{}').completed).toEqual({ search: true })
    fireEvent.click(screen.getByRole('button', { name: 'Reset tour' }))
    expect(screen.queryByRole('complementary', { name: 'Guided tour' })).toBeNull()
    expect(JSON.parse(localStorage.getItem('atlas.tour') ?? '{}').completed).toEqual({})
  })

  it('persists_progress_and_survives_corrupt_storage', () => {
    setup()
    act(() => screen.getByText('fire').click())
    expect(JSON.parse(localStorage.getItem('atlas.tour') ?? '{}').completed).toEqual({ search: true })
  })

  it('does_not_take_focus_until_started_and_tolerates_corrupt_storage', () => {
    localStorage.setItem('atlas.tour', '{not json')
    render(
      <MemoryRouter>
        <TourStore steps={steps}>
          <TourOverlay />
        </TourStore>
      </MemoryRouter>,
    )
    expect(screen.queryByRole('complementary')).toBeNull()
  })
})
