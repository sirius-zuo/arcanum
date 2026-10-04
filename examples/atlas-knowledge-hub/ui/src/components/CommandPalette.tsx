import { useEffect, useId, useMemo, useRef, useState } from 'react'
import type { KeyboardEvent as ReactKeyboardEvent } from 'react'
import { CornerDownLeft, Search, Zap } from 'lucide-react'
import { useNavigate } from 'react-router-dom'
import { ROUTES } from '../routes'

export interface PaletteAction {
  id: string
  label: string
  run: () => void
  /** Why the action cannot run right now; the entry is listed but inert. */
  disabledReason?: string
}

interface Entry {
  id: string
  label: string
  group: 'Go to' | 'Actions'
  run: () => void
  disabledReason?: string
}

/** Subsequence match; lower is better, null means no match. Substring hits beat scattered ones. */
export function fuzzyScore(query: string, text: string): number | null {
  const q = query.trim().toLowerCase()
  if (!q) return 0
  const t = text.toLowerCase()
  const at = t.indexOf(q)
  if (at >= 0) return at === 0 ? 0 : 1 + at / 100
  let ti = 0
  let gaps = 0
  for (const ch of q) {
    const found = t.indexOf(ch, ti)
    if (found < 0) return null
    gaps += found - ti
    ti = found + 1
  }
  return 10 + gaps
}

export function CommandPalette({ actions }: { actions: PaletteAction[] }) {
  const navigate = useNavigate()
  const [open, setOpen] = useState(false)
  const [query, setQuery] = useState('')
  const [active, setActive] = useState(0)
  const opener = useRef<HTMLElement | null>(null)
  const input = useRef<HTMLInputElement>(null)
  const listId = useId()

  const entries = useMemo<Entry[]>(
    () => [
      ...ROUTES.map<Entry>((r) => ({ id: `route:${r.path}`, label: r.label, group: 'Go to', run: () => navigate(r.path) })),
      ...actions.map<Entry>((a) => ({ id: `action:${a.id}`, label: a.label, group: 'Actions', run: a.run, disabledReason: a.disabledReason })),
    ],
    [actions, navigate],
  )

  const results = useMemo(() => {
    const scored: { entry: Entry; score: number }[] = []
    for (const entry of entries) {
      const score = fuzzyScore(query, entry.label)
      if (score !== null) scored.push({ entry, score })
    }
    return scored.sort((a, b) => a.score - b.score).map((s) => s.entry)
  }, [entries, query])

  const show = () => {
    if (document.activeElement instanceof HTMLElement) opener.current = document.activeElement
    setQuery('')
    setActive(0)
    setOpen(true)
  }
  const hide = () => {
    setOpen(false)
    opener.current?.focus()
    opener.current = null
  }

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === 'k') {
        e.preventDefault()
        if (open) hide()
        else show()
      }
    }
    document.addEventListener('keydown', onKey)
    return () => document.removeEventListener('keydown', onKey)
  })

  useEffect(() => {
    if (open) input.current?.focus()
  }, [open])

  if (!open) return null

  const choose = (entry: Entry | undefined) => {
    if (!entry || entry.disabledReason) return
    hide()
    entry.run()
  }

  const onKeyDown = (e: ReactKeyboardEvent) => {
    if (e.key === 'ArrowDown') {
      e.preventDefault()
      setActive((a) => Math.min(a + 1, results.length - 1))
    } else if (e.key === 'ArrowUp') {
      e.preventDefault()
      setActive((a) => Math.max(a - 1, 0))
    } else if (e.key === 'Enter') {
      e.preventDefault()
      choose(results[active])
    } else if (e.key === 'Tab') {
      // The input is the only tab stop in the dialog: keep focus inside it.
      e.preventDefault()
    } else if (e.key === 'Escape') {
      e.stopPropagation()
      hide()
    }
  }

  return (
    <div className="fixed inset-0 z-50 flex items-start justify-center bg-text/30 px-4 pt-[14vh] backdrop-blur-sm" onMouseDown={hide}>
      <div
        role="dialog"
        aria-modal="true"
        aria-label="Command palette"
        onKeyDown={onKeyDown}
        onMouseDown={(e) => e.stopPropagation()}
        className="w-full max-w-lg overflow-hidden rounded-card border border-border bg-surface shadow-lift animate-rise"
      >
        <div className="flex items-center gap-3 border-b border-border px-4">
          <Search className="h-4 w-4 text-muted" aria-hidden="true" />
          <input
            ref={input}
            role="combobox"
            aria-expanded="true"
            aria-controls={listId}
            aria-activedescendant={results[active] ? `${listId}-${active}` : undefined}
            aria-label="Search pages and actions"
            value={query}
            onChange={(e) => {
              setQuery(e.target.value)
              setActive(0)
            }}
            placeholder="Jump to a page or run an action"
            className="h-12 flex-1 bg-transparent text-sm outline-none placeholder:text-muted"
          />
          <kbd className="rounded border border-border px-1.5 py-0.5 font-mono text-[10px] text-muted">Esc</kbd>
        </div>
        <ul id={listId} role="listbox" aria-label="Results" className="max-h-80 overflow-y-auto p-2">
          {results.length === 0 && <li className="px-3 py-6 text-center text-sm text-muted">Nothing matches that.</li>}
          {results.map((r, i) => (
            <li
              key={r.id}
              id={`${listId}-${i}`}
              role="option"
              aria-selected={i === active}
              aria-disabled={r.disabledReason ? true : undefined}
              onMouseMove={() => setActive(i)}
              onClick={() => choose(r)}
              className={
                'flex cursor-pointer items-center justify-between gap-3 rounded-lg px-3 py-2 text-sm ' +
                (r.disabledReason ? 'cursor-not-allowed text-muted ' : '') +
                (i === active ? 'bg-accent/10 text-accent' : '')
              }
            >
              <span className="flex items-center gap-2">
                {r.group === 'Actions' && <Zap className="h-3.5 w-3.5" aria-hidden="true" />}
                {r.label}
              </span>
              <span className="flex items-center gap-1.5 text-xs text-muted">
                {r.disabledReason ?? r.group}
                {i === active && !r.disabledReason && <CornerDownLeft className="h-3 w-3" aria-hidden="true" />}
              </span>
            </li>
          ))}
        </ul>
      </div>
    </div>
  )
}
