import { AlertTriangle, Check, ChevronDown, ChevronRight, X } from 'lucide-react'
import { useState } from 'react'
import type { Claim, ClaimEvidence, EvidenceVersionStatus, VerifiedSentence } from '../api/types'
import { Chip } from './Chip'
import type { ChipTone } from './Chip'
import { CopyButton } from './CopyButton'
import { SourceViewer } from './SourceViewer'
import { VerdictChip } from './VerdictChip'

const STATUS_TONE: Record<EvidenceVersionStatus, ChipTone> = {
  active: 'supported',
  superseded: 'partial',
  deleted: 'unsupported',
  unknown: 'neutral',
}

export const UNMATCHED_NOTICE = 'judge quote not found in source; showing the whole passage range'

function EvidenceItem({ ev }: { ev: ClaimEvidence }) {
  const [open, setOpen] = useState(true)
  return (
    <li className="rounded-lg border border-border bg-surface p-3">
      <div className="flex flex-wrap items-center gap-1.5">
        <Chip tone="accent" mono>
          {ev.ref_id}
        </Chip>
        <span className="break-all font-mono text-[11px] text-muted">{ev.source_uri}</span>
        <Chip mono>v{ev.version_num}</Chip>
        <Chip tone={STATUS_TONE[ev.version_status]}>{ev.version_status}</Chip>
        {ev.quote_matched ? (
          <Chip tone="supported" icon={<Check className="h-3 w-3" aria-hidden="true" />}>
            quote matched
          </Chip>
        ) : (
          <Chip tone="partial" icon={<AlertTriangle className="h-3 w-3" aria-hidden="true" />}>
            quote not matched
          </Chip>
        )}
      </div>
      <p className="mt-1.5 flex flex-wrap items-center gap-1 font-mono text-[11px] text-muted">
        <span>chunk</span>
        <span className="break-all text-text">{ev.chunk_id}</span>
        <CopyButton text={ev.chunk_id} label="Copy chunk id" />
      </p>
      {ev.quote !== '' && (
        <blockquote className="mt-2 border-l-2 border-border pl-3 text-[13px] italic leading-relaxed text-muted">{ev.quote}</blockquote>
      )}
      {!ev.quote_matched && (
        <p className="mt-2 flex items-start gap-1.5 text-xs text-vink-partial">
          <AlertTriangle className="mt-px h-3.5 w-3.5 shrink-0" aria-hidden="true" />
          <span>{UNMATCHED_NOTICE}</span>
        </p>
      )}
      <button
        type="button"
        aria-expanded={open}
        onClick={() => setOpen((o) => !o)}
        className="mt-2 inline-flex items-center gap-1 text-[11px] text-muted transition hover:text-text"
      >
        {open ? <ChevronDown className="h-3 w-3" aria-hidden="true" /> : <ChevronRight className="h-3 w-3" aria-hidden="true" />}
        {open ? 'Hide source' : 'Show source'}
      </button>
      {open && (
        <div className="mt-1.5">
          <SourceViewer documentId={ev.document_id} version={ev.version_num} start={ev.offset_start} end={ev.offset_end} />
        </div>
      )}
    </li>
  )
}

function ClaimBlock({ claim }: { claim: Claim }) {
  return (
    <li className="space-y-2">
      <div className="flex flex-wrap items-start gap-2">
        <p className="min-w-0 flex-1 text-sm">{claim.text}</p>
        {claim.supported ? (
          <Chip tone="supported" icon={<Check className="h-3 w-3" aria-hidden="true" />}>
            supported
          </Chip>
        ) : (
          <Chip tone="unsupported" icon={<X className="h-3 w-3" aria-hidden="true" />}>
            not supported
          </Chip>
        )}
      </div>
      {claim.evidence.length > 0 ? (
        <ul className="space-y-2 pl-3">
          {claim.evidence.map((ev, i) => (
            <EvidenceItem key={`${ev.chunk_id}-${i}`} ev={ev} />
          ))}
        </ul>
      ) : (
        <p className="pl-3 text-xs text-muted">No passage supports this claim.</p>
      )}
    </li>
  )
}

/** The evidence behind one verified sentence: verdict, citations, claims and the highlighted source. */
export function EvidenceView({ sentence }: { sentence: VerifiedSentence }) {
  return (
    <section aria-label="Evidence for the selected sentence" className="space-y-3 rounded-card border border-border bg-surface-2/40 p-4">
      <p className="text-sm font-medium leading-relaxed">{sentence.text}</p>
      <div className="flex flex-wrap items-center gap-x-4 gap-y-1.5 text-xs">
        <VerdictChip verdict={sentence.verdict} />
        <span className="inline-flex flex-wrap items-center gap-1.5">
          <span className="text-muted">cited</span>
          {sentence.cited.length === 0 ? <span className="text-muted">nothing</span> : sentence.cited.map((id) => <Chip key={id} tone="accent" mono>{id}</Chip>)}
        </span>
        {sentence.invalid_refs.length > 0 && (
          <span className="inline-flex flex-wrap items-center gap-1.5">
            <span className="text-muted">invalid refs</span>
            {sentence.invalid_refs.map((id) => (
              <Chip key={id} tone="partial" mono icon={<AlertTriangle className="h-3 w-3" aria-hidden="true" />}>
                {id}
              </Chip>
            ))}
          </span>
        )}
      </div>
      {sentence.claims.length === 0 ? (
        <p className="text-sm text-muted">
          {sentence.verdict === 'no_claim'
            ? 'This sentence makes no factual claim, so there is nothing to check.'
            : 'The judge reported no claims for this sentence.'}
        </p>
      ) : (
        <ul className="space-y-4">
          {sentence.claims.map((c, i) => (
            <ClaimBlock key={i} claim={c} />
          ))}
        </ul>
      )}
    </section>
  )
}
