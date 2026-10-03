import clsx from 'clsx'

export function Skeleton({ className }: { className?: string }) {
  return (
    <div
      aria-hidden="true"
      className={clsx('skeleton relative overflow-hidden rounded-md bg-surface-2', className ?? 'h-4 w-full')}
    />
  )
}
