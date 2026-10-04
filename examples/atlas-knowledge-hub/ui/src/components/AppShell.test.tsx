import { afterEach, describe, expect, it, vi } from 'vitest'
import { render, screen } from '@testing-library/react'
import { createMemoryRouter, RouterProvider } from 'react-router-dom'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { AppShell } from './AppShell'
import { BootstrapProvider } from '../state/bootstrap'
import { ThemeProvider } from '../state/theme'

describe('AppShell', () => {
  afterEach(() => {
    vi.unstubAllGlobals()
  })

  it('shell_exposes_banner_navigation_and_main_landmarks', () => {
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue(new Response('{}', { status: 500 })))
    const router = createMemoryRouter([{ path: '/', element: <AppShell />, children: [{ index: true, element: <p>page</p> }] }])
    render(
      <ThemeProvider>
        <BootstrapProvider>
          <QueryClientProvider client={new QueryClient()}>
            <RouterProvider router={router} />
          </QueryClientProvider>
        </BootstrapProvider>
      </ThemeProvider>,
    )
    expect(screen.getByRole('banner')).toBeInTheDocument()
    expect(screen.getByRole('navigation', { name: 'Primary' })).toBeInTheDocument()
    expect(screen.getByRole('main')).toBeInTheDocument()
  })
})
