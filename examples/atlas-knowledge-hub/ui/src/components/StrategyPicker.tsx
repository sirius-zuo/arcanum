import type { ChunkStrategyConfig } from '../api/types'

export interface StrategyPreset {
  id: string
  label: string
  config: ChunkStrategyConfig
}

/**
 * Names and params the server's chunk registry accepts (arcanum-ingestion `default_registry`). Note the README
 * table lists `structure_aware`, but the registered name is `structure`.
 */
export const PRESETS: StrategyPreset[] = [
  { id: 'fixed-512', label: 'fixed, 512 chars, overlap 64', config: { strategy: 'fixed', params: { chunk_size: 512, overlap: 64 } } },
  { id: 'fixed-256', label: 'fixed, 256 chars, overlap 32', config: { strategy: 'fixed', params: { chunk_size: 256, overlap: 32 } } },
  { id: 'semantic-800', label: 'semantic, max 800 chars', config: { strategy: 'semantic', params: { max_chars: 800 } } },
  { id: 'semantic-400', label: 'semantic, max 400 chars', config: { strategy: 'semantic', params: { max_chars: 400 } } },
  { id: 'propositional', label: 'propositional', config: { strategy: 'propositional', params: {} } },
  { id: 'hierarchical', label: 'hierarchical', config: { strategy: 'hierarchical', params: {} } },
  { id: 'structure-2000', label: 'structure, max 2000 chars', config: { strategy: 'structure', params: { max_chunk_chars: 2000 } } },
]

export const MAX_STRATEGIES = 3

export function presetsById(ids: string[]): ChunkStrategyConfig[] {
  return ids.flatMap((id) => PRESETS.find((p) => p.id === id)?.config ?? [])
}

interface StrategyPickerProps {
  selected: string[]
  onChange: (ids: string[]) => void
}

export function StrategyPicker({ selected, onChange }: StrategyPickerProps) {
  const full = selected.length >= MAX_STRATEGIES
  const toggle = (id: string) =>
    onChange(selected.includes(id) ? selected.filter((s) => s !== id) : full ? selected : [...selected, id])
  return (
    <fieldset>
      <legend className="mb-1.5 text-xs font-medium text-muted">Strategies (up to {MAX_STRATEGIES})</legend>
      <div className="flex flex-wrap gap-2">
        {PRESETS.map((p) => {
          const on = selected.includes(p.id)
          return (
            <label
              key={p.id}
              className={`flex cursor-pointer items-center gap-2 rounded-lg border px-2.5 py-1.5 text-xs transition ${
                on ? 'border-accent bg-accent/10' : 'border-border bg-surface hover:border-accent/40'
              } ${!on && full ? 'cursor-not-allowed opacity-50' : ''}`}
            >
              <input type="checkbox" checked={on} disabled={!on && full} onChange={() => toggle(p.id)} className="accent-[rgb(var(--accent))]" />
              <span className="font-mono">{p.label}</span>
            </label>
          )
        })}
      </div>
    </fieldset>
  )
}
