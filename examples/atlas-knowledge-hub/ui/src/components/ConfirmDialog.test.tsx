import { describe, expect, it, vi } from 'vitest'
import { fireEvent, render, screen } from '@testing-library/react'
import { useState } from 'react'
import { ConfirmDialog } from './ConfirmDialog'

function Harness({ onConfirm }: { onConfirm: () => void }) {
  const [open, setOpen] = useState(false)
  return (
    <>
      <button onClick={() => setOpen(true)}>Open</button>
      {open && (
        <ConfirmDialog title="Rotate?" confirmLabel="Rotate keys" onConfirm={onConfirm} onCancel={() => setOpen(false)}>
          Sure?
        </ConfirmDialog>
      )}
    </>
  )
}

describe('ConfirmDialog', () => {
  it('focuses_cancel_closes_on_escape_and_restores_focus', () => {
    render(<Harness onConfirm={() => {}} />)
    const opener = screen.getByRole('button', { name: 'Open' })
    opener.focus()
    fireEvent.click(opener)
    const dialog = screen.getByRole('alertdialog', { name: 'Rotate?' })
    expect(screen.getByRole('button', { name: 'Cancel' })).toHaveFocus()
    fireEvent.keyDown(dialog, { key: 'Escape' })
    expect(screen.queryByRole('alertdialog')).not.toBeInTheDocument()
    expect(opener).toHaveFocus()
  })

  it('confirm_calls_back_and_tab_wraps', () => {
    const onConfirm = vi.fn()
    render(<Harness onConfirm={onConfirm} />)
    fireEvent.click(screen.getByRole('button', { name: 'Open' }))
    const confirm = screen.getByRole('button', { name: 'Rotate keys' })
    confirm.focus()
    fireEvent.keyDown(confirm, { key: 'Tab' })
    expect(screen.getByRole('button', { name: 'Cancel' })).toHaveFocus()
    fireEvent.click(confirm)
    expect(onConfirm).toHaveBeenCalled()
  })
})
