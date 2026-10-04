import { Check, Copy } from 'lucide-react'
import { useEffect, useRef, useState } from 'react'

interface CodeBlockProps {
  title: string
  code: string
  note?: string
}

/** A snippet with a copy button. If the clipboard is blocked, a selectable textarea takes over. */
export function CodeBlock({ title, code, note }: CodeBlockProps) {
  const [done, setDone] = useState(false)
  const [manual, setManual] = useState(false)
  const timer = useRef<number | undefined>(undefined)
  const area = useRef<HTMLTextAreaElement>(null)
  useEffect(() => () => window.clearTimeout(timer.current), [])
  useEffect(() => {
    if (manual) area.current?.select()
  }, [manual])

  const copy = async () => {
    try {
      await navigator.clipboard.writeText(code)
      setManual(false)
      setDone(true)
      window.clearTimeout(timer.current)
      timer.current = window.setTimeout(() => setDone(false), 1500)
    } catch {
      setDone(false)
      setManual(true)
    }
  }
  const Icon = done ? Check : Copy
  return (
    <figure className="overflow-hidden rounded-lg border border-border bg-surface-2">
      <figcaption className="flex items-center justify-between gap-2 border-b border-border px-3 py-1.5">
        <span className="text-xs font-medium">{title}</span>
        <button
          type="button"
          onClick={() => void copy()}
          aria-label={`Copy ${title}`}
          className="inline-flex h-6 items-center gap-1 rounded-md px-1.5 text-xs text-muted transition hover:bg-surface hover:text-text"
        >
          <Icon className="h-3.5 w-3.5" aria-hidden="true" />
          {done ? 'Copied' : 'Copy'}
        </button>
      </figcaption>
      {note && <p className="px-3 pt-2 text-xs text-muted">{note}</p>}
      {manual ? (
        <div className="p-3">
          <p role="status" className="mb-2 text-xs text-muted">
            The browser could not access the clipboard. Select the text below and copy it manually.
          </p>
          <textarea
            ref={area}
            readOnly
            value={code}
            aria-label={`${title}, select and copy manually`}
            rows={Math.min(14, code.split('\n').length)}
            className="w-full rounded-md border border-border bg-surface p-2 font-mono text-xs"
          />
        </div>
      ) : (
        <pre tabIndex={0} className="overflow-x-auto px-3 py-2 font-mono text-xs leading-relaxed">
          {code}
        </pre>
      )}
    </figure>
  )
}
