import type { ReactNode } from 'react'
import type { LucideIcon } from 'lucide-react'

interface EmptyStateProps {
  icon: LucideIcon
  title: string
  description: string
  action?: ReactNode
}

export function EmptyState({ icon: Icon, title, description, action }: EmptyStateProps) {
  return (
    <div className="relative flex flex-col items-center overflow-hidden rounded-card border border-dashed border-border px-6 py-16 text-center">
      <div className="atlas-dots pointer-events-none absolute inset-0" aria-hidden="true" />
      <div className="relative mb-4 grid h-11 w-11 place-items-center rounded-xl border border-border bg-surface shadow-soft">
        <Icon className="h-5 w-5 text-accent" aria-hidden="true" />
      </div>
      <h2 className="relative text-base font-semibold">{title}</h2>
      <p className="relative mt-1 max-w-md text-sm text-muted">{description}</p>
      {action && <div className="relative mt-5">{action}</div>}
    </div>
  )
}
