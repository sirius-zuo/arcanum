import { afterEach, describe, expect, it, vi } from 'vitest'
import { fireEvent, render, screen, waitFor } from '@testing-library/react'
import { CodeBlock } from './CodeBlock'

afterEach(() => vi.restoreAllMocks())

describe('CodeBlock', () => {
  it('copies_text_to_clipboard', async () => {
    const writeText = vi.fn().mockResolvedValue(undefined)
    Object.defineProperty(navigator, 'clipboard', { value: { writeText }, configurable: true })
    render(<CodeBlock title="Context" code="curl x" />)
    fireEvent.click(screen.getByRole('button', { name: 'Copy Context' }))
    await waitFor(() => expect(writeText).toHaveBeenCalledWith('curl x'))
    expect(await screen.findByText('Copied')).toBeInTheDocument()
  })

  it('clipboard_failure_shows_manual_copy_fallback', async () => {
    const writeText = vi.fn().mockRejectedValue(new Error('denied'))
    Object.defineProperty(navigator, 'clipboard', { value: { writeText }, configurable: true })
    render(<CodeBlock title="Context" code="curl x" />)
    fireEvent.click(screen.getByRole('button', { name: 'Copy Context' }))
    const box = (await screen.findByLabelText('Context, select and copy manually')) as HTMLTextAreaElement
    expect(box.value).toBe('curl x')
    expect(screen.getByRole('status')).toHaveTextContent(/could not access the clipboard/i)
  })
})
