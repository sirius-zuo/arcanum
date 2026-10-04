import type { ReactNode } from 'react'

interface StatProps {
  label: string
  value: ReactNode
  hint?: string
}

export function Stat({ label, value, hint }: StatProps) {
  return (
    <div className="flex flex-col gap-1">
      <span className="font-mono text-[11px] uppercase tracking-[0.08em] text-muted">{label}</span>
      <span className="font-mono text-2xl font-medium tabular-nums">{value}</span>
      {hint && <span className="text-xs text-muted">{hint}</span>}
    </div>
  )
}
