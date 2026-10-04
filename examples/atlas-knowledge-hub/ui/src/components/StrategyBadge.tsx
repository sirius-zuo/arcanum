import { Braces, Network, Search, Sigma, TreePine } from 'lucide-react'
import type { LucideIcon } from 'lucide-react'
import { Chip } from './Chip'
import type { ChipTone } from './Chip'
import type { RetrievalStrategy } from '../api/types'

export const STRATEGIES: RetrievalStrategy[] = ['Vector', 'Bm25', 'ColBert', 'Raptor', 'Graph']

interface Look {
  label: string
  tone: ChipTone
  Icon: LucideIcon
  hint: string
}

const LOOKS: Record<RetrievalStrategy, Look> = {
  Vector: { label: 'Vector', tone: 'accent', Icon: Sigma, hint: 'Dense embedding similarity' },
  Bm25: { label: 'BM25', tone: 'partial', Icon: Search, hint: 'Lexical keyword match' },
  ColBert: { label: 'ColBERT', tone: 'uncited', Icon: Braces, hint: 'Token-level late interaction' },
  Raptor: { label: 'RAPTOR', tone: 'miscited', Icon: TreePine, hint: 'Hierarchical summary tree' },
  Graph: { label: 'Graph', tone: 'supported', Icon: Network, hint: 'Entity and relation graph' },
}

/** The API sends `Vector` on search and `vector` on context; accept either spelling. */
function normalize(name: string): RetrievalStrategy | null {
  const found = STRATEGIES.find((s) => s.toLowerCase() === name.toLowerCase())
  return found ?? null
}

export function StrategyBadge({ strategy }: { strategy: string }) {
  const key = normalize(strategy)
  if (!key) return <Chip>{strategy}</Chip>
  const { label, tone, Icon, hint } = LOOKS[key]
  return (
    <span title={hint}>
      <Chip tone={tone} icon={<Icon className="h-3 w-3" data-icon={Icon.displayName ?? label} aria-hidden="true" />}>
        {label}
      </Chip>
    </span>
  )
}
