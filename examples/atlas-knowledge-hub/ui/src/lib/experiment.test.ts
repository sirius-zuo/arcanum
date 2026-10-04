import { describe, expect, it } from 'vitest'
import { readiness } from './experiment'
import type { ExperimentMetrics } from '../api/types'

const m = (sample_size: number, challenger: number, champion: number): ExperimentMetrics => ({
  sample_size,
  challenger_recall_at_5: challenger,
  champion_recall_at_5: champion,
  computed_at: 'now',
})

describe('readiness', () => {
  it('explains_that_12_golden_queries_cannot_reach_50', () => {
    const r = readiness(m(12, 0.99, 0.5))
    expect(r.ready).toBe(false)
    expect(r.sampleSize).toBe(12)
    expect(r.needed).toBe(50)
    expect(r.message).toMatch(/50/)
  })

  it('is_ready_with_enough_samples_and_a_clear_margin', () => {
    expect(readiness(m(60, 0.9, 0.8)).ready).toBe(true)
  })

  it('is_not_ready_when_the_margin_is_only_0_05', () => {
    const r = readiness(m(60, 0.82, 0.8))
    expect(r.ready).toBe(false)
    expect(r.message).toMatch(/0\.05/)
  })

  it('is_not_ready_at_an_exact_f32_style_boundary', () => {
    expect(readiness(m(60, 4 / 60, 1 / 60)).ready).toBe(false)
    expect(readiness(m(60, 0.8 + 0.05, 0.8)).ready).toBe(false)
  })

  it('names_the_champion_id_label_caveat', () => {
    expect(readiness(m(12, 0.9, 0.5)).message).toMatch(/champion chunk ids/)
  })

  it('treats_missing_metrics_as_sample_zero', () => {
    const r = readiness(null)
    expect(r.sampleSize).toBe(0)
    expect(r.ready).toBe(false)
  })
})
