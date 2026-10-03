import { Compass } from 'lucide-react'
import { Chip } from './Chip'
import { HealthPill } from './HealthPill'
import { ThemeToggle } from './ThemeToggle'
import { useBootstrap } from '../state/bootstrap'

export function TopBar() {
  const { data } = useBootstrap()
  return (
    <div className="sticky top-0 z-10 flex h-16 items-center justify-between gap-4 border-b border-border bg-bg/80 px-8 backdrop-blur">
      <div className="flex items-center gap-2 text-sm">
        <span className="text-muted">Collection</span>
        {data ? <Chip tone="accent" mono>{data.collection}</Chip> : <span className="h-5 w-16 animate-pulse rounded-md bg-surface-2" aria-hidden="true" />}
      </div>
      <div className="flex items-center gap-2">
        <HealthPill />
        <button
          type="button"
          disabled
          title="The guided tour arrives in a later task"
          className="inline-flex h-9 items-center gap-2 rounded-lg bg-accent px-3.5 text-sm font-medium text-accent-fg opacity-60 transition"
        >
          <Compass className="h-4 w-4" aria-hidden="true" />
          Tour
        </button>
        <ThemeToggle />
      </div>
    </div>
  )
}
