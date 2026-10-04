import type { ReactNode } from 'react'

interface PageHeaderProps {
  eyebrow?: string
  title: string
  description?: string
  actions?: ReactNode
}

export function PageHeader({ eyebrow, title, description, actions }: PageHeaderProps) {
  return (
    <header className="mb-8 flex flex-wrap items-end justify-between gap-4 animate-rise">
      <div className="min-w-0">
        {eyebrow && (
          <p className="mb-2 flex items-center gap-2 font-mono text-[11px] uppercase tracking-[0.14em] text-accent">
            <span className="h-px w-6 bg-accent/60" aria-hidden="true" />
            {eyebrow}
          </p>
        )}
        <h1 className="text-[28px] font-semibold leading-tight tracking-tight">{title}</h1>
        {description && <p className="mt-2 max-w-2xl text-[15px] leading-relaxed text-muted">{description}</p>}
      </div>
      {actions && <div className="flex items-center gap-2">{actions}</div>}
    </header>
  )
}
