import type { ExperimentMetrics } from '../api/types'

/** Mirrors `ExperimentService::update_metrics` in arcanum-engine. */
export const MIN_SAMPLES = 50
export const MIN_MARGIN = 0.05

export interface Readiness {
  sampleSize: number
  needed: 50
  ready: boolean
  message: string
}

export function readiness(metrics: ExperimentMetrics | null | undefined): Readiness {
  if (!metrics) {
    return {
      sampleSize: 0,
      needed: MIN_SAMPLES,
      ready: false,
      message: `Not evaluated yet. Promotion needs an evaluation with at least ${MIN_SAMPLES} labeled queries where the challenger recall@5 beats the champion by more than ${MIN_MARGIN}.`,
    }
  }
  const sampleSize = metrics.sample_size
  if (sampleSize < MIN_SAMPLES) {
    return {
      sampleSize,
      needed: MIN_SAMPLES,
      ready: false,
      message: `This evaluation used ${sampleSize} queries and promotion needs at least ${MIN_SAMPLES}. The golden set is small by design, so this demo cannot reach the threshold: the server will refuse to promote.`,
    }
  }
  const margin = metrics.challenger_recall_at_5 - metrics.champion_recall_at_5
  if (!(metrics.challenger_recall_at_5 > metrics.champion_recall_at_5 + MIN_MARGIN)) {
    return {
      sampleSize,
      needed: MIN_SAMPLES,
      ready: false,
      message: `Enough queries (${sampleSize}), but the challenger leads by ${margin.toFixed(2)} and it must lead by more than ${MIN_MARGIN.toFixed(2)} recall@5.`,
    }
  }
  return {
    sampleSize,
    needed: MIN_SAMPLES,
    ready: true,
    message: `Ready to promote: ${sampleSize} queries and the challenger leads by ${margin.toFixed(2)} recall@5.`,
  }
}
