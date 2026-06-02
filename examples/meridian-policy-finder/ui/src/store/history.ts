export interface HistoryEntry {
  question: string
  strategy: string
  topTitle: string
  ts: number
}

const KEY = 'meridian_history'
const MAX_ENTRIES = 50

export function readHistory(): HistoryEntry[] {
  try {
    const raw = localStorage.getItem(KEY)
    return raw ? (JSON.parse(raw) as HistoryEntry[]) : []
  } catch {
    return []
  }
}

export function prependHistory(entry: HistoryEntry): void {
  try {
    const prev = readHistory()
    localStorage.setItem(KEY, JSON.stringify([entry, ...prev].slice(0, MAX_ENTRIES)))
  } catch {
    // Storage quota or permission error — skip silently
  }
}

export function clearHistory(): void {
  localStorage.removeItem(KEY)
}
