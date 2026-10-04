import { useMutation } from '@tanstack/react-query'
import { Play } from 'lucide-react'
import { Link } from 'react-router-dom'
import { ApiError } from '../api/client'
import { runEval } from '../api/lab'
import type { EvalResponse } from '../api/types'
import { useBootstrap } from '../state/bootstrap'
import { Chip } from './Chip'
import { ErrorState } from './ErrorState'
import { Stat } from './Stat'

const f = (n: number) => n.toFixed(2)

export function EvalView({ data }: { data: EvalResponse }) {
  const { report, queries } = data
  const k = report.k
  return (
    <div className="space-y-4">
      <div className="grid grid-cols-2 gap-4 rounded-card border border-border bg-surface p-4 sm:grid-cols-3 lg:grid-cols-5">
        <Stat label={`Hit rate@${k}`} value={f(report.hit_rate_at_k)} />
        <Stat label="MRR" value={f(report.mrr)} />
        <Stat label={`NDCG@${k}`} value={f(report.ndcg_at_k)} />
        <Stat label={`Precision@${k}`} value={f(report.precision_at_k)} hint={`Cannot exceed ${f(1 / k)} (1/${k})`} />
        <Stat label={`Recall@${k}`} value={f(report.recall_at_k)} />
      </div>
      <p className="text-xs text-muted">
        Precision@{k} is bounded by 1/{k} = {f(1 / k)} because each golden query has exactly one relevant document, so even a perfect
        ranking fills at most one of the {k} slots with a relevant result. Read hit rate, MRR and recall for quality.
      </p>
      <div className="overflow-x-auto rounded-card border border-border bg-surface">
        <table className="w-full min-w-[520px] text-left text-sm">
          <caption className="sr-only">Per-query results</caption>
          <thead className="font-mono text-[11px] uppercase tracking-[0.08em] text-muted">
            <tr>
              <th className="px-4 py-2 font-medium">Query</th>
              <th className="px-3 py-2 font-medium">Expected document</th>
              <th className="px-4 py-2 text-right font-medium">First relevant rank</th>
            </tr>
          </thead>
          <tbody>
            {queries.map((q, i) => (
              <tr key={i} className="border-t border-border">
                <td className="px-4 py-2">{q.query}</td>
                <td className="px-3 py-2 font-mono text-xs">{q.relevant_source_uri}</td>
                <td className="px-4 py-2 text-right">
                  {q.first_relevant_rank === null ? (
                    <Chip tone="unsupported">not in top {k}</Chip>
                  ) : (
                    <Chip tone={q.first_relevant_rank === 1 ? 'supported' : 'partial'} mono>
                      rank {q.first_relevant_rank}
                    </Chip>
                  )}
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    </div>
  )
}

export function EvalPanel() {
  const { client } = useBootstrap()
  const run = useMutation<EvalResponse, Error>({ mutationFn: () => runEval(client) })
  return (
    <div className="space-y-4">
      <div className="flex flex-wrap items-center justify-between gap-3">
        <p className="max-w-2xl text-sm text-muted">
          These are document-level golden queries over the sample corpus: each query names one source document, and a result counts as
          a hit when the chunk returned for that document ranks high. Load the sample corpus first so there is something to retrieve.
        </p>
        <button
          type="button"
          onClick={() => run.mutate()}
          disabled={run.isPending}
          className="inline-flex h-9 items-center gap-2 rounded-lg bg-accent px-3 text-sm font-medium text-accent-fg transition hover:opacity-90 disabled:opacity-50"
        >
          <Play className="h-4 w-4" aria-hidden="true" />
          {run.isPending ? 'Evaluating...' : 'Run evaluation'}
        </button>
      </div>
      {run.isError &&
        (run.error instanceof ApiError && run.error.status === 409 ? (
          <ErrorState
            title="Nothing to evaluate yet"
            message={`${run.error.message}. Load the sample corpus first.`}
            action={
              <Link to="/library" className="text-sm font-medium text-accent underline">
                Open Library
              </Link>
            }
          />
        ) : (
          <ErrorState title="Evaluation failed" message={run.error.message} fix="Check Overview for the health of Ollama and the models." />
        ))}
      {run.data && <EvalView data={run.data} />}
    </div>
  )
}
