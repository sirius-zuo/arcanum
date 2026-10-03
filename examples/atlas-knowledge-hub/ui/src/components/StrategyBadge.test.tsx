import { describe, expect, it } from 'vitest'
import { render } from '@testing-library/react'
import { StrategyBadge, STRATEGIES } from './StrategyBadge'

describe('StrategyBadge', () => {
  it('each_strategy_has_a_distinct_label_and_icon', () => {
    const labels = new Set<string>()
    const icons = new Set<string>()
    for (const s of STRATEGIES) {
      const { container, unmount } = render(<StrategyBadge strategy={s} />)
      labels.add(container.textContent ?? '')
      const svg = container.querySelector('svg')
      expect(svg).not.toBeNull()
      icons.add(svg?.getAttribute('class') ?? '')
      unmount()
    }
    expect(labels.size).toBe(STRATEGIES.length)
    expect(icons.size).toBe(STRATEGIES.length)
  })

  it('accepts_lowercase_names_from_the_context_api', () => {
    const { container } = render(<StrategyBadge strategy="bm25" />)
    expect(container.textContent).toContain('BM25')
  })
})
