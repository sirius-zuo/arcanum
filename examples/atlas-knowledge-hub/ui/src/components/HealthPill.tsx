import { useQuery } from '@tanstack/react-query'
import clsx from 'clsx'
import { Link } from 'react-router-dom'
import type { DemoHealth } from '../api/types'
import { useBootstrap } from '../state/bootstrap'

export function HealthPill() {
  const { data, client } = useBootstrap()
  const health = useQuery({
    queryKey: ['demo', 'health'],
    queryFn: () => client.get<DemoHealth>('/demo/health'),
    enabled: data !== null,
    refetchInterval: 15_000,
    retry: false,
  })

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
