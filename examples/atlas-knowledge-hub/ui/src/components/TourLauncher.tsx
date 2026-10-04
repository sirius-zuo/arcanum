import { Compass } from 'lucide-react'
import { useTour } from '../state/tour'

export function TourLauncher() {
  const { state, steps, start, dismiss } = useTour()
  const total = steps.length
  const done = steps.filter((s) => state.completed[s.id]).length
  const label = state.active ? `Tour ${done}/${total}` : done === 0 ? 'Tour' : done >= total && total > 0 ? 'Replay tour' : `Resume tour ${done}/${total}`
  return (
    <button
      id="tour-launcher"
      type="button"
      onClick={state.active ? dismiss : start}
      disabled={total === 0}
      aria-pressed={state.active}
      title={state.active ? 'Hide the guided tour' : 'Start or resume the guided tour'}
      className="inline-flex h-9 items-center gap-2 rounded-lg bg-accent px-3.5 text-sm font-medium text-accent-fg transition hover:opacity-90 disabled:opacity-60"
    >
      <Compass className="h-4 w-4" aria-hidden="true" />
      {label}
    </button>
  )
}
