import { useMutation } from '@tanstack/react-query'
import { Play } from 'lucide-react'
import { runBenchmark } from '../api/lab'
import type { CorpusDoc } from '../api/lab'
import type { BenchmarkMetrics, ChunkStrategyConfig, GoldenQuery } from '../api/types'
import { useBootstrap } from '../state/bootstrap'
import { ErrorState } from './ErrorState'

export const describeStrategy = (c: ChunkStrategyConfig) => {
  const p = Object.entries(c.params)
  return p.length === 0 ? c.strategy : `${c.strategy} (${p.map(([k, v]) => `${k}=${v}`).join(', ')})`
}

interface BenchmarkPanelProps {
  strategies: ChunkStrategyConfig[]
  golden: GoldenQuery[]
  /** Loads the text of every document to benchmark over. */
  loadCorpus: () => Promise<CorpusDoc[]>
}

export function BenchmarkPanel({ strategies, golden, loadCorpus }: BenchmarkPanelProps) {
  const { client } = useBootstrap()
  const bench = useMutation<BenchmarkMetrics[], Error>({
    mutationFn: async () => runBenchmark(client, await loadCorpus(), golden, strategies),
  })
  const rows = bench.data

  return (
    <section aria-label="Benchmark" className="rounded-card border border-border bg-surface p-4">
      <div className="flex flex-wrap items-center justify-between gap-3">
        <div>
          <h3 className="text-sm font-semibold">Benchmark</h3>
          <p className="mt-1 max-w-2xl text-xs text-muted">
            Chunks every library document with each strategy, then scores the {golden.length} golden queries by plain word overlap
            (no embeddings). It compares how strategies slice text, not how the live index ranks.
          </p>
        </div>
        <button
          type="button"
          onClick={() => bench.mutate()}
          disabled={bench.isPending || strategies.length === 0 || golden.length === 0}
          className="inline-flex h-9 items-center gap-2 rounded-lg bg-accent px-3 text-sm font-medium text-accent-fg transition hover:opacity-90 disabled:opacity-50"
        >
          <Play className="h-4 w-4" aria-hidden="true" />
          {bench.isPending ? 'Running...' : 'Run benchmark'}
        </button>
      </div>
      {bench.isError && (
        <div className="mt-3">
          <ErrorState title="Benchmark failed" message={bench.error.message} />
        </div>
      )}
      {rows && (
        <div className="mt-3 overflow-x-auto">
          <table className="w-full min-w-[560px] text-left text-sm">
            <thead className="font-mono text-[11px] uppercase tracking-[0.08em] text-muted">
              <tr>
                <th className="py-1.5 pr-3 font-medium">Strategy</th>
                <th className="px-3 py-1.5 text-right font-medium">Recall@5</th>
                <th className="px-3 py-1.5 text-right font-medium">Recall@10</th>
                <th className="px-3 py-1.5 text-right font-medium">Mean tokens</th>
                <th className="px-3 py-1.5 text-right font-medium">p50</th>
                <th className="py-1.5 pl-3 text-right font-medium">p95</th>
              </tr>
            </thead>
            <tbody className="font-mono text-xs tabular-nums">
              {rows.map((r, i) => (
                <tr key={i} className="border-t border-border">
                  <td className="py-2 pr-3">{describeStrategy(r.strategy)}</td>
                  <td className="px-3 py-2 text-right">{r.recall_at_5.toFixed(2)}</td>
                  <td className="px-3 py-2 text-right">{r.recall_at_10.toFixed(2)}</td>
                  <td className="px-3 py-2 text-right">{r.mean_chunk_tokens.toFixed(1)}</td>
                  <td className="px-3 py-2 text-right">{r.chunk_size_p50.toFixed(0)}</td>
                  <td className="py-2 pl-3 text-right">{r.chunk_size_p95.toFixed(0)}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
    </section>
  )
}
