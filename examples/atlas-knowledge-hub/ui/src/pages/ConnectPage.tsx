import { Plug } from 'lucide-react'
import { useMcp } from '../api/admin'
import { CodeBlock } from '../components/CodeBlock'
import { Card } from '../components/Card'
import { ErrorState } from '../components/ErrorState'
import { McpPanel } from '../components/McpPanel'
import { PageHeader } from '../components/PageHeader'
import { Skeleton } from '../components/Skeleton'
import {
  contextSnippet,
  generateSnippet,
  generateStreamSnippet,
  mcpConfigSnippet,
  mcpListSnippet,
  verifySnippet,
} from '../lib/snippets'
import { ROUTES, routeMeta } from '../routes'
import { useBootstrap } from '../state/bootstrap'

const meta = routeMeta('/connect')
const step = String(ROUTES.indexOf(meta) + 1).padStart(2, '0')

export default function ConnectPage() {
  const { data } = useBootstrap()
  const mcp = useMcp()

  if (!data) {
    return (
      <>
        <PageHeader eyebrow={`${step} / ${meta.label}`} title={meta.label} description={meta.blurb} />
        <Skeleton className="h-48" />
      </>
    )
  }

  const endpoint = mcp.data?.endpoint ?? `http://localhost:${data.mcp_port}/mcp`
  const input = { origin: window.location.origin, apiKey: data.api_key, collection: data.collection, mcpEndpoint: endpoint }

  return (
    <>
      <PageHeader eyebrow={`${step} / ${meta.label}`} title={meta.label} description={meta.blurb} />
      <div className="space-y-6">
        <Card className="p-5">
          <h2 className="flex items-center gap-2 text-base font-semibold">
            <Plug className="h-4 w-4 text-accent" aria-hidden="true" />
            MCP endpoint
          </h2>
          <p className="mb-4 mt-1 text-sm text-muted">
            The MCP server runs on its own port (<span className="font-mono">{data.mcp_port}</span>). Point any MCP client at it.
          </p>
          <div className="space-y-3">
            <CodeBlock title="MCP client config" code={mcpConfigSnippet(input)} />
            <CodeBlock title="List the tools with curl" code={mcpListSnippet(input)} />
          </div>
        </Card>

        <Card className="p-5">
          <h2 className="text-base font-semibold">REST API</h2>
          <p className="mb-4 mt-1 text-sm text-muted">
            These snippets use the demo API key for this local session and collection <span className="font-mono">{data.collection}</span>.
          </p>
          <div className="space-y-3">
            <CodeBlock title="Context" code={contextSnippet(input)} />
            <CodeBlock title="Generate" code={generateSnippet(input)} />
            <CodeBlock title="Generate (streaming)" code={generateStreamSnippet(input)} />
            <CodeBlock title="Verify" code={verifySnippet(input)} note="Use the ref_id and chunk_ids of a Context response as the passages." />
          </div>
        </Card>

        <Card className="p-5">
          <h2 className="text-base font-semibold">Live tool list</h2>
          <p className="mb-4 mt-1 text-sm text-muted">Fetched from the MCP server with tools/list.</p>
          {mcp.isPending && <Skeleton className="h-24" />}
          {mcp.isError && <ErrorState title="Could not read the MCP tool list" message={mcp.error.message} />}
          {mcp.data && <McpPanel mcp={mcp.data} />}
        </Card>
      </div>
    </>
  )
}
