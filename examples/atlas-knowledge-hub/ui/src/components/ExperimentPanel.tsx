import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { useEffect, useState } from 'react'
import { abandonExperiment, buildExperimentSamples, evalExperiment, getExperiment, promoteExperiment, startExperiment } from '../api/lab'
import { ApiError } from '../api/client'
import type { Experiment, ExperimentStatus, GoldenQuery } from '../api/types'
import { readiness } from '../lib/experiment'
import { safeGet, safeRemove, safeSet } from '../lib/storage'
import { useBootstrap } from '../state/bootstrap'
import { describeStrategy } from './BenchmarkPanel'
import { Chip } from './Chip'
import type { ChipTone } from './Chip'
import { ErrorState } from './ErrorState'
import { PRESETS } from './StrategyPicker'

/** The server has no list route, so the id lives in localStorage. */
export const EXPERIMENT_KEY = 'atlas.lab.experiment'

const STATUS: Record<ExperimentStatus, { label: string; tone: ChipTone }> = {
  active: { label: 'active', tone: 'accent' },
  ready_to_promote: { label: 'ready to promote', tone: 'supported' },
  closed: { label: 'closed', tone: 'noclaim' },
}

const btn =
  'inline-flex h-9 items-center rounded-lg border border-border bg-surface px-3 text-sm font-medium transition hover:border-accent/40 disabled:opacity-50'

interface ExperimentPanelProps {
  collection: string
  golden: GoldenQuery[]
  samplesError?: string | null
}

