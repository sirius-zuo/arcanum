import { describe, expect, it, vi } from 'vitest'
import { render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { MemoryRouter } from 'react-router-dom'
import { CommandPalette } from './CommandPalette'

describe('command palette', () => {
  it('filters_and_runs_action', async () => {
    const user = userEvent.setup()
    const run = vi.fn()
    render(
      <MemoryRouter>
        <CommandPalette actions={[{ id: 'theme', label: 'Toggle theme', run }, { id: 'load', label: 'Load sample corpus', run: vi.fn() }]} />
      </MemoryRouter>,
    )
    await user.keyboard('{Control>}k{/Control}')
    expect(screen.getByRole('dialog', { name: /command palette/i })).toBeInTheDocument()
    await user.keyboard('theme')
    expect(screen.getByRole('option', { name: /toggle theme/i })).toBeInTheDocument()
    expect(screen.queryByRole('option', { name: /load sample corpus/i })).not.toBeInTheDocument()
    await user.keyboard('{Enter}')
    expect(run).toHaveBeenCalledTimes(1)
    expect(screen.queryByRole('dialog')).not.toBeInTheDocument()
  })

  it('disabled_action_is_listed_but_does_not_run_and_tab_keeps_focus', async () => {
    const user = userEvent.setup()
    const run = vi.fn()
    render(
      <MemoryRouter>
        <CommandPalette actions={[{ id: 'load', label: 'Load sample corpus', run, disabledReason: 'Not ready' }]} />
      </MemoryRouter>,
    )
    await user.keyboard('{Control>}k{/Control}')
    await user.keyboard('load')
    const option = screen.getByRole('option', { name: /load sample corpus/i })
    expect(option).toHaveAttribute('aria-disabled', 'true')
    expect(option).toHaveTextContent('Not ready')
    await user.keyboard('{Enter}')
    expect(run).not.toHaveBeenCalled()
    expect(screen.getByRole('dialog')).toBeInTheDocument()
    await user.tab()
    expect(screen.getByRole('combobox')).toHaveFocus()
  })
})
