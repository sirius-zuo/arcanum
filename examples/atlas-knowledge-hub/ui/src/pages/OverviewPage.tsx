import {
  Command,
  Compass,
  Sparkles,
  Activity,
  BookOpen,
  Boxes,
  FileSearch,
  FlaskConical,
  GitFork,
  History,
  Layers,
  LineChart,
  MessageSquareText,
  Plug,
  Scissors,
  Search,
  ShieldCheck,
} from 'lucide-react'
import type { LucideIcon } from 'lucide-react'
import { useTerminalCount } from '../api/ingest'
import { useApplyUpdate, useHealth, useLibrary, useLoadSamples } from '../api/library'
import type { Bootstrap, DemoHealth, Library } from '../api/types'
import { CapabilityCard } from '../components/CapabilityCard'
import type { CapabilityStatus } from '../components/CapabilityCard'
import { Card } from '../components/Card'
import { ErrorState } from '../components/ErrorState'
import { HealthChecklist } from '../components/HealthChecklist'
import { HowItWorks } from '../components/HowItWorks'
import { LoadCorpusButton } from '../components/LoadCorpusButton'
import { Skeleton } from '../components/Skeleton'
import { useBootstrap } from '../state/bootstrap'
import { useOperations } from '../state/operations'
import { useTour } from '../state/tour'

interface Capability {
  title: string
  description: string
  to: string
  icon: LucideIcon
  status: (ctx: StatusContext) => CapabilityStatus
}

interface StatusContext {
  boot: Bootstrap | null
  health: DemoHealth | undefined
  library: Library | undefined
}

function docCount(ctx: StatusContext): number {
  return ctx.library?.documents.length ?? 0
}

function featureStatus(on: boolean | undefined, label: string): CapabilityStatus {
  if (on === undefined) return { label: 'Checking' }
  return on ? { label, tone: 'supported' } : { label: 'Disabled', tone: 'noclaim' }
}

const loaded = (ctx: StatusContext, fallback: string): CapabilityStatus =>
  docCount(ctx) > 0 ? { label: `${docCount(ctx)} documents`, tone: 'accent', mono: true } : { label: fallback }

/** One card per row of the capability matrix (spec 9), grouped by the page that shows it. */
const CAPABILITIES: Capability[] = [
  { title: 'Durable ingestion', description: 'Idempotent operations through the full pipeline, with live progress events.', to: '/library', icon: BookOpen, status: (c) => loaded(c, 'Load the corpus') },
  { title: 'Versions and deletion', description: 'Every re-ingest is a new version; the old one is superseded. Sources can be deleted.', to: '/library', icon: History, status: (c) => {
      const versions = c.library?.documents.reduce((n, d) => n + d.versions.length, 0) ?? 0
      return versions > docCount(c) ? { label: 'Superseded versions', tone: 'partial' } : { label: 'Apply the update' }
    } },
  { title: 'Chunking and preprocessing', description: 'Per-backend chunkers; compare strategies side by side on a real document.', to: '/lab', icon: Scissors, status: () => ({ label: 'Inspect chunks' }) },
  { title: 'Multi-strategy retrieval', description: 'Vector, BM25, graph and RAPTOR retrieval fused by reciprocal rank, with the winner shown.', to: '/search', icon: Search, status: (c) => ({ label: c.boot ? `Mode: ${c.boot.orchestration_mode}` : 'Checking', mono: true, tone: 'accent' }) },
  { title: 'Context packing', description: 'Token-budgeted, numbered passages rendered as XML, Markdown or a list.', to: '/context', icon: Layers, status: (c) => featureStatus(c.boot?.features.context, 'Enabled') },
  { title: 'Grounded generation', description: 'Streaming answers with clickable citations back to the passages.', to: '/ask', icon: MessageSquareText, status: (c) => {
      const g = c.boot?.generators.find((x) => x.is_default)
      return c.boot?.features.generate ? { label: g ? g.model : 'Enabled', tone: 'supported', mono: true } : featureStatus(c.boot?.features.generate, 'Enabled')
    } },
  { title: 'Verification', description: 'Sentence-level verdicts from a judge, with strict citation checking.', to: '/verify', icon: ShieldCheck, status: (c) => (c.boot?.features.verify ? { label: c.boot.judge ? `Judge: ${c.boot.judge}` : 'No judge', tone: c.boot.judge ? 'supported' : 'partial', mono: true } : featureStatus(c.boot?.features.verify, 'Enabled')) },
  { title: 'Evidence and provenance', description: 'Trace any id to the exact bytes of the source version it came from.', to: '/evidence', icon: FileSearch, status: (c) => featureStatus(c.boot?.features.evidence, 'Enabled') },
  { title: 'Knowledge graph', description: 'Entities and relations extracted from the corpus, each linked to evidence.', to: '/graph', icon: GitFork, status: (c) => loaded(c, 'Fills after load') },
  { title: 'Chunk experiments', description: 'Benchmark chunking strategies and run shadow experiments with the sample-size caveat.', to: '/lab', icon: FlaskConical, status: (c) => featureStatus(c.boot?.features.experiments, 'Enabled') },
  { title: 'Retrieval evaluation', description: 'Hit rate, MRR and NDCG against a golden set of twelve questions.', to: '/lab', icon: LineChart, status: (c) => loaded(c, 'Needs a corpus') },
  { title: 'Auth, roles and audit', description: 'Scoped API keys, key rotation and an audit log of every call. Retention GC is explained.', to: '/admin', icon: Boxes, status: (c) => (c.boot?.features.gc ? { label: 'GC available', tone: 'supported' } : { label: 'GC needs Postgres', tone: 'noclaim' }) },
  { title: 'Observability', description: 'Metrics, circuit breakers, readiness and a live event feed.', to: '/admin', icon: Activity, status: (c) => (c.health ? (c.health.ready ? { label: 'Ready', tone: 'supported' } : { label: 'Needs attention', tone: 'unsupported' }) : { label: 'Checking' }) },
  { title: 'MCP server', description: 'Seven tools for any MCP client, with copyable config and curl snippets.', to: '/connect', icon: Plug, status: (c) => ({ label: c.boot ? `Port ${c.boot.mcp_port}` : 'Checking', mono: true, tone: 'accent' }) },
]

