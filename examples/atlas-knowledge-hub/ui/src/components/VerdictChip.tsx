import { AlertTriangle, ArrowLeftRight, Check, Link2, Minus, X } from 'lucide-react'
import type { LucideIcon } from 'lucide-react'
import type { Verdict } from '../api/types'
import { verdictMeta } from '../lib/verdicts'
import type { VerdictIcon } from '../lib/verdicts'
import { Chip } from './Chip'
import type { ChipTone } from './Chip'

const ICONS: Record<VerdictIcon, LucideIcon> = {
  check: Check,
  alert: AlertTriangle,
  x: X,
  swap: ArrowLeftRight,
  link: Link2,
  minus: Minus,
}

export function VerdictIcon({ verdict, className = 'h-3 w-3' }: { verdict: Verdict; className?: string }) {
  const Icon = ICONS[verdictMeta(verdict).icon]
  return <Icon className={className} aria-hidden="true" />
}

interface VerdictChipProps {
  verdict: Verdict
  /** A count shown after the label (the overall counts row). */
  count?: number
  className?: string
}

/** Icon plus label plus tone: a verdict is never conveyed by color alone. */
export function VerdictChip({ verdict, count, className }: VerdictChipProps) {
  const meta = verdictMeta(verdict)
  return (
    <Chip tone={meta.tone as ChipTone} icon={<VerdictIcon verdict={verdict} />} className={className}>
      {meta.label}
      {count !== undefined && <span className="font-mono">{count}</span>}
    </Chip>
  )
}
