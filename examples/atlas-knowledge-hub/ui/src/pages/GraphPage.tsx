import { MessageSquareText, Network } from 'lucide-react'
import { Link, useNavigate } from 'react-router-dom'
import { useGraph } from '../api/graph'
import { EmptyState } from '../components/EmptyState'
import { ErrorState } from '../components/ErrorState'
import { GraphCanvas } from '../components/GraphCanvas'
import { HowItWorks } from '../components/HowItWorks'
import { PageHeader } from '../components/PageHeader'
import { Skeleton } from '../components/Skeleton'
import { ASK_PREFILL_KEY } from '../lib/askPrefill'
import { ROUTES, routeMeta } from '../routes'

const meta = routeMeta('/graph')
const step = String(ROUTES.indexOf(meta) + 1).padStart(2, '0')

export const MULTI_HOP_QUESTION = 'Who is the on-call lead for the team that owns the navigation stack?'

export default function GraphPage() {
  const graph = useGraph()
  const navigate = useNavigate()

  const askMultiHop = () => {
    try {
      sessionStorage.setItem(ASK_PREFILL_KEY, MULTI_HOP_QUESTION)
    } catch {
      // storage blocked: Ask opens without the prefilled question
    }
    navigate('/ask')
  }

  return (
    <>
      <PageHeader
        eyebrow={`${step} / ${meta.label}`}
        title={meta.label}
        description={meta.blurb}
        actions={
          <>
            <button type="button" onClick={askMultiHop} className="inline-flex h-9 items-center gap-2 rounded-lg border border-border bg-surface px-3 text-sm font-medium hover:shadow-soft">
              <MessageSquareText className="h-4 w-4" aria-hidden="true" />
              Ask a multi-hop question
            </button>
            <HowItWorks arcanum={['GET /api/v1/graph']} demo={[]} />
          </>
        }
      />
      {graph.isPending ? (
        <Skeleton className="h-96 w-full" />
      ) : graph.isError ? (
        <ErrorState title="Could not load the graph" message={graph.error instanceof Error ? graph.error.message : 'The server did not answer.'} />
      ) : graph.data.nodes.length === 0 ? (
        <EmptyState
          icon={Network}
          title="No entities yet"
          description="Entities and relations appear after documents are ingested with the full pipeline, which runs entity extraction."
          action={
            <Link to="/library" className="text-sm font-medium text-accent underline-offset-2 hover:underline">
              Go to the Library
            </Link>
          }
        />
      ) : (
        <GraphCanvas graph={graph.data} />
      )}
    </>
  )
}
