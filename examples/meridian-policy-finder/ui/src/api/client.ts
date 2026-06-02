export const apiKey =
  import.meta.env.VITE_API_KEY ??
  localStorage.getItem('arcanum_key') ??
  '';

export async function arcanumFetch(path: string, init?: RequestInit): Promise<Response> {
  return fetch(path, {
    ...init,
    headers: {
      Authorization: `Bearer ${apiKey}`,
      'Content-Type': 'application/json',
      ...init?.headers,
    },
  });
}
