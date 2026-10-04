import { Check, Copy } from 'lucide-react'
import { useEffect, useRef, useState } from 'react'

interface CopyButtonProps {
  text: string
  label: string
  /** Show the label next to the icon. */
  showLabel?: boolean
}

export function CopyButton({ text, label, showLabel }: CopyButtonProps) {
  const [done, setDone] = useState(false)
  const timer = useRef<number | undefined>(undefined)
  useEffect(() => () => window.clearTimeout(timer.current), [])

  const copy = async () => {
    try {
      await navigator.clipboard.writeText(text)
      setDone(true)
      window.clearTimeout(timer.current)
      timer.current = window.setTimeout(() => setDone(false), 1500)
    } catch {
      // clipboard blocked: nothing else to do, the text stays selectable
    }
  }
  const Icon = done ? Check : Copy
  return (
    <button
      type="button"
      onClick={() => void copy()}
      aria-label={label}
      title={label}
      className="inline-flex h-6 items-center gap-1 rounded-md px-1.5 text-xs text-muted transition hover:bg-surface-2 hover:text-text"
    >
      <Icon className="h-3.5 w-3.5" aria-hidden="true" />
      {showLabel && (done ? 'Copied' : 'Copy')}
    </button>
  )
}
