import { useRef, useState } from 'react'
import clsx from 'clsx'
import { AlertTriangle, CheckCircle2, UploadCloud } from 'lucide-react'
import { checkUploadFile, submitUpload } from '../api/ingest'
import { useBootstrap } from '../state/bootstrap'
import { useOperations } from '../state/operations'

interface FileResult {
  id: number
  name: string
  ok: boolean
  message: string
}

let nextId = 0

export function UploadZone() {
  const { client } = useBootstrap()
  const { track } = useOperations()
  const input = useRef<HTMLInputElement>(null)
  const [over, setOver] = useState(false)
  const [results, setResults] = useState<FileResult[]>([])

  async function handle(list: FileList | File[]) {
    const files = Array.from(list)
    for (const file of files) {
      const reason = checkUploadFile(file)
      if (reason) {
        setResults((r) => [{ id: nextId++, name: file.name, ok: false, message: reason }, ...r])
        continue
      }
      try {
        const res = await submitUpload(client, file)
        track([{ source_uri: file.name, operation_id: res.operation_id, replay: res.replay }])
        const message = res.replay ? 'Replay: the server already has this exact upload.' : 'Accepted. Follow it in the operations list.'
        setResults((r) => [{ id: nextId++, name: file.name, ok: true, message }, ...r])
      } catch (e) {
        setResults((r) => [{ id: nextId++, name: file.name, ok: false, message: e instanceof Error ? e.message : String(e) }, ...r])
      }
    }
  }

  return (
    <div>
      <div
        onDragOver={(e) => {
          e.preventDefault()
          setOver(true)
        }}
        onDragLeave={() => setOver(false)}
        onDrop={(e) => {
          e.preventDefault()
          setOver(false)
          void handle(e.dataTransfer.files)
        }}
        className={clsx(
          'flex flex-col items-center rounded-card border border-dashed px-4 py-6 text-center transition',
          over ? 'border-accent bg-accent/5' : 'border-border',
        )}
      >
        <UploadCloud className="mb-2 h-5 w-5 text-accent" aria-hidden="true" />
        <p className="text-sm font-medium">Drop .md or .txt files here</p>
        <p className="mt-1 max-w-xs text-xs text-muted">Plain text only: this demo does not extract text from other formats. Re-uploading the same file is a safe replay.</p>
        <input
          ref={input}
          type="file"
          multiple
          accept=".md,.txt"
          className="sr-only"
          aria-label="Choose files to upload"
          onChange={(e) => {
            if (e.target.files) void handle(e.target.files)
            e.target.value = ''
          }}
        />
        <button
          type="button"
          onClick={() => input.current?.click()}
          className="mt-3 h-8 rounded-lg border border-border bg-surface px-3 text-xs font-medium transition hover:shadow-soft"
        >
          Choose files
        </button>
      </div>
      {results.length > 0 && (
        <ul className="mt-3 space-y-1.5" aria-label="Upload results">
          {results.slice(0, 6).map((r) => (
            <li key={r.id} className="flex items-start gap-2 text-xs" role={r.ok ? undefined : 'alert'}>
              {r.ok ? (
                <CheckCircle2 className="mt-0.5 h-3.5 w-3.5 shrink-0 text-v-supported" aria-hidden="true" />
              ) : (
                <AlertTriangle className="mt-0.5 h-3.5 w-3.5 shrink-0 text-v-unsupported" aria-hidden="true" />
              )}
              <span>
                <span className="font-mono font-medium">{r.name}</span>
                <span className="ml-1.5 text-muted">{r.message}</span>
              </span>
            </li>
          ))}
        </ul>
      )}
    </div>
  )
}
