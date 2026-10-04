import { createContext, useCallback, useContext, useMemo, useRef } from 'react'
import type { ReactNode } from 'react'

type Handler = () => void

interface CommandActionsValue {
  /** Register a handler for an action id; returns the unregister function. */
  register: (id: string, handler: Handler) => () => void
  /** Run the handler for an id; a no-op until something registers one. */
  run: (id: string) => void
}

const Ctx = createContext<CommandActionsValue | null>(null)

/** A registry so later features (the tour) can supply real handlers for palette actions. */
export function CommandActionsProvider({ children }: { children: ReactNode }) {
  const handlers = useRef(new Map<string, Handler>())

  const register = useCallback((id: string, handler: Handler) => {
    handlers.current.set(id, handler)
    return () => {
      if (handlers.current.get(id) === handler) handlers.current.delete(id)
    }
  }, [])
  const run = useCallback((id: string) => handlers.current.get(id)?.(), [])

  const value = useMemo(() => ({ register, run }), [register, run])
  return <Ctx.Provider value={value}>{children}</Ctx.Provider>
}

export function useCommandActions(): CommandActionsValue {
  const v = useContext(Ctx)
  if (!v) throw new Error('useCommandActions must be used inside CommandActionsProvider')
  return v
}
