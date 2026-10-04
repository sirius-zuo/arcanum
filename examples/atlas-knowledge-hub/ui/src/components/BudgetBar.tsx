interface BudgetBarProps {
  used: number
  budget: number
  dropped: number
}

export function BudgetBar({ used, budget, dropped }: BudgetBarProps) {
  const pct = budget > 0 ? Math.min(100, Math.max(0, Math.round((used / budget) * 100))) : 0
  const fmt = (n: number) => n.toLocaleString('en-US')
  return (
    <div>
      <div className="mb-1.5 flex flex-wrap items-baseline justify-between gap-2 text-xs">
        <span className="font-mono">
          {fmt(used)} / {fmt(budget)} tokens
        </span>
        <span className="text-muted">
          {pct}% used, {dropped} dropped {dropped === 1 ? 'passage' : 'passages'}
        </span>
      </div>
      <div
        role="progressbar"
        aria-label="Token budget used"
        aria-valuemin={0}
        aria-valuemax={100}
        aria-valuenow={pct}
        className="h-2 overflow-hidden rounded-full bg-surface-2"
      >
        <div className="h-full rounded-full bg-accent transition-all" style={{ width: `${pct}%` }} />
      </div>
    </div>
  )
}
