export interface CollectionInfo {
  id: string
  name: string
  docCount: number
  chunkCount: number
  lastIngested: string   // ISO 8601
}

const KEY = 'arcanum_collections'

export function getCollections(): CollectionInfo[] {
  try {
    const raw = localStorage.getItem(KEY)
    return raw ? JSON.parse(raw) : []
  } catch {
    return []
  }
}

function save(cols: CollectionInfo[]): void {
  localStorage.setItem(KEY, JSON.stringify(cols))
}

export function upsertCollection(
  name: string,
  delta: { docDelta?: number; chunkDelta?: number },
): void {
  const cols = getCollections()
  const idx = cols.findIndex(c => c.id === name)
  if (idx >= 0) {
    cols[idx].docCount  += delta.docDelta  ?? 0
    cols[idx].chunkCount += delta.chunkDelta ?? 0
    cols[idx].lastIngested = new Date().toISOString()
  } else {
    cols.push({
      id: name,
      name,
      docCount:  delta.docDelta  ?? 0,
      chunkCount: delta.chunkDelta ?? 0,
      lastIngested: new Date().toISOString(),
    })
  }
  save(cols)
}

export function addCollection(name: string): void {
  const cols = getCollections()
  if (!cols.find(c => c.id === name)) {
    cols.push({ id: name, name, docCount: 0, chunkCount: 0, lastIngested: new Date().toISOString() })
    save(cols)
  }
}

export function deleteCollection(name: string): void {
  save(getCollections().filter(c => c.id !== name))
}