export function ExperimentPanel({ collection, golden, samplesError }: ExperimentPanelProps) {
  const { client } = useBootstrap()
  const qc = useQueryClient()
  const [id, setId] = useState<string | null>(() => safeGet(EXPERIMENT_KEY))
  const [presetId, setPresetId] = useState('semantic-800')
  const [notice, setNotice] = useState<string | null>(null)

  const remember = (next: string | null) => {
    setId(next)
    if (next) safeSet(EXPERIMENT_KEY, next)
    else safeRemove(EXPERIMENT_KEY)
  }

  const exp = useQuery<Experiment, Error>({
    queryKey: ['lab', 'experiment', collection, id],
    queryFn: () => getExperiment(client, collection, id as string),
    enabled: id !== null,
    retry: false,
  })
  useEffect(() => {
    if (exp.error instanceof ApiError && exp.error.status === 404) {
      setId(null)
      safeRemove(EXPERIMENT_KEY)
      setNotice('The stored experiment was not found on the server (it may have restarted), so it was forgotten.')
    }
  }, [exp.error])
  const refresh = () => qc.invalidateQueries({ queryKey: ['lab', 'experiment', collection] })

  const start = useMutation<Experiment, Error>({
    mutationFn: () => {
      const preset = PRESETS.find((p) => p.id === presetId) ?? PRESETS[0]
      return startExperiment(client, collection, { vector: preset.config, lexical: null, graph: null, tree: null })
    },
    onSuccess: (e) => {
      setNotice(null)
      remember(e.experiment_id)
    },
  })
  const evaluate = useMutation<void, Error>({
    mutationFn: async () => {
      const { samples, skipped } = await buildExperimentSamples(client, collection, golden)
      if (samples.length === 0) throw new Error('None of the golden queries retrieved their expected document, so there are no labeled samples. Load the sample corpus first.')
      await evalExperiment(client, collection, id as string, samples)
      setNotice(skipped > 0 ? `${skipped} golden queries did not retrieve their expected document and were left out.` : null)
      await refresh()
    },
  })
  const promote = useMutation<{ message: string }, Error>({
    mutationFn: () => promoteExperiment(client, collection, id as string),
    onSuccess: async (r) => {
      setNotice(r.message)
      await refresh()
    },
  })
  const abandon = useMutation<void, Error>({
    mutationFn: () => abandonExperiment(client, collection, id as string),
    onSuccess: async () => {
      setNotice('Experiment abandoned.')
      await refresh()
    },
  })

  const clearOutcomes = () => {
    setNotice(null)
    start.reset()
    evaluate.reset()
    promote.reset()
    abandon.reset()
  }
  const data = exp.data
  const ready = readiness(data?.metrics)
  const closed = data?.status === 'closed'
  const busy = start.isPending || evaluate.isPending || promote.isPending || abandon.isPending
  const startError = start.error?.message

  return (
    <div className="space-y-4">
      <p className="max-w-2xl text-sm text-muted">
        A shadow experiment re-chunks the collection with a challenger strategy in a separate namespace and compares its recall@5 with the live
        champion. Only one experiment can be active per collection.
      </p>

      {(id === null || closed) && (
        <section aria-label="Start an experiment" className="flex flex-wrap items-end gap-3 rounded-card border border-border bg-surface p-4">
          <label className="text-xs font-medium text-muted">
            Challenger vector chunker
            <select
              value={presetId}
              onChange={(e) => setPresetId(e.target.value)}
              className="mt-1 block h-9 rounded-lg border border-border bg-surface px-2 font-mono text-xs text-text"
            >
              {PRESETS.map((p) => (
                <option key={p.id} value={p.id}>
                  {p.label}
                </option>
              ))}
            </select>
          </label>
          <button type="button" className={btn} disabled={busy} onClick={() => {
              clearOutcomes()
              start.mutate()
            }}>
            {start.isPending ? 'Starting...' : 'Start experiment'}
          </button>
        </section>
      )}
      {start.isError && <ErrorState title="Could not start the experiment" message={startError ?? 'Unknown error'} />}

      {id !== null && exp.isError && !(exp.error instanceof ApiError && exp.error.status === 404) && (
        <ErrorState
          title="Could not load the experiment"
          message={exp.error.message}
          action={
            <button type="button" className={btn} onClick={() => remember(null)}>
              Forget this experiment
            </button>
          }
        />
      )}

      {data && (
        <section aria-label="Experiment" className="space-y-3 rounded-card border border-border bg-surface p-4">
          <div className="flex flex-wrap items-center gap-2">
            <h3 className="text-sm font-semibold">Experiment</h3>
            <Chip tone={STATUS[data.status].tone}>{STATUS[data.status].label}</Chip>
            <Chip mono>{data.experiment_id.slice(0, 8)}</Chip>
            <span className="text-xs text-muted">challenger: {describeStrategy(data.challenger_config.vector)}</span>
          </div>

          {data.metrics ? (
            <dl className="grid grid-cols-2 gap-4 sm:grid-cols-4">
              {[
                ['Champion recall@5', data.metrics.champion_recall_at_5.toFixed(2)],
                ['Challenger recall@5', data.metrics.challenger_recall_at_5.toFixed(2)],
                ['Samples', String(data.metrics.sample_size)],
                ['Evaluated', new Date(data.metrics.computed_at).toLocaleTimeString()],
              ].map(([k, v]) => (
                <div key={k}>
                  <dt className="font-mono text-[11px] uppercase tracking-[0.08em] text-muted">{k}</dt>
                  <dd className="font-mono text-lg tabular-nums">{v}</dd>
                </div>
              ))}
            </dl>
          ) : (
            <p className="text-sm text-muted">No metrics yet.</p>
          )}

          <p role="status" className="rounded-lg bg-surface-2 px-3 py-2 text-sm">
            <span className="font-medium">{ready.ready ? 'Ready: ' : 'Not ready: '}</span>
            {ready.message}
          </p>

          {!closed && (
            <p className="text-xs text-muted">
              Caveat: the evaluation labels are champion chunk ids, and a challenger that re-chunks gets fresh ids in its shadow namespace, so it is
              structurally disadvantaged. This panel demonstrates the experiment lifecycle and the readiness rules, not a fair quality verdict.
            </p>
          )}
          {samplesError && <p className="text-xs text-v-unsupported">Could not load the golden queries: {samplesError}</p>}
          {!closed && (
            <div className="flex flex-wrap gap-2">
              <button type="button" className={btn} disabled={busy || golden.length === 0} onClick={() => {
                  clearOutcomes()
                  evaluate.mutate()
                }}>
                {evaluate.isPending ? 'Evaluating...' : 'Evaluate on golden queries'}
              </button>
              <button type="button" className={btn} disabled={busy} onClick={() => {
                  clearOutcomes()
                  promote.mutate()
                }}>
                Promote
              </button>
              <button type="button" className={btn} disabled={busy} onClick={() => {
                  clearOutcomes()
                  abandon.mutate()
                }}>
                Abandon
              </button>
            </div>
          )}
        </section>
      )}

      {evaluate.isError && <ErrorState title="Evaluation failed" message={evaluate.error.message} />}
      {promote.isError && <ErrorState title="Promotion was refused" message={promote.error.message} />}
      {abandon.isError && <ErrorState title="Could not abandon the experiment" message={abandon.error.message} />}
      {notice && (
        <p role="status" className="text-sm text-muted">
          {notice}
        </p>
      )}
    </div>
  )
}
