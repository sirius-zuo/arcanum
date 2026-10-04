import clsx from 'clsx'
import type { HTMLAttributes } from 'react'

interface CardProps extends HTMLAttributes<HTMLDivElement> {
  interactive?: boolean
}

export function Card({ interactive, className, ...rest }: CardProps) {
  return (
    <div
      className={clsx(
        'rounded-card border border-border bg-surface shadow-soft transition',
        interactive && 'hover:-translate-y-px hover:border-accent/40 hover:shadow-lift',
        className,
      )}
      {...rest}
    />
  )
}
