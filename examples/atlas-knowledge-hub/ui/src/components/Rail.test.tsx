import { describe, expect, it } from 'vitest'
import { render, screen } from '@testing-library/react'
import { MemoryRouter } from 'react-router-dom'
import { Rail } from './Rail'

describe('Rail', () => {
  it('rail_lists_all_routes_in_order', () => {
    render(
      <MemoryRouter>
        <Rail collapsed={false} onToggle={() => undefined} />
      </MemoryRouter>,
    )
    const links = screen.getAllByRole('link').filter((a) => a.getAttribute('data-rail-item') !== null)
    expect(links.map((a) => a.textContent?.trim())).toEqual([
      'Overview',
      'Library',
      'Search',
      'Context',
      'Ask',
      'Verify',
      'Evidence',
      'Graph',
      'Lab',
      'Admin',
      'Connect',
    ])
    expect(links.map((a) => a.getAttribute('href'))).toEqual([
      '/', '/library', '/search', '/context', '/ask', '/verify', '/evidence', '/graph', '/lab', '/admin', '/connect',
    ])
  })

  it('collapsed_rail_keeps_accessible_names', () => {
    render(
      <MemoryRouter>
        <Rail collapsed onToggle={() => undefined} />
      </MemoryRouter>,
    )
    expect(screen.getByRole('link', { name: 'Overview' })).toBeInTheDocument()
  })
})
