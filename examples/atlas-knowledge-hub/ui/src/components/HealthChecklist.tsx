import { useState } from 'react'
import { Check, Copy, X } from 'lucide-react'
import type { HealthCheck } from '../api/types'

function CopyBlock({ text }: { text: string }) {
  const [copied, setCopied] = useState(false)
  const copy = async () => {
    try {
      await navigator.clipboard.writeText(text)
      setCopied(true)
      setTimeout(() => setCopied(false), 1500)
    } catch {
      // clipboard blocked: the command stays selectable in the block
    }
  }
  return (
    <div className="mt-2 flex items-stretch overflow-hidden rounded-lg border border-border bg-surface-2">
      <pre className="min-w-0 flex-1 overflow-x-auto px-3 py-2 font-mono text-xs">{text}</pre>
      <button
        type="button"
        onClick={copy}
        aria-label={copied ? 'Copied' : 'Copy command'}
        className="flex items-center gap-1.5 border-l border-border px-3 text-xs font-medium text-muted transition hover:text-text"
      >
        {copied ? <Check className="h-3.5 w-3.5" aria-hidden="true" /> : <Copy className="h-3.5 w-3.5" aria-hidden="true" />}
        {copied ? 'Copied' : 'Copy'}
      </button>
    </div>
  )
}

export function HealthChecklist({ checks }: { checks: HealthCheck[] }) {
  return (
    <ul className="divide-y divide-border">
      {checks.map((c) => (
        <li key={c.id} className="flex items-start gap-3 py-2.5">
          <span
            className={
              c.ok
                ? 'mt-0.5 grid h-5 w-5 shrink-0 place-items-center rounded-full bg-v-supported/10 text-v-supported'
                : 'mt-0.5 grid h-5 w-5 shrink-0 place-items-center rounded-full bg-v-unsupported/10 text-v-unsupported'
            }
          >
            {c.ok ? <Check className="h-3 w-3" aria-label="Passing" /> : <X className="h-3 w-3" aria-label="Failing" />}
          </span>
          <div className="min-w-0 flex-1">
            <p className="text-sm font-medium">{c.label}</p>
            <p className="text-xs text-muted">{c.detail}</p>
            {!c.ok && c.fix && <CopyBlock text={c.fix} />}
          </div>
        </li>
      ))}
    </ul>
  )
}
