import { createContext, useCallback, useContext, useEffect, useMemo, useReducer, useRef, useState } from 'react'
import type { ReactNode } from 'react'
import { useLibrary, useSamples } from '../api/library'
import type { TourStep } from '../api/types'
import { libraryEvents, parseTour, staleLibraryEvents, reduceTour, serializeTour } from '../lib/tourState'
import type { TourEvent, TourState } from '../lib/tourState'
import { safeGet, safeSet } from '../lib/storage'
import { useCommandActions } from './commandActions'

const KEY = 'atlas.tour'

export interface TourValue {
  state: TourState
  steps: TourStep[]
  /** Increments on every start so the card can take focus for a user-initiated start only. */
  startCount: number
  /** Increments on every reset so library-derived steps can be re-derived from unchanged data. */
  resets: number
  start: () => void
  next: () => void
  prev: () => void
  goto: (index: number) => void
  dismiss: () => void
  reset: () => void
  signal: (event: TourEvent) => void
  revoke: (events: TourEvent[]) => void
}

const Ctx = createContext<TourValue | null>(null)

/** State holder with the steps supplied directly; TourProvider feeds it from the API. */
export function TourStore({ steps, children }: { steps: TourStep[]; children: ReactNode }) {
  const [state, dispatch] = useReducer(reduceTour, undefined, () => parseTour(safeGet(KEY)))
  const [startCount, setStartCount] = useState(0)
  const [resets, setResets] = useState(0)

  useEffect(() => {
    safeSet(KEY, serializeTour(state))
  }, [state])

  // Read at call time so a signal from a long-lived callback never sees a stale step list.
  const stepsRef = useRef(steps)
  stepsRef.current = steps
  const count = steps.length
  const start = useCallback(() => {
    dispatch({ type: 'start', steps })
    setStartCount((n) => n + 1)
  }, [steps])
  const next = useCallback(() => dispatch({ type: 'next', count }), [count])
  const prev = useCallback(() => dispatch({ type: 'prev', count }), [count])
  const goto = useCallback((index: number) => dispatch({ type: 'goto', index, count }), [count])
  const dismiss = useCallback(() => dispatch({ type: 'dismiss' }), [])
  const reset = useCallback(() => {
    dispatch({ type: 'reset' })
    setResets((n) => n + 1)
  }, [])
  const revoke = useCallback(
    (events: TourEvent[]) => dispatch({ type: 'revoke', events, steps: stepsRef.current }),
    [steps],
  )
  const signal = useCallback(
    (event: TourEvent) => dispatch({ type: 'complete', event, steps: stepsRef.current }),
    // steps is a dependency on purpose: a new identity re-runs LibrarySync once the step list arrives.
    [steps],
  )

  const value = useMemo(
    () => ({ state, steps, startCount, resets, start, next, prev, goto, dismiss, reset, signal, revoke }),
    [state, steps, startCount, resets, start, next, prev, goto, dismiss, reset, signal, revoke],
  )
  return <Ctx.Provider value={value}>{children}</Ctx.Provider>
}

/** Completes corpus_loaded and update_applied from library data, never from ingestion events. */
function LibrarySync() {
  const library = useLibrary()
  const { signal, revoke, resets } = useTour()
  const data = library.data
  useEffect(() => {
    // Data resets on every start: a completion the library no longer backs is taken back.
    revoke(staleLibraryEvents(data))
    for (const e of libraryEvents(data)) signal(e)
  }, [data, signal, revoke, resets])
  return null
}

function PaletteBridge() {
  const { register } = useCommandActions()
  const { start } = useTour()
  useEffect(() => register('tour.start', start), [register, start])
  return null
}

const NO_STEPS: TourStep[] = []

export function TourProvider({ children }: { children: ReactNode }) {
  const samples = useSamples()
  const steps = samples.data?.tour ?? NO_STEPS
  return (
    <TourStore steps={steps}>
      <LibrarySync />
      <PaletteBridge />
      {children}
    </TourStore>
  )
}

export function useTour(): TourValue {
  const v = useContext(Ctx)
  if (!v) throw new Error('useTour must be used inside TourProvider')
  return v
}

const NOOP = (): void => {}

/** For pages: signals the tour, or does nothing when no tour is mounted (tests, embedding). */
export function useTourSignal(): (event: TourEvent) => void {
  return useContext(Ctx)?.signal ?? NOOP
}
