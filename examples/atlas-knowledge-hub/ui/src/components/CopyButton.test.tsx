import { describe, expect, it, vi } from 'vitest'
import { fireEvent, render, screen, waitFor } from '@testing-library/react'
import { CopyButton } from './CopyButton'

describe('CopyButton', () => {
  it('clipboard_failure_does_not_throw', async () => {
    const writeText = vi.fn().mockRejectedValue(new Error('denied'))
    Object.defineProperty(navigator, 'clipboard', { value: { writeText }, configurable: true })
    render(<CopyButton text="abc" label="Copy it" showLabel />)
    fireEvent.click(screen.getByRole('button', { name: 'Copy it' }))
    await waitFor(() => expect(writeText).toHaveBeenCalledWith('abc'))
    expect(screen.queryByText('Copied')).not.toBeInTheDocument()
  })
})
