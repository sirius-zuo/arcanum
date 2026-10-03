import { afterEach, describe, expect, it, vi } from 'vitest'
import { render, screen, waitFor } from '@testing-library/react'
import { BootstrapProvider, useBootstrap } from './bootstrap'
import type { Bootstrap } from '../api/types'

const payload: Bootstrap = {
  api_key: 'key-123',
  collection: 'halcyon',
  orchestration_mode: 'ParallelFusion',
  generators: [{ name: 'local', protocol: 'openai_compatible', model: 'qwen2.5', is_default: true }],
  judge: 'local',
  anthropic_enabled: false,
  ollama_url: 'http://localhost:11434',
  mcp_port: 8081,
  features: { context: true, generate: true, verify: true, evidence: true, experiments: true, gc: false },
}

function Probe() {
  const { data, client, error } = useBootstrap()
  return (
    <div>
      <span data-testid="coll">{data?.collection ?? 'none'}</span>
      <span data-testid="err">{error?.message ?? ''}</span>
      <button onClick={() => void client.get('/demo/health').catch(() => undefined)}>go</button>
    </div>
  )
}

describe('bootstrap provider', () => {
  afterEach(() => {
    vi.unstubAllGlobals()
  })

  it('bootstrap_provider_exposes_client_with_key', async () => {
    const fetchMock = vi.fn().mockImplementation(async (url: string) =>
      url === '/demo/bootstrap'
        ? new Response(JSON.stringify(payload), { status: 200 })
        : new Response('{}', { status: 200 }),
    )
    vi.stubGlobal('fetch', fetchMock)
    render(
      <BootstrapProvider>
        <Probe />
      </BootstrapProvider>,
    )
    await waitFor(() => expect(screen.getByTestId('coll')).toHaveTextContent('halcyon'))
    screen.getByText('go').click()
    await waitFor(() => expect(fetchMock).toHaveBeenCalledTimes(2))
    const [url, init] = fetchMock.mock.calls[1] as [string, RequestInit]
    expect(url).toBe('/demo/health')
    expect(new Headers(init.headers).get('Authorization')).toBe('Bearer key-123')
    expect(fetchMock.mock.calls.filter((c) => c[0] === '/demo/bootstrap')).toHaveLength(1)
  })

  it('surfaces_a_bootstrap_failure_and_sends_nothing_else', async () => {
    const fetchMock = vi
      .fn()
      .mockResolvedValue(new Response(JSON.stringify({ error: 'down' }), { status: 500 }))
    vi.stubGlobal('fetch', fetchMock)
    render(
      <BootstrapProvider>
        <Probe />
      </BootstrapProvider>,
    )
    await waitFor(() => expect(screen.getByTestId('err')).toHaveTextContent('down'))
    screen.getByText('go').click()
    await new Promise((r) => setTimeout(r, 20))
    expect(fetchMock).toHaveBeenCalledTimes(1)
  })
})
