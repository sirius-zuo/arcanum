import { useEffect, useId, useRef, useState } from 'react'
import { Info } from 'lucide-react'

interface HowItWorksProps {
  /** Calls that are Arcanum's own API. */
  arcanum: string[]
  /** Calls that are /demo helpers this example adds. */
  demo: string[]
}

function Group({ title, note, items }: { title: string; note: string; items: string[] }) {
  if (items.length === 0) return null
  return (
    <section className="mt-3 first:mt-0">
      <h3 className="text-xs font-semibold">{title}</h3>
      <p className="mb-1.5 text-xs text-muted">{note}</p>
      <ul className="space-y-1">
        {items.map((i) => (
          <li key={i} className="rounded-md bg-surface-2 px-2 py-1 font-mono text-[11px] leading-5">
            {i}
          </li>
        ))}
      </ul>
    </section>
  )
}

export function HowItWorks({ arcanum, demo }: HowItWorksProps) {
  const [open, setOpen] = useState(false)
  const panelId = useId()
  const root = useRef<HTMLDivElement>(null)

  useEffect(() => {
    if (!open) return
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') setOpen(false)
    }
    const onDown = (e: MouseEvent) => {
      if (root.current && !root.current.contains(e.target as Node)) setOpen(false)
    }
    document.addEventListener('keydown', onKey)
    document.addEventListener('mousedown', onDown)
    return () => {
      document.removeEventListener('keydown', onKey)
      document.removeEventListener('mousedown', onDown)
    }
  }, [open])

  return (
    <div ref={root} className="relative">
      <button
        type="button"
        aria-expanded={open}
        aria-controls={panelId}
        onClick={() => setOpen((o) => !o)}
        className="inline-flex h-8 items-center gap-1.5 rounded-lg border border-border bg-surface px-2.5 text-xs font-medium text-muted transition hover:text-text hover:shadow-soft"
      >
        <Info className="h-3.5 w-3.5" aria-hidden="true" />
        How this works
      </button>
      {open && (
        <div
          id={panelId}
          role="region"
          aria-label="How this works"
          className="absolute right-0 z-20 mt-2 w-[360px] max-w-[calc(100vw-2rem)] rounded-card border border-border bg-surface p-4 shadow-lift animate-rise"
        >
          <h2 className="mb-3 text-sm font-semibold">How this works</h2>
          <Group title="Arcanum's own API" note="Calls any client could make against Arcanum." items={arcanum} />
          <Group title="Demo helpers" note="Thin /demo routes this example adds for what the REST API lacks." items={demo} />
        </div>
      )}
    </div>
  )
}
