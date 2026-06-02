export async function arcanumFetch(path: string, init?: RequestInit): Promise<Response> {
  return fetch(path, init)
}
