import { Search as SearchIcon } from 'lucide-react'
import { useState } from 'react'
import type { FormEvent } from 'react'
import { Link } from 'react-router-dom'
import { useSamples } from '../api/library'
import { useSearch } from '../api/search'
import { Chip } from '../components/Chip'
import { EmptyState } from '../components/EmptyState'
import { ErrorState } from '../components/ErrorState'
import { HowItWorks } from '../components/HowItWorks'
import { PageHeader } from '../components/PageHeader'
import { ResultCard } from '../components/ResultCard'
import { Skeleton } from '../components/Skeleton'
import { ROUTES, routeMeta } from '../routes'
import { useBootstrap } from '../state/bootstrap'

const meta = routeMeta('/search')
const step = String(ROUTES.indexOf(meta) + 1).padStart(2, '0')

const HOW_ARCANUM = ['POST /api/v1/search']
const HOW_DEMO: string[] = []

/** Tour hook point: Task 19 wires this to the tour. */
const onSearched = (): void => {}

const libraryLink = (
  <Link to="/library" className="text-sm font-medium text-accent underline-offset-2 hover:underline">
    Open the Library
  </Link>
)

export default function SearchPage() {
  const { data: boot } = useBootstrap()
  const samples = useSamples()
  const search = useSearch()
  const [query, setQuery] = useState('')
  const [topK, setTopK] = useState(5)
  const [asked, setAsked] = useState('')

  const suggestions = (samples.data?.golden ?? []).slice(0, 4).map((g) => g.query)

  const run = (q: string) => {
    const text = q.trim()
    if (!text || !boot) return
    setAsked(text)
    search.mutate(
      { query: text, collection_id: boot.collection, top_k: topK },
      { onSuccess: () => onSearched() },
    )
  }

  const onSubmit = (e: FormEvent) => {
    e.preventDefault()
    run(query)
  }

  const err = search.error

  return (
    <>
      <PageHeader
        eyebrow={`${step} / ${meta.label}`}
        title={meta.label}
        description={meta.blurb}
        actions={<HowItWorks arcanum={HOW_ARCANUM} demo={HOW_DEMO} />}
      />

      <form onSubmit={onSubmit} className="mb-3 flex flex-wrap items-end gap-3">
        <div className="min-w-[16rem] flex-1">
          <label htmlFor="search-query" className="mb-1 block text-xs font-medium text-muted">
            Query
          </label>
          <input
            id="search-query"
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            placeholder="Ask about the sample corpus"
            className="h-10 w-full rounded-lg border border-border bg-surface px-3 text-sm outline-none focus:border-accent"
          />
        </div>
        <div>
          <label htmlFor="search-topk" className="mb-1 block text-xs font-medium text-muted">
            Results (top_k)
          </label>
          <input
            id="search-topk"
            type="number"
            min={1}
            max={50}
            value={topK}
            onChange={(e) => setTopK(Math.min(50, Math.max(1, Number(e.target.value) || 1)))}
            className="h-10 w-24 rounded-lg border border-border bg-surface px-3 font-mono text-sm outline-none focus:border-accent"
          />
        </div>
        <button
          type="submit"
          disabled={!query.trim() || !boot || search.isPending}
          className="inline-flex h-10 items-center gap-2 rounded-lg bg-accent px-4 text-sm font-medium text-accent-fg transition hover:opacity-90 disabled:opacity-50"
        >
          <SearchIcon className="h-4 w-4" aria-hidden="true" />
          Search
        </button>
      </form>

      <div className="mb-6 flex flex-wrap items-center gap-2">
        {boot && (
          <Chip tone="accent" mono>
            mode: {boot.orchestration_mode}
          </Chip>
        )}
        <span className="text-xs text-muted">Fusion keeps one chunk per document.</span>
        {suggestions.map((s) => (
          <button
            key={s}
            type="button"
            onClick={() => {
              setQuery(s)
              run(s)
            }}
            className="rounded-full border border-border bg-surface px-3 py-1 text-xs text-muted transition hover:text-text hover:shadow-soft"
          >
            {s}
          </button>
        ))}
      </div>

      <div aria-live="polite">
        {search.isPending && (
          <div className="space-y-3" aria-label="Searching">
            {[0, 1, 2].map((i) => (
              <Skeleton key={i} className="h-28 w-full" />
            ))}
          </div>
        )}
        {err && !search.isPending && (
          <ErrorState
            title="Search failed"
            message={err.message}
          />
        )}
        {search.data && !search.isPending && search.data.chunks.length === 0 && (
          <EmptyState
            icon={SearchIcon}
            title="No results"
            description="Nothing matched. If the collection is empty, load the sample corpus first."
            action={libraryLink}
          />
        )}
        {search.data && !search.isPending && search.data.chunks.length > 0 && (
          <ol className="space-y-3">
            {search.data.chunks.map((c) => (
              <li key={c.indexed_chunk.chunk.id}>
                <ResultCard chunk={c} query={asked} />
              </li>
            ))}
          </ol>
        )}
        {!search.isPending && !search.data && !err && (
          <EmptyState
            icon={SearchIcon}
            title="Run a search"
            description="Type a question or pick an example. Each result shows which retrieval strategy surfaced it and where it came from."
          />
        )}
      </div>
    </>
  )
}
