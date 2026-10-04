import { ChevronRight } from 'lucide-react'
import type { DemoMcp } from '../api/types'

/** The live tool catalogue from `tools/list`, each schema collapsed until asked for. */
export function McpPanel({ mcp }: { mcp: DemoMcp }) {
  if (mcp.tools.length === 0) return <p className="text-sm text-muted">The MCP server reports no tools.</p>
  return (
    <ul className="divide-y divide-border rounded-lg border border-border">
      {mcp.tools.map((t) => (
        <li key={t.name} className="px-3 py-2.5">
          <p className="font-mono text-sm font-medium">{t.name}</p>
          <p className="mt-0.5 text-sm text-muted">{t.description}</p>
          <details className="group mt-1.5">
            <summary className="inline-flex cursor-pointer list-none items-center gap-1 text-xs text-accent">
              <ChevronRight className="h-3 w-3 transition group-open:rotate-90" aria-hidden="true" />
              Input schema
            </summary>
            <pre tabIndex={0} className="mt-2 overflow-x-auto rounded-md bg-surface-2 p-2 font-mono text-xs">
              {JSON.stringify(t.input_schema, null, 2)}
            </pre>
          </details>
        </li>
      ))}
    </ul>
  )
}
