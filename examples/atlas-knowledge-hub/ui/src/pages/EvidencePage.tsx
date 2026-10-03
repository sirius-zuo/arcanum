import { Search } from 'lucide-react'
import { useEffect, useRef, useState } from 'react'
import type { FormEvent } from 'react'
import { useSearchParams } from 'react-router-dom'
import { ApiError } from '../api/client'
import { getChunkProof, getEntityProof, getRelationProof, getTreeNodeProof, isUuid } from '../api/evidence'
import type { ProofChain } from '../api/types'
import { Card } from '../components/Card'
import { EmptyState } from '../components/EmptyState'
import { ErrorState } from '../components/ErrorState'
import { HowItWorks } from '../components/HowItWorks'
import { PageHeader } from '../components/PageHeader'
import { ProofTree } from '../components/ProofTree'
import { ROUTES, routeMeta } from '../routes'
import { useBootstrap } from '../state/bootstrap'

const meta = routeMeta('/evidence')
const step = String(ROUTES.indexOf(meta) + 1).padStart(2, '0')

const HOW_ARCANUM = ['GET /evidence/chunk/:id', 'GET /evidence/tree-node/:id', 'GET /evidence/entity/:id', 'GET /evidence/relation/:source/:type/:target']
const HOW_DEMO = ['GET /demo/documents/:id/versions/:n/text']

/** Tour hook: Task 19 wires this to the tour. */
const onEvidenceOpened = (): void => {}

type Kind = 'chunk' | 'tree-node' | 'entity' | 'relation'

const KINDS: { kind: Kind; label: string; idLabel: string }[] = [
  { kind: 'chunk', label: 'Chunk', idLabel: 'Chunk id' },
  { kind: 'tree-node', label: 'Tree node', idLabel: 'Tree node id' },
  { kind: 'entity', label: 'Entity', idLabel: 'Entity id' },
  { kind: 'relation', label: 'Relation', idLabel: '' },
]

interface Fields {
  id: string
  source: string
  type: string
  target: string
}

interface Failure {
  title: string
  message: string
}

function describe(e: unknown): Failure {
  if (e instanceof ApiError) {
    if (e.status === 404) {
      return { title: 'No evidence found', message: 'The server has no evidence for that id. It may belong to a different collection, or the document was re-ingested and its chunks replaced.' }
    }
    if (e.status === 503) return { title: 'Evidence is not available', message: e.message || 'evidence resolver not configured' }
    if (e.status === 400) return { title: 'The server rejected that id', message: e.message }
    return { title: 'Could not load the evidence', message: e.message }
  }
  return { title: 'Could not load the evidence', message: e instanceof Error ? e.message : String(e) }
}

function invalidUuid(kind: Kind, v: Fields): string | null {
  const checks: [string, string][] = kind === 'relation' ? [['Source entity id', v.source], ['Target entity id', v.target]] : [['That id', v.id]]
  const bad = checks.find(([, value]) => !isUuid(value))
  return bad ? bad[0] : null
}

