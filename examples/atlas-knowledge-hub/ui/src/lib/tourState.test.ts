import { describe, expect, it } from 'vitest'
import { INITIAL_TOUR, libraryEvents, staleLibraryEvents, nextIncomplete, parseTour, reduceTour, serializeTour } from './tourState'
import type { TourState } from './tourState'
import type { Library } from '../api/types'

const steps = [
  { id: 'load', completes_when: 'corpus_loaded' },
  { id: 'search', completes_when: 'searched' },
  { id: 'ask', completes_when: 'asked' },
]

describe('revoke', () => {
  it('removes_only_the_completions_of_the_named_events', () => {
    const state: TourState = { active: true, index: 1, completed: { load: true, search: true } }
    const out = reduceTour(state, { type: 'revoke', events: ['corpus_loaded'], steps })
    expect(out.completed).toEqual({ search: true })
    expect(reduceTour(out, { type: 'revoke', events: ['corpus_loaded'], steps })).toBe(out)
  })

  it('stale_library_events_are_those_the_data_no_longer_shows', () => {
    expect(staleLibraryEvents(undefined)).toEqual([])
    expect(staleLibraryEvents({ collection: 'halcyon', documents: [] })).toEqual(['corpus_loaded', 'update_applied'])
  })
})

describe('reduceTour', () => {
  it('start_activates_at_the_first_incomplete_step', () => {
    const s = reduceTour({ ...INITIAL_TOUR, completed: { load: true } }, { type: 'start', steps })
    expect(s).toMatchObject({ active: true, index: 1 })
  })

  it('complete_marks_only_the_matching_step', () => {
    const s = reduceTour(INITIAL_TOUR, { type: 'complete', event: 'searched', steps })
    expect(s.completed).toEqual({ search: true })
  })

  it('completing_a_later_step_does_not_move_the_index', () => {
    const s = reduceTour({ active: true, index: 0, completed: {} }, { type: 'complete', event: 'asked', steps })
    expect(s.index).toBe(0)
  })

  it('next_stops_at_the_last_step_and_prev_at_the_first', () => {
    let s: TourState = { active: true, index: 1, completed: {} }
    s = reduceTour(s, { type: 'next', count: 3 })
    s = reduceTour(s, { type: 'next', count: 3 })
    expect(s.index).toBe(2)
    s = reduceTour({ ...s, index: 0 }, { type: 'prev', count: 3 })
    expect(s.index).toBe(0)
    expect(reduceTour(s, { type: 'goto', index: 99, count: 3 }).index).toBe(2)
  })

  it('prev_clamps_a_stale_index_into_range', () => {
    expect(reduceTour({ active: true, index: 40, completed: {} }, { type: 'prev', count: 3 }).index).toBe(2)
  })

  it('dismiss_keeps_progress_and_reset_clears_it', () => {
    const s: TourState = { active: true, index: 2, completed: { load: true } }
    const d = reduceTour(s, { type: 'dismiss' })
    expect(d).toEqual({ active: false, index: 2, completed: { load: true } })
    expect(reduceTour(d, { type: 'reset' })).toEqual(INITIAL_TOUR)
  })
})

describe('persistence', () => {
  it('parseTour_of_garbage_returns_the_initial_state', () => {
    for (const raw of [null, '', '{', 'null', '[]', '{"active":"yes"}', '{"active":true,"index":-1,"completed":{}}', '{"active":true,"index":0,"completed":[]}']) {
      expect(parseTour(raw)).toEqual(INITIAL_TOUR)
    }
  })

  it('round_trips', () => {
    const s: TourState = { active: true, index: 3, completed: { load: true } }
    expect(parseTour(serializeTour(s))).toEqual(s)
  })
})

describe('nextIncomplete', () => {
  it('skips_completed_steps_and_reports_none_when_all_done', () => {
    expect(nextIncomplete({ ...INITIAL_TOUR, completed: { load: true, search: true } }, steps)).toBe(2)
    expect(nextIncomplete({ ...INITIAL_TOUR, completed: { load: true, search: true, ask: true } }, steps)).toBe(-1)
  })
})

describe('libraryEvents', () => {
  const doc = (source_uri: string, versions: number) => ({
    source_uri,
    document_id: source_uri,
    chunks: 1,
    versions: Array.from({ length: versions }, (_, i) => ({ version_num: i + 1, status: 'Active' as const, ingested_at: '', content_hash: '', snapshot_uri: '' })),
  })
  const lib = (docs: ReturnType<typeof doc>[]): Library => ({ collection: 'halcyon', documents: docs })

  it('derives_corpus_loaded_from_ten_documents', () => {
    const nine = Array.from({ length: 9 }, (_, i) => doc(`d${i}.md`, 1))
    expect(libraryEvents(lib(nine))).toEqual([])
    expect(libraryEvents(lib([...nine, doc('d9.md', 1)]))).toEqual(['corpus_loaded'])
  })

  it('derives_update_applied_from_two_policy_versions', () => {
    expect(libraryEvents(lib([doc('security-policy.md', 2)]))).toEqual(['update_applied'])
    expect(libraryEvents(lib([doc('security-policy.md', 1), doc('other.md', 2)]))).toEqual([])
    expect(libraryEvents(undefined)).toEqual([])
  })
})