const HOW_ARCANUM = [
  'POST /api/v1/ingestion-operations (the same durable pipeline the load button drives)',
]
const HOW_DEMO = [
  'GET /demo/bootstrap',
  'GET /demo/health',
  'GET /demo/samples',
  'GET /demo/library',
  'POST /demo/samples/load',
  'POST /demo/samples/apply-update',
]

export default function OverviewPage() {
  const { data: boot } = useBootstrap()
  const health = useHealth()
  const library = useLibrary()
  const load = useLoadSamples()
  const update = useApplyUpdate()
  const { ops } = useOperations()
  const tour = useTour()
  const progress = useTerminalCount(ops.map((o) => o.operation_id))
  const ctx: StatusContext = { boot, health: health.data, library: library.data }

  return (
    <div className="space-y-10">
      <section className="relative overflow-hidden rounded-card border border-border bg-surface px-8 py-10 shadow-soft animate-rise">
        <div className="atlas-dots pointer-events-none absolute inset-y-0 right-0 w-1/2 opacity-70" aria-hidden="true" />
        <div className="relative max-w-2xl">
          <p className="mb-3 flex items-center gap-2 font-mono text-[11px] uppercase tracking-[0.14em] text-accent">
            <span className="h-px w-6 bg-accent/60" aria-hidden="true" />
            Atlas Knowledge Hub
          </p>
          <h1 className="text-[34px] font-semibold leading-[1.15] tracking-tight">Answers you can trace to the exact source.</h1>
          <p className="mt-4 text-[15px] leading-relaxed text-muted">
            Atlas runs the whole Arcanum stack against the documents of a fictional company: ingest, retrieve, generate with citations, verify every sentence and follow any claim back to the bytes it came from.
          </p>
          <div className="mt-7 flex flex-wrap items-start gap-4">
            <LoadCorpusButton health={health.data} loading={load.isPending} trackedCount={ops.length} doneCount={progress.done} onLoad={() => load.mutate()} />
            <button
              type="button"
              onClick={tour.start}
              disabled={tour.steps.length === 0}
              className="inline-flex h-10 items-center gap-2 rounded-lg border border-border bg-surface px-4 text-sm font-medium transition hover:shadow-soft disabled:cursor-not-allowed disabled:opacity-50"
            >
              <Compass className="h-4 w-4" aria-hidden="true" />
              Start the guided tour
            </button>
            <button
              type="button"
              onClick={() => update.mutate()}
              disabled={!health.data?.ready || update.isPending || docCount(ctx) === 0}
              title={docCount(ctx) === 0 ? 'Load the sample corpus first' : undefined}
              className="inline-flex h-10 items-center gap-2 rounded-lg border border-border bg-surface px-4 text-sm font-medium transition hover:shadow-soft disabled:cursor-not-allowed disabled:opacity-50"
            >
              <Sparkles className="h-4 w-4" aria-hidden="true" />
              Apply policy update
            </button>
          </div>
          {load.isError && <p role="alert" className="mt-3 text-sm text-v-unsupported">{load.error.message}</p>}
          {update.isError && <p role="alert" className="mt-3 text-sm text-v-unsupported">{update.error.message}</p>}
          <p className="mt-6 flex items-center gap-1.5 text-xs text-muted">
            <Command className="h-3 w-3" aria-hidden="true" />
            Press <kbd className="rounded border border-border px-1 font-mono text-[10px]">Ctrl</kbd>
            <kbd className="rounded border border-border px-1 font-mono text-[10px]">K</kbd> to jump anywhere.
          </p>
        </div>
      </section>

      <section aria-labelledby="health-title" className="animate-rise">
        <div className="mb-3 flex items-center justify-between gap-3">
          <h2 id="health-title" className="text-base font-semibold">
            Health
          </h2>
          <HowItWorks arcanum={HOW_ARCANUM} demo={HOW_DEMO} />
        </div>
        <Card className="px-5 py-2">
          {health.isError ? (
            <div className="py-3">
              <ErrorState title="Health could not be read" message={health.error.message} fix="cargo run   # in examples/atlas-knowledge-hub" />
            </div>
          ) : health.data ? (
            <HealthChecklist checks={health.data.checks} />
          ) : (
            <div className="space-y-4 py-4" aria-busy="true">
              {[0, 1, 2, 3].map((i) => (
                <Skeleton key={i} className="h-5 w-full" />
              ))}
            </div>
          )}
        </Card>
      </section>

      <section aria-labelledby="map-title" className="animate-rise">
        <h2 id="map-title" className="text-base font-semibold">
          What Atlas demonstrates
        </h2>
        <p className="mb-4 mt-1 text-sm text-muted">Every card links to the page where that capability runs for real.</p>
        <div className="grid gap-4 sm:grid-cols-2 xl:grid-cols-3">
          {CAPABILITIES.map((c) => (
            <CapabilityCard key={c.title} title={c.title} description={c.description} to={c.to} icon={c.icon} status={c.status(ctx)} />
          ))}
        </div>
      </section>
    </div>
  )
}
