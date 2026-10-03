import { describe, expect, it } from 'vitest'
import { render, screen } from '@testing-library/react'
import { HealthChecklist } from './HealthChecklist'
import { LoadCorpusButton } from './LoadCorpusButton'
import type { DemoHealth } from '../api/types'

const health: DemoHealth = {
  ready: false,
  checks: [
    { id: 'ollama', label: 'Ollama is reachable', ok: true, detail: 'ok', fix: null },
    { id: 'chat_model', label: 'Chat model qwen2.5 is pulled', ok: false, detail: 'missing', fix: 'ollama pull qwen2.5' },
  ],
}

describe('health checklist', () => {
  it('failing_checks_show_fix_and_block_load', () => {
    render(
      <>
        <HealthChecklist checks={health.checks} />
        <LoadCorpusButton health={health} loading={false} trackedCount={0} onLoad={() => {}} />
      </>,
    )
    expect(screen.getByText('ollama pull qwen2.5')).toBeInTheDocument()
    expect(screen.getByRole('button', { name: /copy/i })).toBeInTheDocument()
    const load = screen.getByRole('button', { name: /load sample corpus/i })
    expect(load).toBeDisabled()
    expect(load).toHaveAttribute('title', 'Chat model qwen2.5 is pulled')
  })
})
