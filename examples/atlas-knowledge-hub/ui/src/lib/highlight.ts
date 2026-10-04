export interface Range {
  start: number
  end: number
  kind: string
  id?: string
}

export interface Segment {
  text: string
  kinds: string[]
  ids: string[]
}

const isHigh = (c: number) => c >= 0xd800 && c <= 0xdbff
const isLow = (c: number) => c >= 0xdc00 && c <= 0xdfff

/** True when index `i` falls between the two halves of a surrogate pair. */
function splitsPair(text: string, i: number): boolean {
  return i > 0 && i < text.length && isHigh(text.charCodeAt(i - 1)) && isLow(text.charCodeAt(i))
}

function pushUnique(list: string[], v: string) {
  if (!list.includes(v)) list.push(v)
}

/**
 * Split `text` into ordered, non-overlapping segments covering all of it.
 * Overlapping ranges merge into one segment that carries every kind and id.
 * Touching ranges stay separate. Boundaries never split a surrogate pair.
 */
export function segmentText(text: string, ranges: Range[]): Segment[] {
  const len = text.length
  const clean: Range[] = []
  for (const r of ranges) {
    let start = Math.max(0, Math.min(len, r.start))
    let end = Math.max(0, Math.min(len, r.end))
    if (splitsPair(text, start)) start -= 1
    if (splitsPair(text, end)) end += 1
    if (end > start) clean.push({ ...r, start, end })
  }
  clean.sort((a, b) => a.start - b.start || a.end - b.end)

  const segments: Segment[] = []
  const plain = (from: number, to: number) => {
    if (to > from) segments.push({ text: text.slice(from, to), kinds: [], ids: [] })
  }

  let cursor = 0
  let i = 0
  while (i < clean.length) {
    const first = clean[i]
    let end = first.end
    const kinds: string[] = []
    const ids: string[] = []
    let j = i
    while (j < clean.length && clean[j].start < end) {
      end = Math.max(end, clean[j].end)
      pushUnique(kinds, clean[j].kind)
      const id = clean[j].id
      if (id !== undefined) pushUnique(ids, id)
      j++
    }
    plain(cursor, first.start)
    segments.push({ text: text.slice(first.start, end), kinds, ids })
    cursor = end
    i = j
  }
  plain(cursor, len)
  return segments
}

function escapeRegex(s: string): string {
  return s.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')
}

/** Case-insensitive matches of the query's terms (2 or more characters). */
export function termRanges(text: string, query: string): Range[] {
  const terms = Array.from(new Set(query.split(/\s+/).filter((t) => t.length >= 2).map((t) => t.toLowerCase())))
  if (terms.length === 0) return []
  terms.sort((a, b) => b.length - a.length)
  const re = new RegExp(terms.map(escapeRegex).join('|'), 'gi')
  const out: Range[] = []
  for (const m of text.matchAll(re)) {
    const start = m.index as number
    out.push({ start, end: start + m[0].length, kind: 'term' })
  }
  return out
}
