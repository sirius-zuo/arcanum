import { ArrowUpRight } from 'lucide-react'
import type { LucideIcon } from 'lucide-react'
import { Link } from 'react-router-dom'
import { Card } from './Card'
import { Chip } from './Chip'
import type { ChipTone } from './Chip'

export interface CapabilityStatus {
  label: string
  tone?: ChipTone
  mono?: boolean
}

interface CapabilityCardProps {
  title: string
  description: string
  to: string
  icon: LucideIcon
  status: CapabilityStatus
}

export function CapabilityCard({ title, description, to, icon: Icon, status }: CapabilityCardProps) {
  return (
    <Link to={to} className="group block rounded-card">
      <Card interactive className="flex h-full flex-col p-4">
        <div className="mb-3 flex items-start justify-between gap-2">
          <span className="grid h-8 w-8 place-items-center rounded-lg bg-accent/10 text-accent">
            <Icon className="h-4 w-4" aria-hidden="true" />
          </span>
          <ArrowUpRight className="h-4 w-4 text-muted opacity-0 transition group-hover:opacity-100 group-focus-visible:opacity-100" aria-hidden="true" />
        </div>
        <h3 className="text-sm font-semibold">{title}</h3>
        <p className="mt-1 flex-1 text-[13px] leading-relaxed text-muted">{description}</p>
        <div className="mt-3">
          <Chip tone={status.tone} mono={status.mono}>
            {status.label}
          </Chip>
        </div>
      </Card>
    </Link>
  )
}