export default function EvidencePage() {
  const { data: boot, client } = useBootstrap()
  const [params] = useSearchParams()
  const [kind, setKind] = useState<Kind>('chunk')
  const [f, setF] = useState<Fields>({ id: '', source: '', type: '', target: '' })
  const [chain, setChain] = useState<ProofChain | null>(null)
  const [fail, setFail] = useState<Failure | null>(null)
  const [busy, setBusy] = useState(false)
  const seq = useRef(0)

  const load = async (k: Kind, v: Fields) => {
    const bad = invalidUuid(k, v)
    if (bad) {
      setChain(null)
      setFail({
        title: 'Check the id',
        message: `${bad} is not a valid UUID. Evidence ids look like 123e4567-e89b-12d3-a456-426614174000; copy one from a Context passage, a verified sentence or a Search result.`,
      })
      return
    }
    if (k === 'relation' && v.type.trim() === '') {
      setChain(null)
      setFail({ title: 'Check the relation type', message: 'Enter the relation type, for example the label shown on a Graph edge.' })
      return
    }
    const mine = ++seq.current
    setBusy(true)
    setFail(null)
    try {
      const result =
        k === 'chunk'
          ? await getChunkProof(client, v.id)
          : k === 'tree-node'
            ? await getTreeNodeProof(client, v.id)
            : k === 'entity'
              ? await getEntityProof(client, v.id)
              : await getRelationProof(client, v.source, v.type, v.target)
      if (seq.current !== mine) return
      setChain(result)
      onEvidenceOpened()
    } catch (e) {
      if (seq.current !== mine) return
      setChain(null)
      setFail(describe(e))
    } finally {
      if (seq.current === mine) setBusy(false)
    }
  }

  // Deep links from other pages: ?chunk=<id> or ?entity=<id>, loaded once when bootstrap is ready.
  const ready = boot !== null
  useEffect(() => {
    if (!ready) return
    const chunk = params.get('chunk')
    const entity = params.get('entity')
    const fromUrl: [Kind, string] | null = chunk ? ['chunk', chunk] : entity ? ['entity', entity] : null
    if (!fromUrl) return
    setKind(fromUrl[0])
    const next = { id: fromUrl[1], source: '', type: '', target: '' }
    setF(next)
    void load(fromUrl[0], next)
    // Once per mount: later edits to the form are the user's, not the URL's.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [ready])

  if (!boot) return null

  const submit = (e: FormEvent) => {
    e.preventDefault()
    void load(kind, f)
  }
  const input = 'h-10 w-full rounded-lg border border-border bg-surface px-3 font-mono text-sm outline-none focus:border-accent'
  const current = KINDS.find((k) => k.kind === kind) ?? KINDS[0]
  const off = !boot.features.evidence

  return (
    <>
      <PageHeader eyebrow={`${step} / ${meta.label}`} title={meta.label} description={meta.blurb} actions={<HowItWorks arcanum={HOW_ARCANUM} demo={HOW_DEMO} />} />

      {off && (
        <div className="mb-6">
          <ErrorState title="Evidence is not available" message="evidence resolver not configured. This server has no chunk registry, so it cannot resolve proof chains." />
        </div>
      )}

      <form onSubmit={submit} className="mb-8">
        <Card className="space-y-4 p-4">
          <div role="tablist" aria-label="What to look up" className="inline-flex rounded-lg border border-border bg-surface-2 p-0.5">
            {KINDS.map((k) => (
              <button
                key={k.kind}
                type="button"
                role="tab"
                aria-selected={kind === k.kind}
                onClick={() => setKind(k.kind)}
                className={`h-7 rounded-md px-3 text-xs transition ${kind === k.kind ? 'bg-surface font-medium shadow-soft' : 'text-muted hover:text-text'}`}
              >
                {k.label}
              </button>
            ))}
          </div>

          {kind === 'relation' ? (
            <div className="grid gap-3 md:grid-cols-3">
              <div>
                <label htmlFor="ev-source" className="mb-1 block text-xs font-medium text-muted">
                  Source entity id
                </label>
                <input id="ev-source" value={f.source} onChange={(e) => setF({ ...f, source: e.target.value })} className={input} placeholder="uuid" spellCheck={false} />
              </div>
              <div>
                <label htmlFor="ev-type" className="mb-1 block text-xs font-medium text-muted">
                  Relation type
                </label>
                <input id="ev-type" value={f.type} onChange={(e) => setF({ ...f, type: e.target.value })} className={input} placeholder="for example works_on" spellCheck={false} />
              </div>
              <div>
                <label htmlFor="ev-target" className="mb-1 block text-xs font-medium text-muted">
                  Target entity id
                </label>
                <input id="ev-target" value={f.target} onChange={(e) => setF({ ...f, target: e.target.value })} className={input} placeholder="uuid" spellCheck={false} />
              </div>
            </div>
          ) : (
            <div>
              <label htmlFor="ev-id" className="mb-1 block text-xs font-medium text-muted">
                {current.idLabel}
              </label>
              <input id="ev-id" value={f.id} onChange={(e) => setF({ ...f, id: e.target.value })} className={input} placeholder="123e4567-e89b-12d3-a456-426614174000" spellCheck={false} />
            </div>
          )}

          <button
            type="submit"
            disabled={busy || off}
            className="inline-flex h-10 items-center gap-2 rounded-lg bg-accent px-4 text-sm font-medium text-accent-fg transition hover:opacity-90 disabled:opacity-50"
          >
            <Search className="h-4 w-4" aria-hidden="true" />
            {busy ? 'Looking up...' : 'Look up'}
          </button>
        </Card>
      </form>

      <div aria-live="polite">
        {fail && <ErrorState title={fail.title} message={fail.message} />}
        {!fail && !chain && !busy && (
          <EmptyState
            icon={meta.icon}
            title="Trace a fact to its source"
            description="Paste a chunk, tree node or entity id, or open a chunk from Search, Context or a verified sentence. You get the proof chain and the exact bytes of the source it rests on."
          />
        )}
        {chain && <ProofTree chain={chain} />}
      </div>
    </>
  )
}
