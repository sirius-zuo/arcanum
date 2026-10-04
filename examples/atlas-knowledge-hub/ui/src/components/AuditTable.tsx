import type { AuditRecord } from '../api/admin'
import { Chip } from './Chip'

function when(ts: string): string {
  const d = new Date(ts)
  return Number.isNaN(d.getTime()) ? ts : d.toLocaleTimeString()
}

export function AuditTable({ records }: { records: AuditRecord[] }) {
  if (records.length === 0) return <p className="text-sm text-muted">No audit records yet. They appear as soon as the API is used.</p>
  return (
    <div className="max-h-96 overflow-auto rounded-lg border border-border">
      <table className="w-full text-left text-xs">
        <caption className="sr-only">Audit log, newest first</caption>
        <thead className="sticky top-0 bg-surface-2 text-muted">
          <tr>
            <th scope="col" className="px-3 py-2 font-medium">Time</th>
            <th scope="col" className="px-3 py-2 font-medium">Operation</th>
            <th scope="col" className="px-3 py-2 font-medium">User</th>
            <th scope="col" className="px-3 py-2 font-medium">Collection</th>
            <th scope="col" className="px-3 py-2 font-medium">Result</th>
          </tr>
        </thead>
        <tbody className="divide-y divide-border">
          {records.map((r, i) => (
            <tr key={`${r.timestamp}-${i}`}>
              <td className="whitespace-nowrap px-3 py-1.5 font-mono">{when(r.timestamp)}</td>
              <td className="px-3 py-1.5 font-mono">{r.entry.operation}</td>
              <td className="px-3 py-1.5">{r.entry.user_id}</td>
              <td className="px-3 py-1.5">{r.entry.collection_id}</td>
              <td className="px-3 py-1.5">
                <Chip mono>{r.entry.result.length > 60 ? `${r.entry.result.slice(0, 60)}...` : r.entry.result}</Chip>
              </td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  )
}
