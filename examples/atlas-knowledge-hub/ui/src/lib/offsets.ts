/** Number of UTF-8 bytes needed to encode `s`. */
export function utf8Length(s: string): number {
  let bytes = 0
  for (const ch of s) bytes += codePointBytes(ch.codePointAt(0) as number)
  return bytes
}

function codePointBytes(cp: number): number {
  if (cp < 0x80) return 1
  if (cp < 0x800) return 2
  // Lone surrogates encode as the 3 byte replacement character.
  if (cp < 0x10000) return 3
  return 4
}

/**
 * UTF-16 index of the character that starts at UTF-8 byte offset `byteOffset`.
 * An offset inside a multibyte character snaps down to that character's start;
 * an offset at or past the end returns `text.length`. One linear scan per call.
 */
export function byteToIndex(text: string, byteOffset: number): number {
  if (byteOffset <= 0) return 0
  let bytes = 0
  let i = 0
  while (i < text.length) {
    const cp = text.codePointAt(i) as number
    const width = codePointBytes(cp)
    if (byteOffset < bytes + width) return i
    bytes += width
    i += cp > 0xffff ? 2 : 1
  }
  return text.length
}

export function byteRangeToIndexRange(text: string, start: number, end: number): [number, number] {
  return [byteToIndex(text, start), byteToIndex(text, end)]
}
