export type AnswerPart = { kind: 'text'; text: string } | { kind: 'cite'; ids: string[]; raw: string }

// Same grammar as the server (arcanum-generate/src/citations.rs): one or more
// ids of 1 to 3 digits, comma separated, whitespace allowed inside the brackets.
const GROUP = /\[\s*[PS]\d{1,3}(?:\s*,\s*[PS]\d{1,3})*\s*\]/g
const ID = /[PS]\d{1,3}/g

/** Split an answer into plain text and citation marker groups, in order. */
export function splitAnswer(text: string): AnswerPart[] {
  const parts: AnswerPart[] = []
  let last = 0
  for (const m of text.matchAll(GROUP)) {
    const at = m.index ?? 0
    if (at > last) parts.push({ kind: 'text', text: text.slice(last, at) })
    parts.push({ kind: 'cite', ids: m[0].match(ID) ?? [], raw: m[0] })
    last = at + m[0].length
  }
  if (last < text.length) parts.push({ kind: 'text', text: text.slice(last) })
  return parts
}
