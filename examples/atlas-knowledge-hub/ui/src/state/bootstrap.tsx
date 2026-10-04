import { createContext, useCallback, useContext, useEffect, useMemo, useRef, useState } from 'react'
import type { ReactNode } from 'react'
import { createClient } from '../api/client'
import type { Client } from '../api/client'
import { fetchBootstrap } from '../api/bootstrap'
import type { Bootstrap } from '../api/types'

interface BootstrapValue {
  data: Bootstrap | null
  error: Error | null
  client: Client
  retry: () => void
}

const Ctx = createContext<BootstrapValue | null>(null)

export function BootstrapProvider({ children }: { children: ReactNode }) {
  const [data, setData] = useState<Bootstrap | null>(null)
  const [error, setError] = useState<Error | null>(null)
  const [attempt, setAttempt] = useState(0)
  const keyRef = useRef<string | null>(null)

  useEffect(() => {
    let live = true
    setError(null)
    fetchBootstrap().then(
      (d) => {
        if (!live) return
        keyRef.current = d.api_key
        setData(d)
      },
      (e: unknown) => {
        if (live) setError(e instanceof Error ? e : new Error(String(e)))
      },
    )
    return () => {
      live = false
    }
  }, [attempt])

  const client = useMemo(() => createClient(() => keyRef.current), [])
  const retry = useCallback(() => setAttempt((n) => n + 1), [])
  const value = useMemo(() => ({ data, error, client, retry }), [data, error, client, retry])
  return <Ctx.Provider value={value}>{children}</Ctx.Provider>
}

export function useBootstrap(): BootstrapValue {
  const v = useContext(Ctx)
  if (!v) throw new Error('useBootstrap must be used inside BootstrapProvider')
  return v
}
