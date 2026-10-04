import { Check, X } from 'lucide-react'
import { useEffect, useRef } from 'react'
import type { KeyboardEvent } from 'react'
import clsx from 'clsx'
import { useTour } from '../state/tour'
import { TourStepCard } from './TourStepCard'

/**
 * Non-blocking guide card. It sits at the bottom of the content column in normal flow (sticky), so on wide
 * screens it never covers the Inspector or the rail and on narrow screens it acts as a bottom sheet; the page
 * keeps its own scroll space beneath it, so forms near the page end stay reachable.
 */
export function TourOverlay() {
  const { state, steps, startCount, next, prev, goto, dismiss } = useTour()
  const root = useRef<HTMLElement>(null)
  const seen = useRef(startCount)

  // Move focus only after an explicit start, never on reload or when a step completes.
  useEffect(() => {
    if (startCount !== seen.current) {
      seen.current = startCount
      root.current?.focus()
    }
  }, [startCount])

  if (!state.active || steps.length === 0) return null
  const index = Math.min(state.index, steps.length - 1)
  const step = steps[index]
  const done = state.completed[step.id] === true
  const last = index === steps.length - 1

  const onKeyDown = (e: KeyboardEvent<HTMLElement>) => {
    const t = e.target as HTMLElement
    if (t.tagName === 'INPUT' || t.tagName === 'TEXTAREA' || t.tagName === 'SELECT') return
    if (e.key === 'ArrowRight') {
      e.preventDefault()
      next()
    } else if (e.key === 'ArrowLeft') {
      e.preventDefault()
      prev()
    } else if (e.key === 'Escape') {
      e.preventDefault()
      dismiss()
    }
  }

  const btn = 'inline-flex h-8 items-center rounded-lg border border-border bg-surface px-3 text-sm font-medium transition hover:shadow-soft disabled:cursor-not-allowed disabled:opacity-50'

  return (
    <aside
      ref={root}
      tabIndex={-1}
      aria-label="Guided tour"
      onKeyDown={onKeyDown}
      className="sticky bottom-0 z-20 mx-auto w-full max-w-[1200px] px-3 pb-3 outline-none sm:px-8 sm:pb-4"
    >
      <div className="max-h-[50vh] overflow-y-auto rounded-card border border-border bg-surface p-4 shadow-lift motion-safe:animate-rise sm:ml-auto sm:max-w-md">
        <div className="mb-2 flex items-start justify-between gap-3">
          <ol className="flex flex-wrap gap-1" aria-label="Tour progress">
            {steps.map((s, i) => {
              const complete = state.completed[s.id] === true
              return (
                <li key={s.id}>
                  <button
                    type="button"
                    onClick={() => goto(i)}
                    aria-label={`Step ${i + 1}: ${s.title}${complete ? ' (complete)' : ''}`}
                    aria-current={i === index ? 'step' : undefined}
                    className={clsx(
                      'grid h-6 w-6 place-items-center rounded-full border text-[11px] font-medium transition',
                      i === index ? 'border-accent bg-accent text-accent-fg' : complete ? 'border-accent text-accent' : 'border-border text-muted hover:text-text',
                    )}
                  >
                    {complete && i !== index ? <Check className="h-3 w-3" aria-hidden="true" /> : i + 1}
                  </button>
                </li>
              )
            })}
          </ol>
          <button
            type="button"
            onClick={dismiss}
            aria-label="Close tour"
            className="grid h-7 w-7 shrink-0 place-items-center rounded-lg text-muted transition hover:bg-surface-2 hover:text-text"
          >
            <X className="h-4 w-4" aria-hidden="true" />
          </button>
        </div>
        <TourStepCard step={step} position={index + 1} total={steps.length} done={done} />
        <div className="mt-3 flex items-center justify-between gap-2">
          <button type="button" onClick={prev} disabled={index === 0} className={btn}>
            Previous
          </button>
          {last ? (
            <button type="button" onClick={dismiss} className={clsx(btn, done && 'border-accent bg-accent text-accent-fg')}>
              Finish
            </button>
          ) : (
            <button type="button" onClick={next} className={clsx(btn, done && 'border-accent bg-accent text-accent-fg')}>
              Next
            </button>
          )}
        </div>
      </div>
    </aside>
  )
}
