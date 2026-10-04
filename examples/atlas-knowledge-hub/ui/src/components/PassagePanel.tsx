import clsx from 'clsx'
import { ChevronDown, ChevronUp } from 'lucide-react'
import { useEffect, useRef, useState } from 'react'
import type { ContextPassage } from '../api/types'
import { Chip } from './Chip'
import { StrategyBadge } from './StrategyBadge'

export interface PassageTarget {
  refId: string
  /** Changes on every click so clicking the same chip twice flashes twice. */
  tick: number
}

interface PassagePanelProps {
  passages: ContextPassage[]
  /** Ref ids the answer actually cites. */
  cited: ReadonlySet<string>
  target: PassageTarget | null
  idPrefix: string
}

export function passageDomId(prefix: string, refId: string): string {
  return `${prefix}-passage-${refId}`
}

const FLASH_MS = 1600

export function PassagePanel({ passages, cited, target, idPrefix }: PassagePanelProps) {
  const [open, setOpen] = useState<ReadonlySet<string>>(new Set())
  const [flash, setFlash] = useState<string | null>(null)
  const timer = useRef<number | undefined>(undefined)
  useEffect(() => () => window.clearTimeout(timer.current), [])

  useEffect(() => {
    if (!target) return
    setOpen((s) => new Set(s).add(target.refId))
    setFlash(target.refId)
    window.clearTimeout(timer.current)
    timer.current = window.setTimeout(() => setFlash(null), FLASH_MS)
    const el = document.getElementById(passageDomId(idPrefix, target.refId))
    el?.scrollIntoView?.({ behavior: 'smooth', block: 'nearest' })
  }, [target, idPrefix])

  const toggle = (ref: string) =>
    setOpen((s) => {
      const next = new Set(s)
      if (next.has(ref)) next.delete(ref)
      else next.add(ref)
      return next
    })

  return (
    <aside aria-label="Passages used for this answer" className="min-w-0">
      <h3 className="mb-2 flex items-center gap-2 text-xs font-semibold uppercase tracking-wider text-muted">
        Passages
        <span className="font-mono normal-case tracking-normal">{passages.length}</span>
      </h3>
      {passages.length === 0 ? (
        <p className="text-sm text-muted">No passages were retrieved.</p>
      ) : (
        <ol className="space-y-2 lg:max-h-[34rem] lg:overflow-y-auto lg:pr-1">
          {passages.map((p) => {
            const expanded = open.has(p.ref_id)
            const isCited = cited.has(p.ref_id)
            return (
              <li
                key={p.ref_id}
                id={passageDomId(idPrefix, p.ref_id)}
                data-flash={flash === p.ref_id ? 'true' : undefined}
                className={clsx(
                  'rounded-lg border bg-surface p-3 transition duration-500',
                  flash === p.ref_id ? 'border-accent bg-accent/10 shadow-lift ring-2 ring-accent/40' : 'border-border',
                  !isCited && 'opacity-80',
                )}
              >
                <div className="mb-1.5 flex flex-wrap items-center gap-1.5">
                  <Chip tone="accent" mono>
                    {p.ref_id}
                  </Chip>
                  {isCited ? <Chip tone="supported">cited</Chip> : <Chip>not cited</Chip>}
                  {p.strategies.map((s) => (
                    <StrategyBadge key={s} strategy={s} />
                  ))}
                </div>
                <p className={clsx('whitespace-pre-wrap text-[13px] leading-relaxed', !expanded && 'line-clamp-3')}>{p.text}</p>
                <button
                  type="button"
                  aria-expanded={expanded}
                  onClick={() => toggle(p.ref_id)}
                  className="mt-1 inline-flex items-center gap-1 text-[11px] text-muted transition hover:text-text"
                >
                  {expanded ? <ChevronUp className="h-3 w-3" aria-hidden="true" /> : <ChevronDown className="h-3 w-3" aria-hidden="true" />}
                  {expanded ? 'Show less' : 'Show full passage'}
                </button>
                <p className="mt-1.5 break-all border-t border-border pt-1.5 font-mono text-[11px] text-muted">
                  {p.source_uri} · v{p.version_num}
                  {p.page !== null && ` · page ${p.page}`}
                  {p.section && ` · ${p.section}`}
                  {' · bytes '}
                  {`${p.offset_start}-${p.offset_end}`}
                </p>
              </li>
            )
          })}
        </ol>
      )}
    </aside>
  )
}
