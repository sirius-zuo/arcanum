import { Chip } from './Chip'
import { HealthPill } from './HealthPill'
import { ThemeToggle } from './ThemeToggle'
import { TourLauncher } from './TourLauncher'
import { useBootstrap } from '../state/bootstrap'

export function TopBar() {
  const { data } = useBootstrap()
  return (
    <header className="sticky top-0 z-10 flex h-16 items-center justify-between gap-4 border-b border-border bg-bg/80 px-8 backdrop-blur">
      <div className="flex items-center gap-2 text-sm">
        <span className="text-muted">Collection</span>
        {data ? <Chip tone="accent" mono>{data.collection}</Chip> : <span className="h-5 w-16 animate-pulse rounded-md bg-surface-2" aria-hidden="true" />}
      </div>
      <div className="flex items-center gap-2">
        <HealthPill />
        <TourLauncher />
        <ThemeToggle />
      </div>
    </header>
  )
}
