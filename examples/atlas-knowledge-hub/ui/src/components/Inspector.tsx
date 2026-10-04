import { createContext, useCallback, useContext, useEffect, useMemo, useRef, useState } from 'react'
import type { ReactNode } from 'react'
import { X } from 'lucide-react'

interface InspectorValue {
  open: (content: ReactNode, title: string) => void
  close: () => void
  /** What is shown right now, or null when the drawer is closed. */
  current: { content: ReactNode; title: string } | null
}

const Ctx = createContext<InspectorValue | null>(null)

export function InspectorProvider({ children }: { children: ReactNode }) {
  const [current, setCurrent] = useState<InspectorValue['current']>(null)
  const opener = useRef<HTMLElement | null>(null)

  const open = useCallback((content: ReactNode, title: string) => {
    // Only remember the opener on the first open so focus returns to the real trigger.
    if (!opener.current && document.activeElement instanceof HTMLElement) opener.current = document.activeElement
    setCurrent({ content, title })
  }, [])

  const close = useCallback(() => {
    setCurrent(null)
    const el = opener.current
    opener.current = null
    if (el && el.isConnected) el.focus()
  }, [])

  const value = useMemo(() => ({ open, close, current }), [open, close, current])
  return <Ctx.Provider value={value}>{children}</Ctx.Provider>
}

export function useInspector(): InspectorValue {
  const v = useContext(Ctx)
  if (!v) throw new Error('useInspector must be used inside InspectorProvider')
  return v
}

/**
 * The right-hand drawer. From 1280 px up it sits beside the page as a second pane;
 * below that it overlays the page with a scrim.
 */
export function Inspector() {
  const { current, close } = useInspector()
  const closeButton = useRef<HTMLButtonElement>(null)

  useEffect(() => {
    if (!current) return
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') close()
    }
    document.addEventListener('keydown', onKey)
    return () => document.removeEventListener('keydown', onKey)
  }, [current, close])

  const shown = current !== null
  useEffect(() => {
    if (shown) closeButton.current?.focus()
  }, [shown])

  if (!current) return null
  return (
    <>
      <div className="fixed inset-0 z-20 bg-text/20 xl:hidden" onClick={close} aria-hidden="true" />
      <aside
        aria-label={current.title}
        className="fixed inset-y-0 right-0 z-30 flex w-[min(420px,100vw)] flex-col border-l border-border bg-surface shadow-lift xl:sticky xl:top-0 xl:z-auto xl:h-screen xl:w-[400px] xl:shrink-0 xl:shadow-none"
      >
        <div className="flex h-16 shrink-0 items-center justify-between gap-2 border-b border-border px-5">
          <h2 className="truncate text-sm font-semibold">{current.title}</h2>
          <button
            ref={closeButton}
            type="button"
            onClick={close}
            aria-label="Close inspector"
            className="grid h-8 w-8 place-items-center rounded-lg text-muted transition hover:bg-surface-2 hover:text-text"
          >
            <X className="h-4 w-4" aria-hidden="true" />
          </button>
        </div>
        <div className="min-h-0 flex-1 overflow-y-auto p-5">{current.content}</div>
      </aside>
    </>
  )
}
