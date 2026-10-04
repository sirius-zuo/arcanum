import clsx from 'clsx'
import { Link } from 'react-router-dom'
import { useHealth } from '../api/library'

export function HealthPill() {
  const health = useHealth()

  let tone = 'bg-muted'
  let label = 'Checking'
  if (health.data) {
    const failing = health.data.checks.filter((c) => !c.ok).length
    tone = health.data.ready ? 'bg-v-supported' : 'bg-v-unsupported'
    label = health.data.ready ? 'Ready' : `${failing} ${failing === 1 ? 'issue' : 'issues'}`
  } else if (health.isError) {
    tone = 'bg-v-unsupported'
    label = 'Unreachable'
  }

  return (
    <Link
      to="/"
      title="Open the health checklist on the Overview"
      className="inline-flex h-9 items-center gap-2 rounded-lg border border-border bg-surface px-3 text-sm transition hover:shadow-soft"
    >
      <span className={clsx('h-2 w-2 rounded-full', tone, !health.data && !health.isError && 'animate-pulse')} aria-hidden="true" />
      <span className="font-medium">{label}</span>
    </Link>
  )
}
