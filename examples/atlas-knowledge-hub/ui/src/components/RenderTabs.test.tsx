import { describe, expect, it, vi } from 'vitest'
import { fireEvent, render, screen } from '@testing-library/react'
import { RenderTabs } from './RenderTabs'

describe('RenderTabs', () => {
  it('shows_the_exact_rendered_string_and_reports_the_chosen_format', () => {
    const onFormat = vi.fn()
    render(<RenderTabs format="xml" onFormat={onFormat} rendered={'<documents>\n  x\n</documents>'} />)
    expect(screen.getByRole('tabpanel').textContent).toBe('<documents>\n  x\n</documents>')
    fireEvent.click(screen.getByRole('tab', { name: 'markdown' }))
    expect(onFormat).toHaveBeenCalledWith('markdown')
  })
})
