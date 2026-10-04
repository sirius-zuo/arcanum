import { describe, expect, it } from 'vitest'
import { render, screen } from '@testing-library/react'
import { EvalView } from './EvalPanel'

describe('EvalPanel', () => {
  it('notes_the_precision_bound_and_shows_ranks', () => {
    render(
      <EvalView
        data={{
          report: { hit_rate_at_k: 0.9, mrr: 0.8, ndcg_at_k: 0.85, k: 5, num_queries: 2, precision_at_k: 0.18, recall_at_k: 0.9 },
          queries: [
            { query: 'q1', relevant_source_uri: 'a.md', first_relevant_rank: 1 },
            { query: 'q2', relevant_source_uri: 'b.md', first_relevant_rank: null },
          ],
        }}
      />,
    )
    expect(screen.getByText(/bounded by 1\/5/)).toBeInTheDocument()
    expect(screen.getByText('rank 1')).toBeInTheDocument()
    expect(screen.getByText('not in top 5')).toBeInTheDocument()
  })
})
