import { describe, expect, it } from 'vitest'
import { render, screen } from '@testing-library/react'
import { BudgetBar } from './BudgetBar'

describe('BudgetBar', () => {
  it('clamps_at_100_percent_and_shows_dropped', () => {
    render(<BudgetBar used={1500} budget={1000} dropped={3} />)
    expect(screen.getByRole('progressbar')).toHaveAttribute('aria-valuenow', '100')
    expect(screen.getByText(/3 dropped/)).toBeInTheDocument()
    expect(screen.getByText(/1,500/)).toBeInTheDocument()
  })

  it('zero_budget_does_not_divide_by_zero', () => {
    render(<BudgetBar used={10} budget={0} dropped={0} />)
    expect(screen.getByRole('progressbar')).toHaveAttribute('aria-valuenow', '0')
  })
})
