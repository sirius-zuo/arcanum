import clsx from 'clsx'
import type { RenderFormat } from '../api/types'
import { CopyButton } from './CopyButton'

const FORMATS: RenderFormat[] = ['xml', 'markdown', 'numbered']

interface RenderTabsProps {
  format: RenderFormat
  onFormat: (f: RenderFormat) => void
  /** The exact string the API returned for `format`, or undefined while it is being fetched. */
  rendered: string | undefined
  pending?: boolean
}

export function RenderTabs({ format, onFormat, rendered, pending }: RenderTabsProps) {
  return (
    <div>
      <div className="mb-2 flex items-center justify-between gap-2">
        <div role="tablist" aria-label="Render format" className="inline-flex rounded-lg border border-border bg-surface-2 p-0.5">
          {FORMATS.map((f) => (
            <button
              key={f}
              type="button"
              role="tab"
              aria-selected={f === format}
              onClick={() => onFormat(f)}
              className={clsx(
                'h-7 rounded-md px-3 font-mono text-xs transition',
                f === format ? 'bg-surface font-medium shadow-soft' : 'text-muted hover:text-text',
              )}
            >
              {f}
            </button>
          ))}
        </div>
        {rendered !== undefined && <CopyButton text={rendered} label="Copy rendered context" showLabel />}
      </div>
      <pre
        role="tabpanel"
        aria-busy={pending ? true : undefined}
        className={clsx(
          'max-h-96 overflow-auto whitespace-pre-wrap break-words rounded-lg border border-border bg-surface-2 p-3 font-mono text-xs leading-5',
          pending && 'opacity-60',
        )}
      >
        {rendered ?? (pending ? 'Rendering...' : 'No rendered output.')}
      </pre>
    </div>
  )
}
