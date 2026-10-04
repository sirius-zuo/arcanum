import type { Verdict, VerdictCounts } from '../api/types'

export type { Verdict }

export type VerdictIcon = 'check' | 'alert' | 'x' | 'swap' | 'link' | 'minus'

export interface VerdictMeta {
  label: string
  icon: VerdictIcon
  /** Tailwind color key under `verdict`, backed by the `--v-*` CSS variables. */
  tone: string
}

const META: Record<Verdict, VerdictMeta> = {
  supported: { label: 'Supported', icon: 'check', tone: 'supported' },
  partial: { label: 'Partially supported', icon: 'alert', tone: 'partial' },
  unsupported: { label: 'Unsupported', icon: 'x', tone: 'unsupported' },
  miscited: { label: 'Miscited', icon: 'swap', tone: 'miscited' },
  uncited_supported: { label: 'Supported, uncited', icon: 'link', tone: 'uncited' },
  no_claim: { label: 'No claim', icon: 'minus', tone: 'noclaim' },
}

export function verdictMeta(v: Verdict): VerdictMeta {
  return META[v]
}

/** Claims that pass: supported, or supported by a passage the sentence did not cite. */
export function summarize(counts: VerdictCounts): { total: number; passing: number; label: string } {
  const total = Object.entries(counts).reduce((n, [v, c]) => (v === 'no_claim' ? n : n + c), 0)
  const passing = counts.supported + counts.uncited_supported
  const label = total === 0 ? 'Nothing to verify' : `${passing} of ${total} claims supported`
  return { total, passing, label }
}
