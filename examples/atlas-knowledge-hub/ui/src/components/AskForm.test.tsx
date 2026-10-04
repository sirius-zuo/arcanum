import { beforeEach, describe, expect, it, vi } from 'vitest'
import { fireEvent, render, screen } from '@testing-library/react'
import { MemoryRouter } from 'react-router-dom'
import { AskForm } from './AskForm'
import { ASK_PREFILL_KEY } from '../lib/askPrefill'
import type { Bootstrap } from '../api/types'

const boot = (over: Partial<Bootstrap> = {}): Bootstrap => ({
  api_key: 'k',
  collection: 'halcyon',
  orchestration_mode: 'parallel',
  generators: [
    { name: 'local', protocol: 'openai_compatible', model: 'qwen2.5', is_default: true },
    { name: 'smart', protocol: 'anthropic', model: 'claude-sonnet-5-5', is_default: false },
  ],
  judge: 'local',
  anthropic_enabled: true,
  ollama_url: 'http://localhost:11434',
  mcp_port: 8081,
  features: { context: true, generate: true, verify: true, evidence: true, experiments: true, gc: true },
  ...over,
})

function mount(b: Bootstrap, props: Partial<Parameters<typeof AskForm>[0]> = {}) {
  const onSubmit = vi.fn()
  const onStop = vi.fn()
  const view = render(
    <MemoryRouter>
      <AskForm boot={b} busy={false} onSubmit={onSubmit} onStop={onStop} {...props} />
    </MemoryRouter>,
  )
  return { onSubmit, onStop, ...view }
}

describe('AskForm', () => {
  beforeEach(() => sessionStorage.clear())

  it('prefill_is_consumed_once', () => {
    sessionStorage.setItem(ASK_PREFILL_KEY, 'What is the remote policy?')
    const first = mount(boot())
    expect(screen.getByLabelText('Question')).toHaveValue('What is the remote policy?')
    expect(sessionStorage.getItem(ASK_PREFILL_KEY)).toBeNull()
    first.unmount()
    mount(boot())
    expect(screen.getByLabelText('Question')).toHaveValue('')
  })

  it('submits_question_mode_and_verify_and_omits_the_default_generator', () => {
    const { onSubmit } = mount(boot())
    fireEvent.change(screen.getByLabelText('Question'), { target: { value: ' hello ' } })
    fireEvent.click(screen.getByRole('checkbox', { name: /verify answer/i }))
    fireEvent.click(screen.getByRole('button', { name: /^ask$/i }))
    expect(onSubmit).toHaveBeenCalledWith({ question: 'hello', mode: 'answer', generator: undefined, verify: true })
  })

  it('sends_a_non_default_generator_and_summarize_mode', () => {
    const { onSubmit } = mount(boot())
    fireEvent.change(screen.getByLabelText('Question'), { target: { value: 'the roadmap' } })
    fireEvent.change(screen.getByLabelText('Generator'), { target: { value: 'smart' } })
    fireEvent.click(screen.getByRole('radio', { name: /summarize/i }))
    fireEvent.click(screen.getByRole('button', { name: /summarize/i }))
    expect(onSubmit).toHaveBeenCalledWith({ question: 'the roadmap', mode: 'summarize', generator: 'smart', verify: false })
  })

  it('disables_verify_with_an_explanation_when_no_judge_is_configured', () => {
    mount(boot({ judge: null }))
    expect(screen.getByRole('checkbox', { name: /verify answer/i })).toBeDisabled()
    expect(screen.getByText(/no judge is configured/i)).toBeInTheDocument()
  })

  it('shows_the_judge_read_only_and_points_to_the_verify_lab', () => {
    mount(boot())
    expect(screen.getByText(/judge: local/i)).toBeInTheDocument()
    expect(screen.getByRole('link', { name: /verify lab/i })).toBeInTheDocument()
  })

  it('example_chips_fill_the_question_including_the_multi_hop_one', () => {
    mount(boot())
    fireEvent.click(screen.getByRole('button', { name: 'Who is the on-call lead for the team that owns the navigation stack?' }))
    expect(screen.getByLabelText('Question')).toHaveValue('Who is the on-call lead for the team that owns the navigation stack?')
  })

  it('shows_stop_while_busy_and_calls_it', () => {
    const { onStop } = mount(boot(), { busy: true })
    fireEvent.click(screen.getByRole('button', { name: /^stop$/i }))
    expect(onStop).toHaveBeenCalled()
  })

  it('shows_the_unavailable_reason_and_disables_verify', () => {
    mount(boot(), { verifyDisabledReason: 'verification requires a configured judge' })
    expect(screen.getByRole('checkbox', { name: /verify answer/i })).toBeDisabled()
    expect(screen.getByText('verification requires a configured judge')).toBeInTheDocument()
  })

  it('restores_the_question_of_a_failed_turn_only_into_an_empty_box', () => {
    const { rerender } = mount(boot())
    const ui = (restore: { key: number; text: string } | null) => (
      <MemoryRouter>
        <AskForm boot={boot()} busy={false} onSubmit={vi.fn()} onStop={vi.fn()} restore={restore} />
      </MemoryRouter>
    )
    fireEvent.change(screen.getByLabelText('Question'), { target: { value: 'why?' } })
    fireEvent.click(screen.getByRole('button', { name: /^ask$/i }))
    expect(screen.getByLabelText('Question')).toHaveValue('')
    rerender(ui({ key: 1, text: 'why?' }))
    expect(screen.getByLabelText('Question')).toHaveValue('why?')
    fireEvent.change(screen.getByLabelText('Question'), { target: { value: 'typed since' } })
    rerender(ui({ key: 2, text: 'other' }))
    expect(screen.getByLabelText('Question')).toHaveValue('typed since')
  })

  it('hides_example_chips_when_asked_to', () => {
    mount(boot(), { showExamples: false })
    expect(screen.queryByLabelText('Example questions')).toBeNull()
  })
})
