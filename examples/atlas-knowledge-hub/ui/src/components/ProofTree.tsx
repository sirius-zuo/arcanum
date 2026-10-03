import { ChevronDown, ChevronRight, CircleDot, FileText, Link2, ListTree } from 'lucide-react'
import type { LucideIcon } from 'lucide-react'
import { useState } from 'react'
import type { EvidenceKind, ProofChain, ProofNode, RawSourceRef } from '../api/types'
import { Chip } from './Chip'
import { CopyButton } from './CopyButton'
import { SourceViewer } from './SourceViewer'

const KIND_ICON: Record<EvidenceKind, LucideIcon> = {
  Chunk: FileText,
  TreeNode: ListTree,
  Entity: CircleDot,
  Relation: Link2,
}

function Node({ node }: { node: ProofNode }) {
  const [open, setOpen] = useState(true)
  const [meta, setMeta] = useState(false)
  const Icon = KIND_ICON[node.kind]
  const hasChildren = node.children.length > 0
  const empty = Object.keys(node.metadata).length === 0
  return (
    <li>
      <div className="flex flex-wrap items-center gap-1.5 rounded-lg px-1 py-1 hover:bg-surface-2/60">
        {hasChildren ? (
          <button
            type="button"
            aria-expanded={open}
            aria-label={`${open ? 'Collapse' : 'Expand'} ${node.label}`}
            onClick={() => setOpen((o) => !o)}
            className="grid h-6 w-6 place-items-center rounded-md text-muted hover:bg-surface-2 hover:text-text"
          >
            {open ? <ChevronDown className="h-4 w-4" aria-hidden="true" /> : <ChevronRight className="h-4 w-4" aria-hidden="true" />}
          </button>
        ) : (
          <span className="h-6 w-6" aria-hidden="true" />
        )}
        <Icon className="h-4 w-4 shrink-0 text-accent" aria-hidden="true" />
        <span className="min-w-0 break-words text-sm font-medium">{node.label}</span>
        <Chip>{node.kind}</Chip>
        <span className="font-mono text-[11px] text-muted">{node.id}</span>
        <CopyButton text={node.id} label={`Copy id of ${node.label}`} />
        <button
          type="button"
          aria-expanded={meta}
          aria-label={`${meta ? 'Hide' : 'Show'} metadata for ${node.label}`}
          onClick={() => setMeta((m) => !m)}
          className="ml-auto rounded-md px-1.5 font-mono text-[11px] text-muted hover:bg-surface-2 hover:text-text"
        >
          {meta ? '{ } hide' : '{ } json'}
        </button>
      </div>
      {meta && (
        <pre className="ml-8 mt-1 max-h-60 overflow-auto rounded-lg border border-border bg-surface-2 p-2 font-mono text-[11px] leading-5">
          {empty ? 'no metadata' : JSON.stringify(node.metadata, null, 2)}
        </pre>
      )}
      {hasChildren && open && (
        <ul role="group" className="ml-3 border-l border-border pl-3">
          {node.children.map((c, i) => (
            <Node key={`${c.id}-${i}`} node={c} />
          ))}
        </ul>
      )}
    </li>
  )
}

function RawSource({ src }: { src: RawSourceRef }) {
  const [open, setOpen] = useState(false)
  return (
    <li className="rounded-lg border border-border bg-surface p-3">
      <div className="flex flex-wrap items-center gap-2 text-xs">
        <span className="break-all font-medium">{src.source_uri}</span>
        <Chip mono>v{src.version_num}</Chip>
        <span className="font-mono text-[11px] text-muted">
          bytes {src.offset_start}-{src.offset_end}
        </span>
        {src.page !== null && <span className="text-muted">page {src.page}</span>}
        {src.section && <span className="text-muted">{src.section}</span>}
        <button
          type="button"
          aria-expanded={open}
          aria-label={`${open ? 'Hide' : 'Show'} source for ${src.source_uri}`}
          onClick={() => setOpen((o) => !o)}
          className="ml-auto rounded-md border border-border px-2 py-0.5 text-[11px] text-muted hover:text-text"
        >
          {open ? 'Hide source' : 'Open source'}
        </button>
      </div>
      {open && (
        <div className="mt-2">
          <SourceViewer documentId={src.document_id} version={src.version_num} start={src.offset_start} end={src.offset_end} />
        </div>
      )}
    </li>
  )
}

/** The proof chain as a collapsible tree, then the raw source ranges it rests on. */
export function ProofTree({ chain }: { chain: ProofChain }) {
  return (
    <div className="space-y-6">
      <section aria-label="Proof tree">
        <ul>
          <Node node={chain.root} />
        </ul>
      </section>
      <section aria-labelledby="proof-sources">
        <h2 id="proof-sources" className="mb-2 text-sm font-semibold">
          Raw sources ({chain.raw_sources.length})
        </h2>
        {chain.raw_sources.length === 0 ? (
          <p className="text-sm text-muted">This proof has no raw source ranges.</p>
        ) : (
          <ul className="space-y-2">
            {chain.raw_sources.map((s, i) => (
              <RawSource key={`${s.document_id}-${s.offset_start}-${i}`} src={s} />
            ))}
          </ul>
        )}
      </section>
    </div>
  )
}
