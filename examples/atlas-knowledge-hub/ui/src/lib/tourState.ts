import type { Library } from '../api/types'

export type TourEvent =
  | 'corpus_loaded'
  | 'searched'
  | 'context_built'
  | 'asked'
  | 'verified'
  | 'flawed_checked'
  | 'update_applied'
  | 'evidence_opened'
  | 'evaluated'

export interface TourState {
  active: boolean
  index: number
  completed: Record<string, boolean>
}

/** The only fields of a tour step the reducer needs. */
export interface StepRef {
  id: string
  completes_when: string
}

export type TourAction =
  | { type: 'start'; steps: StepRef[] }
  | { type: 'next'; count: number }
  | { type: 'prev'; count: number }
  | { type: 'goto'; index: number; count: number }
  | { type: 'dismiss' }
  | { type: 'complete'; event: TourEvent; steps: StepRef[] }
  | { type: 'reset' }

export const INITIAL_TOUR: TourState = { active: false, index: 0, completed: {} }

const clamp = (n: number, count: number) => Math.max(0, Math.min(n, Math.max(0, count - 1)))

/** Index of the first step not yet completed, or -1 when every step is done. */
export function nextIncomplete(state: TourState, steps: StepRef[]): number {
  return steps.findIndex((s) => !state.completed[s.id])
}

export function reduceTour(state: TourState, action: TourAction): TourState {
  switch (action.type) {
    case 'start': {
      const i = nextIncomplete(state, action.steps)
      return { ...state, active: true, index: i < 0 ? 0 : i }
    }
    case 'next':
      return { ...state, index: clamp(state.index + 1, action.count) }
    case 'prev':
      return { ...state, index: clamp(state.index - 1, action.count) }
    case 'goto':
      return { ...state, index: clamp(action.index, action.count) }
    case 'dismiss':
      return { ...state, active: false }
    case 'complete': {
      const hit = action.steps.filter((s) => s.completes_when === action.event && !state.completed[s.id])
      if (hit.length === 0) return state
      const completed = { ...state.completed }
      for (const s of hit) completed[s.id] = true
      return { ...state, completed }
    }
    case 'reset':
      return INITIAL_TOUR
  }
}

export function serializeTour(state: TourState): string {
  return JSON.stringify(state)
}

/** Tolerates null, garbage and wrong shapes by returning the initial state. */
export function parseTour(raw: string | null): TourState {
  if (!raw) return INITIAL_TOUR
  try {
    const v: unknown = JSON.parse(raw)
    if (typeof v !== 'object' || v === null) return INITIAL_TOUR
    const o = v as Record<string, unknown>
    if (typeof o.active !== 'boolean' || typeof o.index !== 'number' || !Number.isInteger(o.index) || o.index < 0) return INITIAL_TOUR
    if (typeof o.completed !== 'object' || o.completed === null || Array.isArray(o.completed)) return INITIAL_TOUR
    const completed: Record<string, boolean> = {}
    for (const [k, val] of Object.entries(o.completed)) {
      if (val === true) completed[k] = true
    }
    return { active: o.active, index: o.index, completed }
  } catch {
    return INITIAL_TOUR
  }
}

export const CORPUS_MIN_DOCUMENTS = 10
const POLICY = 'security-policy.md'

/** Events implied by library data, so they hold after a reload or a corpus loaded before the tour. */
export function libraryEvents(library: Library | undefined): TourEvent[] {
  if (!library) return []
  const events: TourEvent[] = []
  if (library.documents.length >= CORPUS_MIN_DOCUMENTS) events.push('corpus_loaded')
  const policy = library.documents.find((d) => d.source_uri === POLICY)
  if (policy && policy.versions.length >= 2) events.push('update_applied')
  return events
}
