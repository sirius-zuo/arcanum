import clsx from 'clsx'
import { Play } from 'lucide-react'
import { useMemo, useState } from 'react'
import { useMutation } from '@tanstack/react-query'
import { inspectChunking } from '../api/lab'
import type { CorpusDoc } from '../api/lab'
import { useDocumentText, useLibrary, useSamples } from '../api/library'
import type { DocumentText, InspectResult, Library, LibraryDocument } from '../api/types'
import { BenchmarkPanel, describeStrategy } from '../components/BenchmarkPanel'
import { Card } from '../components/Card'
import { Chip } from '../components/Chip'
import { ChunkSpans } from '../components/ChunkSpans'
import { EmptyState } from '../components/EmptyState'
import { ErrorState } from '../components/ErrorState'
import { EvalPanel } from '../components/EvalPanel'
import { ExperimentPanel } from '../components/ExperimentPanel'
import { HowItWorks } from '../components/HowItWorks'
import { PageHeader } from '../components/PageHeader'
import { StrategyPicker, presetsById } from '../components/StrategyPicker'
import { ROUTES, routeMeta } from '../routes'
import { useBootstrap } from '../state/bootstrap'

const meta = routeMeta('/lab')
const step = String(ROUTES.indexOf(meta) + 1).padStart(2, '0')

const HOW_ARCANUM = [
  'POST /api/v1/chunk/inspect',
  'POST /api/v1/chunk/benchmark',
  'POST|GET|DELETE /api/v1/collections/{id}/experiments',
  'POST /api/v1/collections/{id}/experiments/{id}/eval',
  'POST /api/v1/collections/{id}/experiments/{id}/promote',
  'POST /api/v1/search',
]
const HOW_DEMO = ['POST /demo/eval', 'GET /demo/documents/{id}/versions/{n}/text', 'GET /demo/samples']

type Tab = 'chunking' | 'experiments' | 'evaluation'
const TABS: { id: Tab; label: string }[] = [
  { id: 'chunking', label: 'Chunking' },
  { id: 'experiments', label: 'Experiments' },
  { id: 'evaluation', label: 'Evaluation' },
]

function latestActive(doc: LibraryDocument) {
  return doc.versions.filter((v) => v.status === 'Active').sort((a, b) => b.version_num - a.version_num)[0] ?? null
}

