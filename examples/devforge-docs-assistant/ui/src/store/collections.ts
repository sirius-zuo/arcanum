const KEY = 'arcanum_known_collections'

export function getKnownCollections(): string[] {
  try {
    const raw = localStorage.getItem(KEY)
    return raw ? JSON.parse(raw) : []
  } catch {
    return []
  }
}

export function rememberCollection(name: string): void {
  const known = getKnownCollections()
  if (!known.includes(name)) {
    localStorage.setItem(KEY, JSON.stringify([...known, name]))
  }
}

export function forgetCollection(name: string): void {
  const known = getKnownCollections().filter(n => n !== name)
  localStorage.setItem(KEY, JSON.stringify(known))
}
