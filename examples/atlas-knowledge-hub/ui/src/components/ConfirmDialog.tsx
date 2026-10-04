import { useEffect, useId, useRef } from 'react'
import type { KeyboardEvent, ReactNode } from 'react'

interface ConfirmDialogProps {
  title: string
  confirmLabel: string
  busy?: boolean
  onConfirm: () => void
  onCancel: () => void
  children: ReactNode
}

/** Modal alert dialog: focus starts on Cancel (the safe choice), Tab stays inside, Escape cancels, focus returns on close. */
export function ConfirmDialog({ title, confirmLabel, busy, onConfirm, onCancel, children }: ConfirmDialogProps) {
  const titleId = useId()
  const descId = useId()
  const root = useRef<HTMLDivElement>(null)
  const cancel = useRef<HTMLButtonElement>(null)

  useEffect(() => {
    const opener = document.activeElement as HTMLElement | null
    cancel.current?.focus()
    return () => opener?.focus?.()
  }, [])

  const onKeyDown = (e: KeyboardEvent<HTMLDivElement>) => {
    if (e.key === 'Escape') {
      e.stopPropagation()
      onCancel()
      return
    }
    if (e.key !== 'Tab') return
    const items = root.current?.querySelectorAll<HTMLElement>('button:not([disabled])')
    if (!items || items.length === 0) return
    const first = items[0]
    const last = items[items.length - 1]
    if (e.shiftKey && document.activeElement === first) {
      e.preventDefault()
      last.focus()
    } else if (!e.shiftKey && document.activeElement === last) {
      e.preventDefault()
      first.focus()
    }
  }

  return (
    <div className="fixed inset-0 z-50 grid place-items-center bg-black/40 p-4" onMouseDown={(e) => e.target === e.currentTarget && onCancel()}>
      <div
        ref={root}
        role="alertdialog"
        aria-modal="true"
        aria-labelledby={titleId}
        aria-describedby={descId}
        onKeyDown={onKeyDown}
        className="w-full max-w-md rounded-card border border-border bg-surface p-5 shadow-lift"
      >
        <h2 id={titleId} className="text-base font-semibold">
          {title}
        </h2>
        <div id={descId} className="mt-2 text-sm text-muted">
          {children}
        </div>
        <div className="mt-5 flex justify-end gap-2">
          <button
            ref={cancel}
            type="button"
            onClick={onCancel}
            className="h-9 rounded-lg border border-border px-3 text-sm font-medium transition hover:bg-surface-2"
          >
            Cancel
          </button>
          <button
            type="button"
            disabled={busy}
            onClick={onConfirm}
            className="h-9 rounded-lg bg-accent px-3 text-sm font-medium text-accent-fg transition hover:opacity-90 disabled:opacity-60"
          >
            {confirmLabel}
          </button>
        </div>
      </div>
    </div>
  )
}
