import { AlertTriangle } from 'lucide-react'
import type { ReactNode } from 'react'

interface ErrorStateProps {
  title: string
  message: string
  /** A command or step the user can run to fix this, shown in monospace. */
  fix?: string | null
  action?: ReactNode
}

export function ErrorState({ title, message, fix, action }: ErrorStateProps) {
  return (
    <div role="alert" className="rounded-card border border-v-unsupported/30 bg-v-unsupported/5 p-5">
      <div className="flex items-start gap-3">
        <AlertTriangle className="mt-0.5 h-5 w-5 shrink-0 text-v-unsupported" aria-hidden="true" />
        <div className="min-w-0 flex-1">
          <h2 className="text-sm font-semibold">{title}</h2>
          <p className="mt-1 text-sm text-muted">{message}</p>
          {fix && (
            <pre className="mt-3 overflow-x-auto rounded-lg border border-border bg-surface-2 px-3 py-2 font-mono text-xs">
              {fix}
            </pre>
          )}
          {action && <div className="mt-3">{action}</div>}
        </div>
      </div>
    </div>
  )
}
