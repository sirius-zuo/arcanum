import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { act, render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { ThemeProvider, useTheme } from './theme'

function Probe() {
  const { theme, toggle } = useTheme()
  return (
    <button onClick={toggle} data-testid="probe">
      {theme}
    </button>
  )
}

function mockMedia(dark: boolean) {
  vi.stubGlobal(
    'matchMedia',
    vi.fn().mockImplementation((q: string) => ({
      matches: dark && q.includes('dark'),
      media: q,
      addEventListener: vi.fn(),
      removeEventListener: vi.fn(),
    })),
  )
}

describe('theme', () => {
  beforeEach(() => {
    localStorage.clear()
    document.documentElement.classList.remove('dark')
  })
  afterEach(() => {
    vi.unstubAllGlobals()
  })

  it('theme_persists_and_toggles', async () => {
    mockMedia(false)
    render(
      <ThemeProvider>
        <Probe />
      </ThemeProvider>,
    )
    expect(screen.getByTestId('probe')).toHaveTextContent('light')
    expect(document.documentElement).not.toHaveClass('dark')
    await act(async () => {
      await userEvent.click(screen.getByTestId('probe'))
    })
    expect(screen.getByTestId('probe')).toHaveTextContent('dark')
    expect(document.documentElement).toHaveClass('dark')
    expect(localStorage.getItem('atlas.theme')).toBe('dark')
  })

  it('stored_value_wins_over_media_query', () => {
    mockMedia(true)
    localStorage.setItem('atlas.theme', 'light')
    render(
      <ThemeProvider>
        <Probe />
      </ThemeProvider>,
    )
    expect(screen.getByTestId('probe')).toHaveTextContent('light')
    expect(document.documentElement).not.toHaveClass('dark')
  })

  it('falls_back_to_prefers_color_scheme', () => {
    mockMedia(true)
    render(
      <ThemeProvider>
        <Probe />
      </ThemeProvider>,
    )
    expect(screen.getByTestId('probe')).toHaveTextContent('dark')
    expect(document.documentElement).toHaveClass('dark')
  })
})
