import clsx from 'clsx'
import type { ReactNode } from 'react'

export type ChipTone = 'neutral' | 'accent' | 'supported' | 'partial' | 'unsupported' | 'miscited' | 'uncited' | 'noclaim'

const tones: Record<ChipTone, string> = {
  neutral: 'bg-surface-2 text-text',
  accent: 'bg-accent/10 text-accent',
  supported: 'bg-v-supported/10 text-vink-supported',
  partial: 'bg-v-partial/10 text-vink-partial',
  unsupported: 'bg-v-unsupported/10 text-vink-unsupported',
  miscited: 'bg-v-miscited/10 text-vink-miscited',
  uncited: 'bg-v-uncited/10 text-vink-uncited',
  noclaim: 'bg-v-noclaim/10 text-vink-noclaim',
}

interface ChipProps {
  tone?: ChipTone
  mono?: boolean
  icon?: ReactNode
  className?: string
  children: ReactNode
}

/** Small pill. `mono` is for numbers, ids and offsets. Pair a tone with an icon or label so color is never the only signal. */
export function Chip({ tone = 'neutral', mono, icon, className, children }: ChipProps) {
  return (
    <span
      className={clsx(
        'inline-flex items-center gap-1 rounded-md px-1.5 py-0.5 text-xs font-medium leading-5',
        mono && 'font-mono text-[11px]',
        tones[tone],
        className,
      )}
    >
      {icon}
      {children}
    </span>
  )
}