function ChunkingTab({ library }: { library: Library }) {
  const { client } = useBootstrap()
  const samples = useSamples()
  const docs = library.documents.filter((d) => latestActive(d) !== null)
  const [docId, setDocId] = useState<string>(docs[0]?.document_id ?? '')
  const [picked, setPicked] = useState<string[]>(['fixed-512', 'semantic-800'])
  const doc = docs.find((d) => d.document_id === docId) ?? docs[0]
  const version = doc ? latestActive(doc) : null
  const text = useDocumentText(doc?.document_id ?? '', version?.version_num ?? 0)
  const loaded: DocumentText | undefined = doc && version ? text.data : undefined

  const compare = useMutation<InspectResult[], Error, { text: string; ids: string[] }>({
    mutationFn: ({ text: t, ids }) => inspectChunking(client, t, presetsById(ids)),
  })
  // Results belong to the text they were computed for.
  const [shown, setShown] = useState<{ docId: string; text: string; results: InspectResult[] } | null>(null)

  const strategies = useMemo(() => presetsById(picked), [picked])
  const loadCorpus = async (): Promise<CorpusDoc[]> => {
    const out: CorpusDoc[] = []
    for (const d of docs) {
      const v = latestActive(d)
      if (!v) continue
      const t = await client.get<DocumentText>(`/demo/documents/${encodeURIComponent(d.document_id)}/versions/${v.version_num}/text`)
      out.push({ source_uri: d.source_uri, text: t.text })
    }
    return out
  }

  if (docs.length === 0) {
    return <EmptyState icon={meta.icon} title="No documents yet" description="Load the sample corpus from Overview or Library, then compare how strategies slice a document." />
  }

  return (
    <div className="space-y-4">
      <Card className="space-y-3 p-4">
        <label className="block text-xs font-medium text-muted">
          Sample document
          <select
            value={doc?.document_id ?? ''}
            onChange={(e) => {
              setDocId(e.target.value)
              setShown(null)
            }}
            className="mt-1 block h-9 w-full max-w-md rounded-lg border border-border bg-surface px-2 text-sm text-text"
          >
            {docs.map((d) => (
              <option key={d.document_id} value={d.document_id}>
                {d.source_uri}
              </option>
            ))}
          </select>
        </label>
        <StrategyPicker selected={picked} onChange={setPicked} />
        <button
          type="button"
          disabled={!loaded || picked.length === 0 || compare.isPending}
          onClick={() => {
            if (!loaded) return
            const docKey = loaded.document_id
            compare.mutate({ text: loaded.text, ids: picked }, { onSuccess: (results) => setShown({ docId: docKey, text: loaded.text, results }) })
          }}
          className="inline-flex h-9 items-center gap-2 rounded-lg bg-accent px-3 text-sm font-medium text-accent-fg transition hover:opacity-90 disabled:opacity-50"
        >
          <Play className="h-4 w-4" aria-hidden="true" />
          {compare.isPending ? 'Chunking...' : 'Compare strategies'}
        </button>
        {text.isError && <ErrorState title="Could not load the document text" message={text.error.message} />}
      </Card>

      {samples.isError && <ErrorState title="Could not load the golden queries" message={samples.error.message} />}
      {compare.isError && <ErrorState title="Chunking failed" message={compare.error.message} />}

      {shown && shown.docId === doc?.document_id && (
        <div className={clsx('grid gap-4', shown.results.length > 1 && 'lg:grid-cols-2', shown.results.length > 2 && 'xl:grid-cols-3')}>
          {shown.results.map((r, i) => (
            <Card key={i} className="min-w-0 p-4">
              <div className="mb-2 flex flex-wrap items-center gap-2">
                <h3 className="font-mono text-xs font-semibold">{describeStrategy(r.strategy)}</h3>
                <Chip mono>{r.total_chunks} chunks</Chip>
                <Chip mono>~{r.mean_tokens.toFixed(0)} tok mean</Chip>
              </div>
              <ChunkSpans text={shown.text} chunks={r.chunks} />
            </Card>
          ))}
        </div>
      )}

      <BenchmarkPanel strategies={strategies} golden={samples.data?.golden ?? []} loadCorpus={loadCorpus} />
    </div>
  )
}

export default function LabPage() {
  const { data: boot } = useBootstrap()
  const library = useLibrary()
  const samples = useSamples()
  const [tab, setTab] = useState<Tab>('chunking')
  if (!boot) return null

  return (
    <>
      <PageHeader eyebrow={`${step} / ${meta.label}`} title={meta.label} description={meta.blurb} actions={<HowItWorks arcanum={HOW_ARCANUM} demo={HOW_DEMO} />} />
      <div role="tablist" aria-label="Lab sections" className="mb-5 inline-flex rounded-lg border border-border bg-surface-2 p-0.5">
        {TABS.map((t) => (
          <button
            key={t.id}
            type="button"
            role="tab"
            aria-selected={t.id === tab}
            onClick={() => setTab(t.id)}
            className={clsx('h-8 rounded-md px-4 text-sm transition', t.id === tab ? 'bg-surface font-medium shadow-soft' : 'text-muted hover:text-text')}
          >
            {t.label}
          </button>
        ))}
      </div>
      <div role="tabpanel">
        {tab === 'chunking' &&
          (library.isError ? (
            <ErrorState title="Could not load the library" message={library.error.message} />
          ) : library.data ? (
            <ChunkingTab library={library.data} />
          ) : (
            <p className="text-sm text-muted">Loading the library...</p>
          ))}
        {tab === 'experiments' && <ExperimentPanel collection={boot.collection} golden={samples.data?.golden ?? []} samplesError={samples.isError ? samples.error.message : null} />}
        {tab === 'evaluation' && <EvalPanel />}
      </div>
    </>
  )
}
